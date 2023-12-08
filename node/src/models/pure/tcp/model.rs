use core::panic;

use crate::{
    automaton::{
        action::{AnyAction, CompletionRoutine, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    models::{
        effectful::mio::action::{MioAction, PollEventsResult, TcpReadResult, TcpWriteResult},
        pure::tcp::{
            action::{Event, ListenerEvent},
            state::{ConnectionType, PollRequest},
        },
    },
};

use super::{
    action::{ConnectionEvent, InitResult, TcpAction, TcpCallbackAction},
    state::{RecvRequest, SendRequest, Status, TcpState},
};

// Callback handlers

fn handle_poll_create(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    events_uid: Uid,
    success: bool,
) {
    assert!(matches!(tcp_state.status, Status::InitPollCreate { .. }));

    if let Status::InitPollCreate {
        init_uid,
        poll_uid,
        ref on_completion,
    } = tcp_state.status
    {
        if success {
            // Dispatch next action to continue initialization
            dispatcher.dispatch(MioAction::EventsCreate {
                uid: events_uid,
                capacity: 1024,
                on_completion: CompletionRoutine::new(|uid| {
                    AnyAction::from(TcpCallbackAction::EventsCreate(uid))
                }),
            });

            // next state
            tcp_state.status = Status::InitEventsCreate {
                init_uid,
                poll_uid,
                events_uid,
                on_completion: on_completion.clone(),
            };
        } else {
            // dispatch error to caller
            dispatcher.completion_dispatch(
                on_completion,
                (init_uid, InitResult::Error("PollCreate failed".to_string())),
            );

            // set init error state
            tcp_state.status = Status::InitError { init_uid };
        }
    }
}

fn handle_events_create(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher) {
    assert!(matches!(tcp_state.status, Status::InitEventsCreate { .. }));

    if let Status::InitEventsCreate {
        init_uid,
        poll_uid,
        events_uid,
        ref on_completion,
    } = tcp_state.status
    {
        dispatcher.completion_dispatch(&on_completion, (init_uid, InitResult::Success));

        tcp_state.status = Status::Ready {
            init_uid,
            poll_uid,
            events_uid,
        };
    }
}

fn handle_listen(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: &Uid,
    result: Result<(), String>,
) {
    dispatcher.completion_dispatch(
        &tcp_state.get_listener(uid).on_completion,
        (*uid, result.clone()),
    );

    if result.is_err() {
        tcp_state.remove_listener(uid);
    }
}

fn handle_accept(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: &Uid,
    result: Result<(), String>,
) {
    let connection = tcp_state.get_connection(uid);
    let ConnectionType::Incoming(listener_uid) = connection.conn_type else {
        panic!(
            "Accept callback on invalid connection type (Uid: {:?}) conn_type: {:?}",
            uid, connection.conn_type
        );
    };

    dispatcher.completion_dispatch(&connection.on_completion, (*uid, result.clone()));

    let events = tcp_state.get_listener_mut(&listener_uid).events_mut();
    assert!(matches!(events, ListenerEvent::AcceptPending));
    *events = ListenerEvent::ConnectionAccepted;

    if result.is_err() {
        tcp_state.remove_connection(uid)
    }
}

fn handle_pending_send_requests(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher) {
    let mut purge_requests = Vec::new();
    let mut dispatched_requests = Vec::new();

    handle_pending_send_requests_aux(
        tcp_state,
        dispatcher,
        &mut purge_requests,
        &mut dispatched_requests,
    );

    let event_reset_list: Vec<Uid> = dispatched_requests
        .iter()
        .map(|uid| {
            let SendRequest {
                connection_uid,
                send_on_poll,
                ..
            } = tcp_state.get_send_request_mut(&uid);
            // we won't handle this request here again, unless the next
            // `TcpWrite` action is interrupted/partial...
            *send_on_poll = false;
            *connection_uid
        })
        .collect();

    for connection_uid in event_reset_list {
        let ConnectionEvent::Ready { send, .. } =
            tcp_state.get_connection_mut(&connection_uid).events_mut()
        else {
            unreachable!()
        };
        // we just dispatched a `TcpWrite` to this connection
        // so "send ready" is no longer true...
        *send = false;
    }

    // remove requests for invalid or closed connections
    for uid in purge_requests.iter() {
        tcp_state.remove_send_request(uid)
    }
}

fn handle_pending_send_requests_aux(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    purge_requests: &mut Vec<Uid>,
    dispatched_requests: &mut Vec<Uid>,
) {
    for (
        &uid,
        SendRequest {
            connection_uid,
            data,
            bytes_sent,
            send_on_poll: _,
            on_completion,
        },
    ) in tcp_state.pending_send_requests()
    {
        let event = tcp_state.get_connection(&connection_uid).events();

        match event {
            ConnectionEvent::Ready { send: true, .. } => {
                dispatcher.dispatch(MioAction::TcpWrite {
                    uid,
                    connection_uid: *connection_uid,
                    data: (&data[*bytes_sent..]).into(),
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpCallbackAction::Send { uid, result })
                    }),
                });

                dispatched_requests.push(uid);
            }
            ConnectionEvent::Ready { send: false, .. } => (),
            ConnectionEvent::Closed => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, Err("Connection closed".to_string())),
                );

                purge_requests.push(uid);
            }
            ConnectionEvent::Error => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, Err("Connection error".to_string())),
                );

                purge_requests.push(uid);
            }
        }
    }
}

fn handle_pending_recv_requests(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher) {
    let mut purge_requests = Vec::new();
    let mut dispatched_requests = Vec::new();

    handle_pending_recv_requests_aux(
        tcp_state,
        dispatcher,
        &mut purge_requests,
        &mut dispatched_requests,
    );

    let event_reset_list: Vec<Uid> = dispatched_requests
        .iter()
        .map(|uid| {
            let RecvRequest {
                connection_uid,
                recv_on_poll,
                ..
            } = tcp_state.get_recv_request_mut(&uid);
            // we won't handle this request here again, unless the next
            // `TcpRead` action is interrupted/partial...
            *recv_on_poll = false;
            *connection_uid
        })
        .collect();

    for connection_uid in event_reset_list {
        let ConnectionEvent::Ready { recv, .. } =
            tcp_state.get_connection_mut(&connection_uid).events_mut()
        else {
            unreachable!()
        };
        // we just dispatched a `TcpRead` to this connection
        // so "recv ready" is no longer true...
        *recv = false;
    }

    // remove requests for invalid or closed connections
    for uid in purge_requests.iter() {
        tcp_state.remove_recv_request(uid)
    }
}

fn handle_pending_recv_requests_aux(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    purge_requests: &mut Vec<Uid>,
    dispatched_requests: &mut Vec<Uid>,
) {
    for (
        &uid,
        RecvRequest {
            connection_uid,
            data,
            bytes_received,
            recv_on_poll: _,
            on_completion,
        },
    ) in tcp_state.pending_recv_requests()
    {
        let event = tcp_state.get_connection(&connection_uid).events();

        match event {
            ConnectionEvent::Ready { recv: true, .. } => {
                dispatcher.dispatch(MioAction::TcpRead {
                    uid,
                    connection_uid: *connection_uid,
                    len: data.len().saturating_sub(*bytes_received),
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpCallbackAction::Recv { uid, result })
                    }),
                });

                dispatched_requests.push(uid);
            }
            ConnectionEvent::Ready { recv: false, .. } => (),
            ConnectionEvent::Closed => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, Err("Connection closed".to_string())),
                );

                purge_requests.push(uid);
            }
            ConnectionEvent::Error => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, Err("Connection error".to_string())),
                );

                purge_requests.push(uid);
            }
        }
    }
}

fn handle_poll(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: PollEventsResult,
) {
    assert!(tcp_state.is_ready());

    match result {
        PollEventsResult::Events(ref events) => {
            // update TCP object events (even for UIDs that were not requested)
            for mio_event in events.iter() {
                tcp_state.update_events(mio_event)
            }

            handle_pending_send_requests(tcp_state, dispatcher);
            handle_pending_recv_requests(tcp_state, dispatcher);

            let request = tcp_state.get_poll_request(&uid);
            // collect events from state for the requested objects
            let events: Vec<(Uid, Event)> = request
                .objects
                .iter()
                .filter_map(|uid| tcp_state.get_events(uid))
                .collect();

            dispatcher.completion_dispatch(&request.on_completion, (uid, Ok(events)));
            tcp_state.remove_poll_request(&uid)
        }
        PollEventsResult::Error(err) => {
            let PollRequest { on_completion, .. } = tcp_state.get_poll_request(&uid);
            dispatcher.completion_dispatch(&on_completion, (uid, Err(err)));
            tcp_state.remove_poll_request(&uid)
        }
        PollEventsResult::Interrupted => {
            // if the syscall was interrupted we re-dispatch the MIO action
            let PollRequest { timeout, .. } = *tcp_state.get_poll_request(&uid);
            let Status::Ready {
                init_uid: _,
                poll_uid,
                events_uid,
            } = tcp_state.status
            else {
                unreachable!()
            };

            dispatcher.dispatch(MioAction::PollEvents {
                uid,
                poll_uid,
                events_uid,
                timeout,
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpCallbackAction::Poll { uid, result })
                }),
            })
        }
    }
}

fn dispatch_send(
    tcp_state: &TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    set_send_on_poll: &mut bool,
) -> bool {
    let SendRequest {
        connection_uid,
        data,
        bytes_sent,
        send_on_poll: _,
        on_completion,
    } = tcp_state.get_send_request(&uid);
    let event = tcp_state.get_connection(connection_uid).events();

    match event {
        ConnectionEvent::Ready { send: true, .. } => {
            dispatcher.dispatch(MioAction::TcpWrite {
                uid,
                connection_uid: *connection_uid,
                data: (&data[*bytes_sent..]).into(),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpCallbackAction::Send { uid, result })
                }),
            });
        }
        ConnectionEvent::Ready { send: false, .. } => *set_send_on_poll = true,
        ConnectionEvent::Closed => {
            // Send failed, notify caller
            dispatcher
                .completion_dispatch(&on_completion, (uid, Err("Connection closed".to_string())));
            return true;
        }
        ConnectionEvent::Error => {
            // Send failed, notify caller
            dispatcher
                .completion_dispatch(&on_completion, (uid, Err("Connection error".to_string())));
            return true;
        }
    }
    return false;
}

fn handle_send(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: TcpWriteResult,
) {
    assert!(tcp_state.is_ready());

    let completed = handle_send_aux(
        uid,
        result,
        tcp_state.get_send_request_mut(&uid),
        dispatcher,
    );
    let mut remove_request = completed;

    // We might need to redispatch if the previous send was incomplete/interrupted.
    if !completed {
        let mut set_send_on_poll = false;
        remove_request = dispatch_send(tcp_state, dispatcher, uid, &mut set_send_on_poll);

        let SendRequest { send_on_poll, .. } = tcp_state.get_send_request_mut(&uid);
        *send_on_poll = set_send_on_poll;
    }

    if remove_request {
        tcp_state.remove_send_request(&uid)
    }
}

fn handle_send_aux(
    uid: Uid,
    result: TcpWriteResult,
    request: &mut SendRequest,
    dispatcher: &mut Dispatcher,
) -> bool {
    match result {
        TcpWriteResult::WrittenAll => {
            // Send complete, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, Ok(())));
            true
        }
        TcpWriteResult::Error(error) => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, Err(error)));
            true
        }
        TcpWriteResult::WrittenPartial(count) => {
            request.bytes_sent += count;
            false
        }
        TcpWriteResult::Interrupted => false,
    }
}

fn dispatch_recv(
    tcp_state: &TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    set_recv_on_poll: &mut bool,
) -> bool {
    let RecvRequest {
        connection_uid,
        data,
        bytes_received,
        recv_on_poll: _,
        on_completion,
    } = tcp_state.get_recv_request(&uid);
    let event = tcp_state.get_connection(connection_uid).events();

    match event {
        ConnectionEvent::Ready { recv: true, .. } => {
            dispatcher.dispatch(MioAction::TcpRead {
                uid,
                connection_uid: *connection_uid,
                len: data.len().saturating_sub(*bytes_received),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpCallbackAction::Recv { uid, result })
                }),
            });
        }
        ConnectionEvent::Ready { recv: false, .. } => *set_recv_on_poll = true,
        ConnectionEvent::Closed => {
            // Send failed, notify caller
            dispatcher
                .completion_dispatch(&on_completion, (uid, Err("Connection closed".to_string())));
            return true;
        }
        ConnectionEvent::Error => {
            // Send failed, notify caller
            dispatcher
                .completion_dispatch(&on_completion, (uid, Err("Connection error".to_string())));
            return true;
        }
    }
    return false;
}

fn handle_recv(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: TcpReadResult,
) {
    assert!(tcp_state.is_ready());

    let completed = handle_recv_aux(
        uid,
        result,
        tcp_state.get_recv_request_mut(&uid),
        dispatcher,
    );
    let mut remove_request = completed;

    // We might need to redispatch if the previous recv was incomplete/interrupted.
    if !completed {
        let mut set_recv_on_poll = false;
        remove_request = dispatch_recv(tcp_state, dispatcher, uid, &mut set_recv_on_poll);

        let RecvRequest { recv_on_poll, .. } = tcp_state.get_recv_request_mut(&uid);
        *recv_on_poll = set_recv_on_poll;
    }

    if remove_request {
        tcp_state.remove_recv_request(&uid)
    }
}

fn handle_recv_aux(
    uid: Uid,
    result: TcpReadResult,
    request: &mut RecvRequest,
    dispatcher: &mut Dispatcher,
) -> bool {
    match result {
        TcpReadResult::ReadAll(data) => {
            // Send complete, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, Ok(data)));
            true
        }
        TcpReadResult::Error(error) => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, Err(error)));
            true
        }
        TcpReadResult::ReadPartial(data) => {
            let start_offset = request.bytes_received;
            let end_offset = start_offset + data.len();
            request.data[start_offset..end_offset].copy_from_slice(&data[..]);
            request.bytes_received = end_offset;
            false
        }
        TcpReadResult::Interrupted => false,
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
                let events_uid = state.new_uid();
                handle_poll_create(state.models.state_mut(), dispatcher, events_uid, success)
            }
            TcpCallbackAction::EventsCreate(_uid) => {
                handle_events_create(state.models.state_mut(), dispatcher)
            }
            TcpCallbackAction::Listen { uid, result } => {
                handle_listen(state.models.state_mut(), dispatcher, &uid, result)
            }
            TcpCallbackAction::Accept { uid, result } => {
                handle_accept(state.models.state_mut(), dispatcher, &uid, result)
            }
            TcpCallbackAction::Poll { uid, result } => {
                handle_poll(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpCallbackAction::Send { uid, result } => {
                handle_send(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpCallbackAction::Recv { uid, result } => {
                handle_recv(state.models.state_mut(), dispatcher, uid, result)
            }
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
                init_uid,
                on_completion,
            } => {
                let poll_uid = state.new_uid();
                let tcp_state: &mut TcpState = state.models.state_mut();

                tcp_state.status = Status::InitPollCreate {
                    init_uid,
                    poll_uid,
                    on_completion,
                };
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
                let tcp_state: &mut TcpState = state.models.state_mut();
                assert!(tcp_state.is_ready());

                tcp_state.new_listener(uid, address.clone(), on_completion);
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
                listener_uid,
                on_completion,
            } => {
                let tcp_state: &mut TcpState = state.models.state_mut();
                assert!(tcp_state.is_ready());
                assert!(matches!(
                    tcp_state.get_listener(&listener_uid).events(),
                    ListenerEvent::AcceptPending
                ));
                let conn_type = ConnectionType::Incoming(listener_uid);

                tcp_state.new_connection(uid, conn_type, on_completion);
                dispatcher.dispatch(MioAction::TcpAccept {
                    uid,
                    listener_uid,
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
                let tcp_state: &mut TcpState = state.models.state_mut();
                let Status::Ready {
                    init_uid: _,
                    poll_uid,
                    events_uid,
                } = tcp_state.status
                else {
                    unreachable!()
                };

                tcp_state.new_poll(uid, objects, timeout, on_completion);
                dispatcher.dispatch(MioAction::PollEvents {
                    uid,
                    poll_uid,
                    events_uid,
                    timeout,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpCallbackAction::Poll { uid, result })
                    }),
                })
            }
            TcpAction::Send {
                uid,
                connection_uid,
                data,
                on_completion,
            } => {
                let tcp_state: &mut TcpState = state.models.state_mut();
                assert!(tcp_state.is_ready());

                let mut set_send_on_poll = false;

                tcp_state.new_send_request(
                    uid,
                    connection_uid,
                    data,
                    set_send_on_poll,
                    on_completion.clone(),
                );

                let remove_request =
                    dispatch_send(tcp_state, dispatcher, uid, &mut set_send_on_poll);

                let SendRequest { send_on_poll, .. } = tcp_state.get_send_request_mut(&uid);
                *send_on_poll = set_send_on_poll;

                if remove_request {
                    tcp_state.remove_send_request(&uid)
                }
            }
            TcpAction::Recv {
                uid,
                connection_uid,
                count,
                on_completion,
            } => {
                let tcp_state: &mut TcpState = state.models.state_mut();
                assert!(tcp_state.is_ready());

                let mut set_recv_on_poll = false;

                tcp_state.new_recv_request(
                    uid,
                    connection_uid,
                    count,
                    set_recv_on_poll,
                    on_completion.clone(),
                );

                let remove_request =
                    dispatch_recv(tcp_state, dispatcher, uid, &mut set_recv_on_poll);

                let RecvRequest { recv_on_poll, .. } = tcp_state.get_recv_request_mut(&uid);
                *recv_on_poll = set_recv_on_poll;

                if remove_request {
                    tcp_state.remove_recv_request(&uid)
                }
            }
        }
    }
}
