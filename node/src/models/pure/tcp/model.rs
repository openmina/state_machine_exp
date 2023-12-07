use crate::{
    automaton::{
        action::{AnyAction, CompletionRoutine, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    models::{
        effectful::mio::action::{MioAction, PollEventsResult, TcpWriteResult},
        pure::tcp::{
            action::{Event, PollResult},
            state::{ConnectionEvent, ConnectionType},
        },
    },
};

use super::{
    action::{InitResult, TcpAction, TcpCallbackAction},
    state::{Status, TcpState},
};

// Callback handlers

fn handle_poll_create<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    success: bool,
) {
    let events_uid = state.new_uid();
    let this: &mut TcpState = state.models.state_mut();
    let init_uid = this.status.init_uid();
    let on_completion = this.status.init_completion_routine();

    if success {
        // Dispatch next action to continue initialization
        this.status.set_init_events(events_uid);
        dispatcher.dispatch(MioAction::EventsCreate {
            uid: events_uid,
            capacity: 1024,
            on_completion: CompletionRoutine::new(|uid| {
                AnyAction::from(TcpCallbackAction::EventsCreate(uid))
            }),
        })
    } else {
        // Otherwise dispatch error to caller
        this.status.set_init_error(init_uid);
        dispatcher.completion_dispatch(
            &on_completion,
            (init_uid, InitResult::Error("PollCreate failed".to_string())),
        );
    }
}

fn handle_events_create<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
) {
    let this: &mut TcpState = state.models.state_mut();
    let init_uid = this.status.init_uid();
    let on_completion = this.status.init_completion_routine();

    this.status.set_init_ready();
    dispatcher.completion_dispatch(&on_completion, (init_uid, InitResult::Success));
}

fn handle_listen<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: Result<(), String>,
) {
    let this: &mut TcpState = state.models.state_mut();

    dispatcher.completion_dispatch(
        &this.obj_as_listener(uid).on_completion,
        (uid, result.clone()),
    );

    if result.is_err() {
        this.remove_obj(uid);
    }
}

fn handle_accept<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: Result<(), String>,
) {
    let this: &mut TcpState = state.models.state_mut();

    {
        let connection = this.obj_as_connection(uid);
        assert!(matches!(connection.conn_type, ConnectionType::Incoming));

        dispatcher.completion_dispatch(&connection.on_completion, (uid, result.clone()));
    }

    if result.is_err() {
        this.remove_obj(uid)
    }
}

fn handle_pending_send_requests(this: &mut TcpState, dispatcher: &mut Dispatcher) {
    for uid in this.pending_send_requests() {
        let mut remove_obj = false;
        {
            let mut request = this.obj_as_send_request_mut(uid);
            let mut connection = this.obj_as_connection_mut(request.connection);

            if let Some(Event::Connection(event)) = connection.get_events() {
                match event {
                    ConnectionEvent::Ready { send: true, recv } => {
                        // remove send event since we might report events back to caller
                        connection.set_events(ConnectionEvent::Ready { send: false, recv });
                        // don't handle it again unless the `TcpWrite`` action dispatched next gets
                        // interrupted or does a partial write.
                        request.send_on_poll = false;
                        dispatcher.dispatch(MioAction::TcpWrite {
                            uid,
                            connection: request.connection,
                            data: (&request.data[request.bytes_sent..]).into(),
                            on_completion: CompletionRoutine::new(|(uid, result)| {
                                AnyAction::from(TcpCallbackAction::Send { uid, result })
                            }),
                        })
                    }
                    ConnectionEvent::Ready { send: false, .. } => (),
                    ConnectionEvent::Closed => {
                        dispatcher.completion_dispatch(
                            &request.on_completion,
                            (uid, Err("Connection closed".to_string())),
                        );
                        remove_obj = true;
                    }
                    ConnectionEvent::Error => {
                        dispatcher.completion_dispatch(
                            &request.on_completion,
                            (uid, Err("Connection error".to_string())),
                        );
                        remove_obj = true;
                    }
                }
            } else {
                unreachable!()
            }
        }

        if remove_obj {
            this.remove_obj(uid)
        }
    }
}

fn handle_poll<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: PollEventsResult,
) {
    let this: &mut TcpState = state.models.state_mut();
    assert!(matches!(this.status, Status::Ready { .. }));

    let mut remove_obj = true;

    match result {
        PollEventsResult::Events(ref events) => {
            // update TCP object events (even for UIDs that were not requested)
            for mio_event in events.iter() {
                this.add_obj_event(mio_event)
            }

            handle_pending_send_requests(this, dispatcher);

            let request = this.obj_as_poll_request(uid);
            // collect events from state for the requested objects
            let mut events = Vec::new();

            for obj_uid in request.objects.iter().cloned() {
                if let Some(event) = this.get_obj_events(obj_uid) {
                    events.push((obj_uid, event))
                }
            }

            dispatcher
                .completion_dispatch(&request.on_completion, (uid, PollResult::Events(events)));
        }
        PollEventsResult::Error(err) => dispatcher.completion_dispatch(
            &this.obj_as_poll_request(uid).on_completion,
            (uid, PollResult::Error(err)),
        ),
        PollEventsResult::Interrupted => {
            // if the syscall was interrupted we re-dispatch the MIO action
            let poll = this.status.poll_uid();
            let events = this.status.events_uid();

            remove_obj = false;
            dispatcher.dispatch(MioAction::PollEvents {
                uid,
                poll,
                events,
                timeout: this.obj_as_poll_request(uid).timeout,
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpCallbackAction::Poll { uid, result })
                }),
            })
        }
    }

    if remove_obj {
        this.remove_obj(uid)
    }
}

fn handle_send<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: TcpWriteResult,
) {
    let this: &mut TcpState = state.models.state_mut();
    assert!(matches!(this.status, Status::Ready { .. }));

    let mut remove_obj = false;

    {
        let mut redispatch = false;
        let mut request = this.obj_as_send_request_mut(uid);

        match result {
            TcpWriteResult::WrittenAll => {
                // Send complete, notify caller
                dispatcher.completion_dispatch(&request.on_completion, (uid, Ok(())));
                remove_obj = true;
            }
            TcpWriteResult::Error(error) => {
                // Send failed, notify caller
                dispatcher.completion_dispatch(&request.on_completion, (uid, Err(error)));
                remove_obj = true;
            }
            TcpWriteResult::WrittenPartial(count) => {
                request.bytes_sent += count;
                redispatch = true;
            }
            TcpWriteResult::Interrupted => redispatch = true,
        }

        // Previous send was interrupted or partial, re-send `TcpWrite`
        if redispatch {
            // Check connection status before redispatching
            if let Some(Event::Connection(event)) =
                this.obj_as_connection(request.connection).get_events()
            {
                match event {
                    ConnectionEvent::Ready { send: true, .. } => {
                        request.send_on_poll = false;
                        dispatcher.dispatch(MioAction::TcpWrite {
                            uid,
                            connection: request.connection,
                            data: (&request.data[request.bytes_sent..]).into(),
                            on_completion: CompletionRoutine::new(|(uid, result)| {
                                AnyAction::from(TcpCallbackAction::Send { uid, result })
                            }),
                        })
                    }
                    ConnectionEvent::Ready { send: false, .. } => request.send_on_poll = true,
                    ConnectionEvent::Closed => {
                        // Send failed, notify caller
                        dispatcher.completion_dispatch(
                            &request.on_completion,
                            (uid, Err("Connection closed".to_string())),
                        );
                        remove_obj = true;
                    }
                    ConnectionEvent::Error => {
                        // Send failed, notify caller
                        dispatcher.completion_dispatch(
                            &request.on_completion,
                            (uid, Err("Connection error".to_string())),
                        );
                        remove_obj = true;
                    }
                }
            } else {
                unreachable!()
            }
        }
    }

    if remove_obj {
        this.remove_obj(uid)
    }
}

impl InputModel for TcpState {
    type Action = TcpCallbackAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpCallbackAction::PollCreate { uid: _, success } => {
                handle_poll_create(state, dispatcher, success)
            }
            TcpCallbackAction::EventsCreate(_uid) => handle_events_create(state, dispatcher),
            TcpCallbackAction::Listen { uid, result } => {
                handle_listen(state, dispatcher, uid, result)
            }
            TcpCallbackAction::Accept { uid, result } => {
                handle_accept(state, dispatcher, uid, result)
            }
            TcpCallbackAction::Poll { uid, result } => handle_poll(state, dispatcher, uid, result),
            TcpCallbackAction::Send { uid, result } => handle_send(state, dispatcher, uid, result),
        }
    }
}

impl PureModel for TcpState {
    type Action = TcpAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpAction::Init {
                uid: init_uid,
                on_completion,
            } => {
                let poll_uid = state.new_uid();
                let this: &mut TcpState = state.models.state_mut();

                this.status.set_init_poll(init_uid, poll_uid, on_completion);
                dispatcher.dispatch(MioAction::PollCreate {
                    uid: poll_uid,
                    on_completion: CompletionRoutine::new(|(uid, success)| {
                        AnyAction::from(TcpCallbackAction::PollCreate { uid, success })
                    }),
                });
            }
            TcpAction::Listen {
                uid,
                address,
                on_completion,
            } => {
                let this: &mut TcpState = state.models.state_mut();
                assert!(matches!(this.status, Status::Ready { .. }));

                this.new_listener(uid, address.clone(), on_completion);
                dispatcher.dispatch(MioAction::TcpListen {
                    uid,
                    address,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpCallbackAction::Listen { uid, result })
                    }),
                });
            }
            TcpAction::Accept {
                uid,
                listener,
                on_completion,
            } => {
                let this: &mut TcpState = state.models.state_mut();
                assert!(matches!(this.status, Status::Ready { .. }));

                this.new_incoming_connection(uid, on_completion);
                dispatcher.dispatch(MioAction::TcpAccept {
                    uid,
                    listener,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpCallbackAction::Accept { uid, result })
                    }),
                });
            }
            TcpAction::Poll {
                uid,
                objects,
                timeout,
                on_completion,
            } => {
                let this: &mut TcpState = state.models.state_mut();
                assert!(matches!(this.status, Status::Ready { .. }));

                let poll = this.status.poll_uid();
                let events = this.status.events_uid();

                this.new_poll(uid, objects, timeout, on_completion);
                dispatcher.dispatch(MioAction::PollEvents {
                    uid,
                    poll,
                    events,
                    timeout,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpCallbackAction::Poll { uid, result })
                    }),
                })
            }
            TcpAction::Send {
                uid,
                connection,
                data,
                on_completion,
            } => {
                let this: &mut TcpState = state.models.state_mut();
                let mut send_on_poll = false;

                assert!(matches!(this.status, Status::Ready { .. }));

                if let Some(Event::Connection(event)) =
                    this.obj_as_connection(connection).get_events()
                {
                    match event {
                        ConnectionEvent::Ready { send: true, .. } => {
                            // If connection is ready, send it now
                            dispatcher.dispatch(MioAction::TcpWrite {
                                uid,
                                connection,
                                data: data.clone(),
                                on_completion: CompletionRoutine::new(|(uid, result)| {
                                    AnyAction::from(TcpCallbackAction::Send { uid, result })
                                }),
                            });
                        }
                        ConnectionEvent::Ready { send: false, .. } => {
                            // otherwise wait for `handle_pending_send_requests` to take care of it
                            send_on_poll = true;
                        }
                        // Bailout cases: notify caller (avoids `SendRequest` object creation).
                        ConnectionEvent::Closed => {
                            dispatcher.completion_dispatch(
                                &on_completion,
                                (uid, Err("Connection closed".to_string())),
                            );
                            return;
                        }
                        ConnectionEvent::Error => {
                            dispatcher.completion_dispatch(
                                &on_completion,
                                (uid, Err("Connection error".to_string())),
                            );
                            return;
                        }
                    };
                }

                this.new_send_request(uid, connection, data, send_on_poll, on_completion.clone());
            }
            TcpAction::Recv {
                uid,
                connection,
                count,
                on_completion,
            } => todo!(),
        }
    }
}
