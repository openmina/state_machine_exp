use core::panic;
use std::rc::Rc;

use crate::{
    automaton::{
        action::{AnyAction, CompletionRoutine, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    models::{
        effectful::mio::action::{
            MioOutputAction, PollEventsResult, TcpReadResult, TcpWriteResult,
        },
        pure::{
            tcp::{
                action::{Event, ListenerEvent},
                state::{Connection, ConnectionType, PollRequest},
            },
            time::state::TimeState,
        },
    },
};

use super::{
    action::{
        ConnectResult, ConnectionEvent, PollResult, RecvResult, TcpInputAction, TcpPureAction,
    },
    state::{ConnectionStatus, Listener, RecvRequest, SendRequest, SendResult, Status, TcpState},
};

// Input action handlers

fn input_poll_create(
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
            dispatcher.dispatch(MioOutputAction::EventsCreate {
                uid: events_uid,
                capacity: 1024,
                on_completion: CompletionRoutine::new(|uid| {
                    AnyAction::from(TcpInputAction::EventsCreate(uid))
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
                (init_uid, Err("PollCreate failed".to_string())),
            );

            // set init error state
            tcp_state.status = Status::InitError { init_uid };
        }
    }
}

fn input_events_create(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher) {
    assert!(matches!(tcp_state.status, Status::InitEventsCreate { .. }));

    if let Status::InitEventsCreate {
        init_uid,
        poll_uid,
        events_uid,
        ref on_completion,
    } = tcp_state.status
    {
        dispatcher.completion_dispatch(&on_completion, (init_uid, Ok(())));

        tcp_state.status = Status::Ready {
            init_uid,
            poll_uid,
            events_uid,
        };
    }
}

fn input_listen(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: &Uid,
    result: Result<(), String>,
) {
    if result.is_ok() {
        // If the listen operation was successful we register the listener in the MIO poll object.
        let Status::Ready { poll_uid, .. } = tcp_state.status else {
            unreachable!()
        };

        // We will dispath the completion routine to the caller from `input_register_listener`
        dispatcher.dispatch(MioOutputAction::PollRegisterTcpServer {
            poll_uid,
            tcp_listener_uid: *uid,
            on_completion: CompletionRoutine::new(|(uid, result)| {
                AnyAction::from(TcpInputAction::RegisterListener { uid, result })
            }),
        });
    } else {
        dispatcher.completion_dispatch(&tcp_state.get_listener(uid).on_completion, (*uid, result));
        tcp_state.remove_listener(uid);
    }
}

fn input_register_listener(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: &Uid,
    result: bool,
) {
    let Listener { on_completion, .. } = tcp_state.get_listener(uid);

    if result {
        dispatcher.completion_dispatch(&on_completion, (*uid, Ok(())));
    } else {
        let error = format!("Error registering listener {:?}", uid);
        dispatcher.completion_dispatch(&on_completion, (*uid, Err(error)));
        tcp_state.remove_listener(uid)
    }
}

fn input_accept(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: &Uid,
    result: Result<(), String>,
) {
    let Connection {
        conn_type,
        on_completion,
        ..
    } = tcp_state.get_connection(uid);
    let ConnectionType::Incoming(listener_uid) = conn_type else {
        panic!(
            "Accept callback on invalid connection type (Uid: {:?}) conn_type: {:?}",
            uid, conn_type
        );
    };
    let mut remove = false;

    match result {
        Ok(()) => {
            // If the connection accept was successful we register it in the MIO poll object.
            let Status::Ready { poll_uid, .. } = tcp_state.status else {
                unreachable!()
            };

            // We will dispath the completion routine to the caller from `input_register_connection`
            dispatcher.dispatch(MioOutputAction::PollRegisterTcpConnection {
                poll_uid,
                connection_uid: *uid,
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpInputAction::RegisterConnection { uid, result })
                }),
            });
        }
        Err(error) => {
            // Dispatch error result now
            dispatcher.completion_dispatch(&on_completion, (*uid, ConnectResult::Error(error)));
            remove = true;
        }
    }

    let listener_uid = *listener_uid;
    let events = tcp_state.get_listener_mut(&listener_uid).events_mut();
    assert!(matches!(events, ListenerEvent::AcceptPending));
    *events = ListenerEvent::ConnectionAccepted;

    if remove {
        tcp_state.remove_connection(uid)
    }
}

fn input_close_connection(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, uid: Uid) {
    let connection = tcp_state.get_connection(&uid);

    let Connection {
        status: ConnectionStatus::CloseRequest(maybe_completion),
        ..
    } = connection
    else {
        panic!(
            "Close callback called on connection (Uid: {:?}) with invalid status {:?}",
            uid, connection.status
        )
    };

    if let Some(on_completion) = maybe_completion {
        dispatcher.completion_dispatch(&on_completion, uid);
    }

    tcp_state.remove_connection(&uid);
}

fn input_register_connection(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection_uid: Uid,
    result: bool,
) {
    let Connection {
        status,
        on_completion,
        ..
    } = tcp_state.get_connection_mut(&connection_uid);

    if !result {
        *status = ConnectionStatus::CloseRequest(None);
        dispatcher.dispatch(MioOutputAction::TcpClose {
            connection_uid,
            on_completion: CompletionRoutine::new(|uid| {
                AnyAction::from(TcpInputAction::CloseConnection { uid })
            }),
        });

        let error = format!("Error registering connection {:?}", connection_uid);
        dispatcher.completion_dispatch(
            &on_completion,
            (connection_uid, ConnectResult::Error(error)),
        );
    }
}

fn input_deregister_connection(
    _tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: bool,
) {
    if result {
        dispatcher.dispatch(MioOutputAction::TcpClose {
            connection_uid: uid,
            on_completion: CompletionRoutine::new(|uid| {
                AnyAction::from(TcpInputAction::CloseConnection { uid })
            }),
        });
    } else {
        panic!("Error de-registering connection {:?}", uid)
    }
}

fn input_connect(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: &Uid,
    result: Result<(), String>,
) {
    let Connection {
        conn_type,
        on_completion,
        ..
    } = tcp_state.get_connection(uid);

    assert!(matches!(conn_type, ConnectionType::Outgoing));

    match result {
        Ok(()) => {
            let Status::Ready { poll_uid, .. } = tcp_state.status else {
                unreachable!()
            };

            // We will dispath the completion routine to the caller from `input_register_connection`
            dispatcher.dispatch(MioOutputAction::PollRegisterTcpConnection {
                poll_uid,
                connection_uid: *uid,
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpInputAction::RegisterConnection { uid, result })
                }),
            });
        }
        Err(error) => {
            dispatcher.completion_dispatch(&on_completion, (*uid, ConnectResult::Error(error)));
            tcp_state.remove_connection(uid);
        }
    }
}

fn input_pending_connections(
    current_time: u128,
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
) {
    let mut purge_requests = Vec::new();

    for (
        &uid,
        Connection {
            status,
            timeout,
            on_completion,
            ..
        },
    ) in tcp_state.pending_connections_mut()
    {
        let timeout = timeout.is_some_and(|timeout| current_time >= timeout);

        if timeout {
            dispatcher.completion_dispatch(&on_completion, (uid, ConnectResult::Timeout));
            purge_requests.push(uid);
        } else {
            match status {
                ConnectionStatus::Pending => {
                    dispatcher.dispatch(MioOutputAction::TcpGetPeerAddress {
                        connection_uid: uid,
                        on_completion: CompletionRoutine::new(|(uid, result)| {
                            AnyAction::from(TcpInputAction::PeerAddress { uid, result })
                        }),
                    });
                    *status = ConnectionStatus::PendingCheck;
                }
                ConnectionStatus::PendingCheck => (),
                _ => unreachable!(),
            }
        }
    }
}

fn input_pending_send_requests(
    current_time: u128,
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
) {
    let mut purge_requests = Vec::new();
    let mut dispatched_requests = Vec::new();

    input_pending_send_requests_aux(
        current_time,
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

fn input_pending_send_requests_aux(
    current_time: u128,
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
            timeout,
            on_completion,
        },
    ) in tcp_state.pending_send_requests()
    {
        let timeout = timeout.is_some_and(|timeout| current_time >= timeout);
        let event = tcp_state.get_connection(&connection_uid).events();

        match event {
            ConnectionEvent::Ready { send: true, .. } => {
                if timeout {
                    dispatcher.completion_dispatch(&on_completion, (uid, SendResult::Timeout));
                    purge_requests.push(uid);
                } else {
                    dispatcher.dispatch(MioOutputAction::TcpWrite {
                        uid,
                        connection_uid: *connection_uid,
                        data: (&data[*bytes_sent..]).into(),
                        on_completion: CompletionRoutine::new(|(uid, result)| {
                            AnyAction::from(TcpInputAction::Send { uid, result })
                        }),
                    });

                    dispatched_requests.push(uid);
                }
            }
            ConnectionEvent::Ready { send: false, .. } => {
                if timeout {
                    dispatcher.completion_dispatch(&on_completion, (uid, SendResult::Timeout));
                    purge_requests.push(uid);
                }
            }
            ConnectionEvent::Closed => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, SendResult::Error("Connection closed".to_string())),
                );

                purge_requests.push(uid);
            }
            ConnectionEvent::Error => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, SendResult::Error("Connection error".to_string())),
                );

                purge_requests.push(uid);
            }
        }
    }
}

fn input_pending_recv_requests(
    current_time: u128,
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
) {
    let mut purge_requests = Vec::new();
    let mut dispatched_requests = Vec::new();

    input_pending_recv_requests_aux(
        current_time,
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

fn input_pending_recv_requests_aux(
    current_time: u128,
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
            timeout,
            on_completion,
        },
    ) in tcp_state.pending_recv_requests()
    {
        let timeout = timeout.is_some_and(|timeout| current_time >= timeout);
        let event = tcp_state.get_connection(&connection_uid).events();

        match event {
            ConnectionEvent::Ready { recv: true, .. } => {
                if timeout {
                    dispatcher.completion_dispatch(
                        &on_completion,
                        (uid, RecvResult::Timeout(data[0..*bytes_received].to_vec())),
                    );
                    purge_requests.push(uid);
                } else {
                    dispatcher.dispatch(MioOutputAction::TcpRead {
                        uid,
                        connection_uid: *connection_uid,
                        len: data.len().saturating_sub(*bytes_received),
                        on_completion: CompletionRoutine::new(|(uid, result)| {
                            AnyAction::from(TcpInputAction::Recv { uid, result })
                        }),
                    });

                    dispatched_requests.push(uid);
                }
            }
            ConnectionEvent::Ready { recv: false, .. } => {
                if timeout {
                    dispatcher.completion_dispatch(
                        &on_completion,
                        (uid, RecvResult::Timeout(data[0..*bytes_received].to_vec())),
                    );
                    purge_requests.push(uid);
                }
            }
            ConnectionEvent::Closed => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, RecvResult::Error("Connection closed".to_string())),
                );

                purge_requests.push(uid);
            }
            ConnectionEvent::Error => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, RecvResult::Error("Connection error".to_string())),
                );

                purge_requests.push(uid);
            }
        }
    }
}

fn input_poll(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
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

            input_pending_connections(current_time, tcp_state, dispatcher);
            input_pending_send_requests(current_time, tcp_state, dispatcher);
            input_pending_recv_requests(current_time, tcp_state, dispatcher);

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

            dispatcher.dispatch(MioOutputAction::PollEvents {
                uid,
                poll_uid,
                events_uid,
                timeout,
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpInputAction::Poll { uid, result })
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
        on_completion,
        ..
    } = tcp_state.get_send_request(&uid);
    let event = tcp_state.get_connection(connection_uid).events();

    match event {
        ConnectionEvent::Ready { send: true, .. } => {
            dispatcher.dispatch(MioOutputAction::TcpWrite {
                uid,
                connection_uid: *connection_uid,
                data: (&data[*bytes_sent..]).into(),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpInputAction::Send { uid, result })
                }),
            });
        }
        ConnectionEvent::Ready { send: false, .. } => {
            // TODO: check timeout and dispatch caller
            *set_send_on_poll = true;
        }
        ConnectionEvent::Closed => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(
                &on_completion,
                (uid, SendResult::Error("Connection closed".to_string())),
            );
            return true;
        }
        ConnectionEvent::Error => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(
                &on_completion,
                (uid, SendResult::Error("Connection error".to_string())),
            );
            return true;
        }
    }
    return false;
}

fn input_send(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    result: TcpWriteResult,
) {
    assert!(tcp_state.is_ready());

    let completed = input_send_aux(
        current_time,
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

fn input_send_aux(
    current_time: u128,
    uid: Uid,
    result: TcpWriteResult,
    request: &mut SendRequest,
    dispatcher: &mut Dispatcher,
) -> bool {
    let timeout = request
        .timeout
        .is_some_and(|timeout| current_time >= timeout);

    match result {
        // if there was a timeout but we already written all or got an error we will let it pass..
        TcpWriteResult::WrittenAll => {
            // Send complete, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, SendResult::Success));
            true
        }
        TcpWriteResult::Error(error) => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, SendResult::Error(error)));
            true
        }
        TcpWriteResult::WrittenPartial(count) => {
            if timeout {
                dispatcher.completion_dispatch(&request.on_completion, (uid, SendResult::Timeout));
                true
            } else {
                request.bytes_sent += count;
                false
            }
        }
        TcpWriteResult::Interrupted => {
            if timeout {
                dispatcher.completion_dispatch(&request.on_completion, (uid, SendResult::Timeout));
                true
            } else {
                false
            }
        }
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
        on_completion,
        ..
    } = tcp_state.get_recv_request(&uid);
    let event = tcp_state.get_connection(connection_uid).events();

    match event {
        ConnectionEvent::Ready { recv: true, .. } => {
            dispatcher.dispatch(MioOutputAction::TcpRead {
                uid,
                connection_uid: *connection_uid,
                len: data.len().saturating_sub(*bytes_received),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(TcpInputAction::Recv { uid, result })
                }),
            });
        }
        ConnectionEvent::Ready { recv: false, .. } => {
            // TODO: check timeouts and dispatch caller
            *set_recv_on_poll = true;
        }
        ConnectionEvent::Closed => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(
                &on_completion,
                (uid, RecvResult::Error("Connection closed".to_string())),
            );
            return true;
        }
        ConnectionEvent::Error => {
            // Send failed, notify caller
            dispatcher.completion_dispatch(
                &on_completion,
                (uid, RecvResult::Error("Connection error".to_string())),
            );
            return true;
        }
    }
    return false;
}

fn input_recv(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    result: TcpReadResult,
) {
    assert!(tcp_state.is_ready());

    let completed = input_recv_aux(
        current_time,
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

fn input_recv_aux(
    current_time: u128,
    uid: Uid,
    result: TcpReadResult,
    request: &mut RecvRequest,
    dispatcher: &mut Dispatcher,
) -> bool {
    let timeout = request
        .timeout
        .is_some_and(|timeout| current_time >= timeout);

    match result {
        // if there was a timeout but we recevied all data or there was an error we let it pass...
        TcpReadResult::ReadAll(data) => {
            // Recv complete, notify caller
            dispatcher
                .completion_dispatch(&request.on_completion, (uid, RecvResult::Success(data)));
            true
        }
        TcpReadResult::Error(error) => {
            // Recv failed, notify caller
            dispatcher.completion_dispatch(&request.on_completion, (uid, RecvResult::Error(error)));
            true
        }
        TcpReadResult::ReadPartial(data) => {
            if timeout {
                dispatcher
                    .completion_dispatch(&request.on_completion, (uid, RecvResult::Timeout(data)));
                true
            } else {
                let start_offset = request.bytes_received;
                let end_offset = start_offset + data.len();
                request.data[start_offset..end_offset].copy_from_slice(&data[..]);
                request.bytes_received = end_offset;
                false
            }
        }
        TcpReadResult::Interrupted => {
            if timeout {
                let data = request.data[0..request.bytes_received].to_vec();
                dispatcher
                    .completion_dispatch(&request.on_completion, (uid, RecvResult::Timeout(data)));
                true
            } else {
                false
            }
        }
    }
}

fn input_peer_address(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: Result<String, String>,
) {
    let Connection {
        status,
        on_completion,
        ..
    } = tcp_state.get_connection_mut(&uid);
    let mut remove = false;

    if let ConnectionStatus::PendingCheck = status {
        let result = match result {
            Ok(_) => {
                *status = ConnectionStatus::Established;
                ConnectResult::Success
            }
            Err(error) => {
                remove = true;
                ConnectResult::Error(error)
            }
        };

        dispatcher.completion_dispatch(on_completion, (uid, result));

        if remove {
            tcp_state.remove_connection(&uid)
        }
    } else {
        panic!(
            "PeerAddress action received for connection (uid: {:?}) with wrong status {:?}",
            uid, status
        )
    }
}

impl InputModel for TcpState {
    type Action = TcpInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpInputAction::PollCreate { uid: _, success } => {
                let events_uid = state.new_uid();
                input_poll_create(state.models.state_mut(), dispatcher, events_uid, success)
            }
            TcpInputAction::EventsCreate(_uid) => {
                input_events_create(state.models.state_mut(), dispatcher)
            }
            TcpInputAction::Listen { uid, result } => {
                input_listen(state.models.state_mut(), dispatcher, &uid, result)
            }
            TcpInputAction::Accept { uid, result } => {
                input_accept(state.models.state_mut(), dispatcher, &uid, result)
            }
            TcpInputAction::Connect { uid, result } => {
                input_connect(state.models.state_mut(), dispatcher, &uid, result)
            }
            TcpInputAction::CloseConnection { uid } => {
                input_close_connection(state.models.state_mut(), dispatcher, uid)
            }
            TcpInputAction::RegisterConnection { uid, result } => {
                input_register_connection(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpInputAction::DeregisterConnection { uid, result } => {
                input_deregister_connection(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpInputAction::RegisterListener { uid, result } => {
                input_register_listener(state.models.state_mut(), dispatcher, &uid, result)
            }
            TcpInputAction::Poll { uid, result } => {
                let current_time = get_current_time(state);

                input_poll(
                    state.models.state_mut(),
                    dispatcher,
                    current_time,
                    uid,
                    result,
                )
            }
            TcpInputAction::Send { uid, result } => {
                let current_time = get_current_time(state);

                input_send(
                    state.models.state_mut(),
                    dispatcher,
                    current_time,
                    uid,
                    result,
                )
            }
            TcpInputAction::Recv { uid, result } => {
                let current_time = get_current_time(state);

                input_recv(
                    state.models.state_mut(),
                    dispatcher,
                    current_time,
                    uid,
                    result,
                )
            }
            TcpInputAction::PeerAddress { uid, result } => {
                input_peer_address(state.models.state_mut(), dispatcher, uid, result)
            }
        }
    }
}

// Pure action handlers

fn pure_init(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    init_uid: Uid,
    poll_uid: Uid,
    on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
) {
    tcp_state.status = Status::InitPollCreate {
        init_uid,
        poll_uid,
        on_completion,
    };
    dispatcher.dispatch(MioOutputAction::PollCreate {
        uid: poll_uid,
        on_completion: CompletionRoutine::new(|(uid, success)| {
            AnyAction::from(TcpInputAction::PollCreate { uid, success })
        }),
    });
}

fn pure_listen(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    address: String,
    on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
) {
    assert!(tcp_state.is_ready());

    tcp_state.new_listener(uid, address.clone(), on_completion);
    dispatcher.dispatch(MioOutputAction::TcpListen {
        uid,
        address,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpInputAction::Listen { uid, result })
        }),
    });
}

fn pure_accept(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    listener_uid: Uid,
    on_completion: CompletionRoutine<(Uid, ConnectResult)>,
) {
    assert!(tcp_state.is_ready());
    assert!(matches!(
        tcp_state.get_listener(&listener_uid).events(),
        ListenerEvent::AcceptPending
    ));
    let conn_type = ConnectionType::Incoming(listener_uid);

    tcp_state.new_connection(uid, conn_type, None, on_completion);
    dispatcher.dispatch(MioOutputAction::TcpAccept {
        uid,
        listener_uid,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpInputAction::Accept { uid, result })
        }),
    });
}

fn pure_connect(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    address: String,
    timeout: Option<u128>,
    on_completion: CompletionRoutine<(Uid, ConnectResult)>,
) {
    assert!(tcp_state.is_ready());

    tcp_state.new_connection(uid, ConnectionType::Outgoing, timeout, on_completion);
    dispatcher.dispatch(MioOutputAction::TcpConnect {
        uid,
        address,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpInputAction::Connect { uid, result })
        }),
    });
}

fn pure_close(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection_uid: Uid,
    on_completion: CompletionRoutine<Uid>,
) {
    let Status::Ready { poll_uid, .. } = tcp_state.status else {
        unreachable!()
    };

    let Connection { status, .. } = tcp_state.get_connection_mut(&connection_uid);

    *status = ConnectionStatus::CloseRequest(Some(on_completion));

    // before closing the stream we remove it from the poll object
    dispatcher.dispatch(MioOutputAction::PollDeregisterTcpConnection {
        poll_uid,
        connection_uid,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpInputAction::DeregisterConnection { uid, result })
        }),
    });
}

fn pure_poll(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    objects: Vec<Uid>,
    timeout: Option<u64>,
    on_completion: CompletionRoutine<(Uid, PollResult)>,
) {
    let Status::Ready {
        init_uid: _,
        poll_uid,
        events_uid,
    } = tcp_state.status
    else {
        unreachable!()
    };

    tcp_state.new_poll(uid, objects, timeout, on_completion);
    dispatcher.dispatch(MioOutputAction::PollEvents {
        uid,
        poll_uid,
        events_uid,
        timeout,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpInputAction::Poll { uid, result })
        }),
    })
}

fn pure_send(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection_uid: Uid,
    data: Rc<[u8]>,
    timeout: Option<u128>,
    on_completion: CompletionRoutine<(Uid, SendResult)>,
) {
    assert!(tcp_state.is_ready());

    let mut set_send_on_poll = false;

    tcp_state.new_send_request(
        uid,
        connection_uid,
        data,
        set_send_on_poll,
        timeout,
        on_completion.clone(),
    );

    let remove_request = dispatch_send(tcp_state, dispatcher, uid, &mut set_send_on_poll);

    let SendRequest { send_on_poll, .. } = tcp_state.get_send_request_mut(&uid);
    *send_on_poll = set_send_on_poll;

    if remove_request {
        tcp_state.remove_send_request(&uid)
    }
}

fn pure_recv(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection_uid: Uid,
    count: usize,
    timeout: Option<u128>,
    on_completion: CompletionRoutine<(Uid, RecvResult)>,
) {
    assert!(tcp_state.is_ready());

    let mut set_recv_on_poll = false;

    tcp_state.new_recv_request(
        uid,
        connection_uid,
        count,
        set_recv_on_poll,
        timeout,
        on_completion.clone(),
    );

    let remove_request = dispatch_recv(tcp_state, dispatcher, uid, &mut set_recv_on_poll);

    let RecvRequest { recv_on_poll, .. } = tcp_state.get_recv_request_mut(&uid);
    *recv_on_poll = set_recv_on_poll;

    if remove_request {
        tcp_state.remove_recv_request(&uid)
    }
}

fn get_current_time<Substate: ModelState>(state: &State<Substate>) -> u128 {
    state.models.state::<TimeState>().now.as_millis()
}

fn get_timeout_absolute<Substate: ModelState>(
    state: &State<Substate>,
    timeout: Option<u64>,
) -> Option<u128> {
    // Convert relative the timeout we passed to absolute timeout by adding the current time
    timeout.and_then(|timeout| Some(get_current_time(state).saturating_add(timeout.into())))
}

impl PureModel for TcpState {
    type Action = TcpPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpPureAction::Init {
                init_uid,
                on_completion,
            } => {
                let poll_uid = state.new_uid();

                pure_init(
                    state.models.state_mut(),
                    dispatcher,
                    init_uid,
                    poll_uid,
                    on_completion,
                )
            }
            TcpPureAction::Listen {
                uid,
                address,
                on_completion,
            } => pure_listen(
                state.models.state_mut(),
                dispatcher,
                uid,
                address,
                on_completion,
            ),
            TcpPureAction::Accept {
                uid,
                listener_uid,
                on_completion,
            } => pure_accept(
                state.models.state_mut(),
                dispatcher,
                uid,
                listener_uid,
                on_completion,
            ),
            TcpPureAction::Connect {
                uid,
                address,
                timeout,
                on_completion,
            } => {
                let timeout = get_timeout_absolute(state, timeout);

                pure_connect(
                    state.models.state_mut(),
                    dispatcher,
                    uid,
                    address,
                    timeout,
                    on_completion,
                )
            }
            TcpPureAction::Close {
                connection_uid,
                on_completion,
            } => pure_close(
                state.models.state_mut(),
                dispatcher,
                connection_uid,
                on_completion,
            ),
            TcpPureAction::Poll {
                uid,
                objects,
                timeout,
                on_completion,
            } => pure_poll(
                state.models.state_mut(),
                dispatcher,
                uid,
                objects,
                timeout,
                on_completion,
            ),
            TcpPureAction::Send {
                uid,
                connection_uid,
                data,
                timeout,
                on_completion,
            } => {
                let timeout = get_timeout_absolute(state, timeout);

                pure_send(
                    state.models.state_mut(),
                    dispatcher,
                    uid,
                    connection_uid,
                    data,
                    timeout,
                    on_completion,
                )
            }
            TcpPureAction::Recv {
                uid,
                connection_uid,
                count,
                timeout,
                on_completion,
            } => {
                let timeout = get_timeout_absolute(state, timeout);

                pure_recv(
                    state.models.state_mut(),
                    dispatcher,
                    uid,
                    connection_uid,
                    count,
                    timeout,
                    on_completion,
                )
            }
        }
    }
}
