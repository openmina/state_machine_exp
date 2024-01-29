use super::{
    action::{
        ConnectResult, ConnectionEvent, RecvResult, TcpInputAction, TcpPollResult, TcpPureAction,
    },
    state::{ConnectionStatus, Listener, RecvRequest, SendRequest, Status, TcpState},
};
use crate::{
    automaton::{
        action::{Dispatcher, ResultDispatch, Timeout, TimeoutAbsolute},
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::{
        effectful::mio::{
            action::{MioOutputAction, PollResult, TcpAcceptResult, TcpReadResult, TcpWriteResult},
            state::MioState,
        },
        pure::{
            net::tcp::{
                action::{AcceptResult, ConnectionResult, Event, ListenerEvent, SendResult},
                state::{Connection, ConnectionDirection, EventUpdater, PollRequest},
            },
            time::{
                model::{get_current_time, get_timeout_absolute},
                state::TimeState,
            },
        },
    },
};
use core::panic;
//use log::info;
use std::rc::Rc;

// The `TcpState` model handles the state of a TCP connection system, which is
// built on top of the `MioState` model. It processes the outcomes of external
// inputs (the results of `MioState` actions) through its `InputModel`
// implementation, while deterministic state transitions are managed by its
// `PureModel` implementation.
//
// This model facilitates various operations, including:
// - Creating polls.
// - Establishing connections to remote peers.
// - Listening for connections.
// - Sending and receiving data.
//
// Another feature provided by this model is timeout support for the async IO.
// While the `TcpState` model simplifies some aspects of the `MioState` model,
// it's still pretty low-level. For simpler use, there are the `TcpClientState`
// and `TcpServerState` models, which are built on top of the `TcpState` model.

// This model depends on the `TimeState` (pure) and `MioState` (output) models.
impl RegisterModel for TcpState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<TimeState>()
            .register::<MioState>()
            .model_pure_and_input::<Self>()
    }
}

enum Act {
    In(TcpInputAction),
    Pure(TcpPureAction),
}

fn process_action<Substate: ModelState>(
    state: &mut State<Substate>,
    action: Act,
    dispatcher: &mut Dispatcher,
) {
    match action {
        Act::Pure(TcpPureAction::Init {
            instance,
            on_result,
        }) => {
            let poll = state.new_uid();
            let tcp_state = state.substate_mut();

            init(tcp_state, dispatcher, instance, poll, on_result)
        }
        // dispatched from init()
        Act::In(TcpInputAction::PollCreateResult { result, .. }) => {
            let events_uid = state.new_uid();
            let tcp_state = state.substate_mut();

            handle_poll_create_result(tcp_state, dispatcher, events_uid, result)
        }
        // dispatched from handle_poll_create_result()
        Act::In(TcpInputAction::EventsCreateResult { .. }) => {
            let tcp_state = state.substate_mut();

            handle_events_create_result(tcp_state, dispatcher)
        }
        Act::Pure(TcpPureAction::Listen {
            tcp_listener,
            address,
            on_result,
        }) => {
            let tcp_state = state.substate_mut();

            listen(tcp_state, dispatcher, tcp_listener, address, on_result)
        }
        Act::In(TcpInputAction::ListenResult {
            tcp_listener,
            result,
        }) => {
            let tcp_state = state.substate_mut();

            handle_listen_result(tcp_state, dispatcher, tcp_listener, result)
        }
        // dispatched from handle_listen_result()
        Act::In(TcpInputAction::RegisterListenerResult {
            tcp_listener,
            result,
        }) => {
            let tcp_state = state.substate_mut();

            handle_register_listener_result(tcp_state, dispatcher, tcp_listener, result)
        }
        Act::Pure(TcpPureAction::Accept {
            connection,
            tcp_listener,
            on_result,
        }) => {
            let tcp_state = state.substate_mut();

            accept(tcp_state, dispatcher, connection, tcp_listener, on_result)
        }
        Act::In(TcpInputAction::AcceptResult { connection, result }) => {
            let tcp_state = state.substate_mut();

            handle_accept_result(tcp_state, dispatcher, connection, result)
        }
        Act::Pure(TcpPureAction::Connect {
            connection,
            address,
            timeout,
            on_result,
        }) => {
            let timeout = get_timeout_absolute(state, timeout);
            let tcp_state = state.substate_mut();

            connect(
                tcp_state, dispatcher, connection, address, timeout, on_result,
            )
        }
        Act::In(TcpInputAction::ConnectResult { connection, result }) => {
            let tcp_state = state.substate_mut();

            handle_connect_result(tcp_state, dispatcher, connection, result)
        }
        // dispatched from: handle_accept_result(), handle_connect_result()
        Act::In(TcpInputAction::RegisterConnectionResult { connection, result }) => {
            let tcp_state = state.substate_mut();

            handle_register_connection_result(tcp_state, dispatcher, connection, result)
        }
        Act::Pure(TcpPureAction::Close {
            connection,
            on_result,
        }) => {
            let tcp_state = state.substate_mut();

            close(tcp_state, dispatcher, connection, on_result)
        }
        Act::In(TcpInputAction::CloseResult { connection }) => {
            let tcp_state = state.substate_mut();

            handle_close_result(tcp_state, dispatcher, connection)
        }
        // dispatched from close()
        Act::In(TcpInputAction::DeregisterConnectionResult { connection, result }) => {
            handle_deregister_connection_result(dispatcher, connection, result)
        }
        Act::Pure(TcpPureAction::Poll {
            uid,
            objects,
            timeout,
            on_result,
        }) => {
            let tcp_state = state.substate_mut();

            poll(tcp_state, dispatcher, uid, objects, timeout, on_result)
        }
        Act::In(TcpInputAction::PollResult { uid, result }) => {
            let current_time = get_current_time(state);
            let tcp_state = state.substate_mut();

            handle_poll_result(tcp_state, dispatcher, current_time, uid, result)
        }
        // dispatched from handle_poll_result() -> process_pending_connections()
        Act::In(TcpInputAction::PeerAddressResult { connection, result }) => {
            let tcp_state = state.substate_mut();

            handle_peer_address_result(tcp_state, dispatcher, connection, result)
        }
        Act::Pure(TcpPureAction::Send {
            uid,
            connection,
            data,
            timeout,
            on_result,
        }) => {
            let timeout = get_timeout_absolute(state, timeout);
            let tcp_state = state.substate_mut();

            send(
                tcp_state, dispatcher, uid, connection, data, timeout, on_result,
            )
        }
        Act::In(TcpInputAction::SendResult { uid, result }) => {
            let current_time = get_current_time(state);
            let tcp_state = state.substate_mut();

            handle_send_result(tcp_state, dispatcher, current_time, uid, result)
        }
        Act::Pure(TcpPureAction::Recv {
            uid,
            connection,
            count,
            timeout,
            on_result,
        }) => {
            let timeout = get_timeout_absolute(state, timeout);
            let tcp_state = state.substate_mut();

            recv(
                tcp_state, dispatcher, uid, connection, count, timeout, on_result,
            )
        }
        Act::In(TcpInputAction::RecvResult { uid, result }) => {
            let current_time = get_current_time(state);

            handle_recv_result(state.substate_mut(), dispatcher, current_time, uid, result)
        }
    }
}

impl PureModel for TcpState {
    type Action = TcpPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::Pure(action), dispatcher)
    }
}

impl InputModel for TcpState {
    type Action = TcpInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::In(action), dispatcher)
    }
}

fn handle_poll_create_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    events: Uid,
    result: Result<(), String>,
) {
    assert!(matches!(tcp_state.status, Status::InitPollCreate { .. }));

    if let Status::InitPollCreate {
        instance,
        poll,
        ref on_result,
    } = tcp_state.status
    {
        if result.is_ok() {
            // Dispatch next action to continue initialization
            dispatcher.dispatch(MioOutputAction::EventsCreate {
                uid: events,
                capacity: 1024,
                on_result: callback!(|uid: Uid| TcpInputAction::EventsCreateResult { uid }),
            });

            // next state
            tcp_state.status = Status::InitEventsCreate {
                instance,
                poll,
                events,
                on_result: on_result.clone(),
            };
        } else {
            // dispatch error to caller
            dispatcher.dispatch_back(on_result, (instance, result));

            // set init error state
            tcp_state.status = Status::InitError { instance };
        }
    }
}

fn handle_events_create_result(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher) {
    assert!(matches!(tcp_state.status, Status::InitEventsCreate { .. }));

    if let Status::InitEventsCreate {
        instance,
        poll,
        events,
        ref on_result,
    } = tcp_state.status
    {
        dispatcher.dispatch_back(&on_result, (instance, Ok::<(), String>(())));

        tcp_state.status = Status::Ready {
            instance,
            poll,
            events,
        };
    }
}

fn handle_listen_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    tcp_listener: Uid,
    result: Result<(), String>,
) {
    if result.is_ok() {
        // If the listen operation was successful we register the listener in the MIO poll object.
        let Status::Ready { poll, .. } = tcp_state.status else {
            unreachable!()
        };

        dispatcher.dispatch(MioOutputAction::PollRegisterTcpServer {
            poll,
            tcp_listener,
            on_result: callback!(|(tcp_listener: Uid, result: Result<(), String>)| {
                TcpInputAction::RegisterListenerResult {
                    tcp_listener,
                    result,
                }
            }),
        });
    } else {
        dispatcher.dispatch_back(
            &tcp_state.get_listener(&tcp_listener).on_result,
            (tcp_listener, result),
        );
        tcp_state.remove_listener(&tcp_listener);
    }
}

fn handle_register_listener_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    tcp_listener: Uid,
    result: Result<(), String>,
) {
    let Listener { on_result, .. } = tcp_state.get_listener(&tcp_listener);

    dispatcher.dispatch_back(&on_result, (tcp_listener, result.clone()));

    if result.is_err() {
        tcp_state.remove_listener(&tcp_listener)
    }
}

fn handle_accept_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    result: TcpAcceptResult,
) {
    let Connection {
        direction,
        on_result,
        ..
    } = tcp_state.get_connection(&connection);
    let ConnectionDirection::Incoming {
        tcp_listener: listener,
    } = direction
    else {
        panic!(
            "Accept callback {:?} on invalid connection type {:?}",
            connection, direction
        );
    };
    let mut remove = false;

    match result {
        TcpAcceptResult::Success => {
            // If the connection accept was successful we register it in the MIO poll object.
            let Status::Ready { poll, .. } = tcp_state.status else {
                panic!("Wrong TCP state: {:?}", tcp_state.status)
            };

            // We will dispatch-back to the caller from `handle_register_connection_result`
            dispatcher.dispatch(MioOutputAction::PollRegisterTcpConnection {
                poll,
                connection,
                on_result: callback!(|(connection: Uid, result: Result<(), String>)| {
                    TcpInputAction::RegisterConnectionResult { connection, result }
                }),
            });
        }
        TcpAcceptResult::WouldBlock => {
            dispatcher.dispatch_back(
                &on_result,
                (
                    connection,
                    ConnectionResult::Incoming(AcceptResult::WouldBlock),
                ),
            );

            let listener_uid = *listener;
            let events = tcp_state.get_listener_mut(&listener_uid).events_mut();

            assert!(matches!(events, ListenerEvent::AcceptPending));
            *events = ListenerEvent::AllAccepted;

            remove = true;
        }
        TcpAcceptResult::Error(error) => {
            // Dispatch error result now
            dispatcher.dispatch_back(
                &on_result,
                (
                    connection,
                    ConnectionResult::Incoming(AcceptResult::Error(error)),
                ),
            );
            remove = true;
        }
    }

    if remove {
        tcp_state.remove_connection(&connection)
    }
}

fn handle_close_result(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, connection: Uid) {
    let Connection { status, .. } = tcp_state.get_connection(&connection);

    match status {
        ConnectionStatus::CloseRequest {
            maybe_on_result: Some(on_result),
        } => dispatcher.dispatch_back(&on_result, connection),
        _ => panic!(
            "Close callback called on connection {:?} with invalid status {:?}",
            connection, status
        ),
    }

    tcp_state.remove_connection(&connection);
}

fn handle_register_connection_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    result: Result<(), String>,
) {
    let Connection {
        status,
        direction,
        on_result,
        ..
    } = tcp_state.get_connection_mut(&connection);

    if let Err(error) = result {
        *status = ConnectionStatus::CloseRequest {
            maybe_on_result: None,
        };
        dispatcher.dispatch(MioOutputAction::TcpClose {
            connection,
            on_result: callback!(|connection: Uid| TcpInputAction::CloseResult { connection }),
        });

        let error = format!("Error registering connection {:?}: {}", connection, error);
        let connection_result = match direction {
            ConnectionDirection::Incoming { .. } => {
                ConnectionResult::Incoming(AcceptResult::Error(error))
            }
            ConnectionDirection::Outgoing => {
                ConnectionResult::Outgoing(ConnectResult::Error(error))
            }
        };

        dispatcher.dispatch_back(&on_result, (connection, connection_result));
    } else {
        if let ConnectionDirection::Incoming { .. } = direction {
            dispatcher.dispatch_back(
                &on_result,
                (
                    connection,
                    ConnectionResult::Incoming(AcceptResult::Success),
                ),
            );
        }
    }
}

fn handle_deregister_connection_result(
    dispatcher: &mut Dispatcher,
    connection: Uid,
    result: Result<(), String>,
) {
    match result {
        Ok(_) => dispatcher.dispatch(MioOutputAction::TcpClose {
            connection,
            on_result: callback!(|connection: Uid| TcpInputAction::CloseResult { connection }),
        }),
        Err(error) => panic!(
            "Error de-registering connection {:?}: {}",
            connection, error
        ),
    }
}

fn handle_connect_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    result: Result<(), String>,
) {
    let Connection {
        direction,
        on_result,
        ..
    } = tcp_state.get_connection(&connection);

    assert!(matches!(direction, ConnectionDirection::Outgoing));

    match result {
        Ok(()) => {
            let Status::Ready { poll, .. } = tcp_state.status else {
                unreachable!()
            };

            dispatcher.dispatch(MioOutputAction::PollRegisterTcpConnection {
                poll,
                connection,
                on_result: callback!(|(connection: Uid, result: Result<(), String>)| {
                    TcpInputAction::RegisterConnectionResult { connection, result }
                }),
            });
        }
        Err(error) => {
            dispatcher.dispatch_back(
                &on_result,
                (
                    connection,
                    ConnectionResult::Outgoing(ConnectResult::Error(error)),
                ),
            );
            tcp_state.remove_connection(&connection);
        }
    }
}

fn process_pending_connections(
    current_time: u128,
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
) {
    let mut purge_requests = Vec::new();

    for (
        &uid,
        Connection {
            status,
            direction,
            timeout,
            on_result,
            ..
        },
    ) in tcp_state.pending_connections_mut()
    {
        let timed_out = match timeout {
            TimeoutAbsolute::Millis(ms) => current_time >= *ms,
            TimeoutAbsolute::Never => false,
        };

        if timed_out {
            assert!(matches!(direction, ConnectionDirection::Outgoing));
            dispatcher.dispatch_back(
                &on_result,
                (uid, ConnectionResult::Outgoing(ConnectResult::Timeout)),
            );
            purge_requests.push(uid);
        } else {
            match status {
                ConnectionStatus::Pending => {
                    dispatcher.dispatch(MioOutputAction::TcpGetPeerAddress {
                        connection: uid,
                        on_result: callback!(|(connection: Uid, result: Result<String, String>)| {
                            TcpInputAction::PeerAddressResult { connection, result }
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

fn process_pending_send_requests(
    current_time: u128,
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
) {
    let mut purge_requests = Vec::new();
    let mut dispatched_requests = Vec::new();

    process_pending_send_requests_aux(
        current_time,
        tcp_state,
        dispatcher,
        &mut purge_requests,
        &mut dispatched_requests,
    );

    // remove requests for invalid or closed connections
    for uid in purge_requests.iter() {
        tcp_state.remove_send_request(uid)
    }
}

fn process_pending_send_requests_aux(
    current_time: u128,
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    purge_requests: &mut Vec<Uid>,
    dispatched_requests: &mut Vec<Uid>,
) {
    for (
        &uid,
        SendRequest {
            connection,
            data,
            bytes_sent,
            timeout,
            on_result,
            ..
        },
    ) in tcp_state.pending_send_requests()
    {
        let timed_out = match timeout {
            TimeoutAbsolute::Millis(ms) => current_time >= *ms,
            TimeoutAbsolute::Never => false,
        };

        let event = tcp_state.get_connection(&connection).events();

        match event {
            ConnectionEvent::Ready { can_send: true, .. } => {
                if timed_out {
                    dispatcher.dispatch_back(&on_result, (uid, SendResult::Timeout));
                    purge_requests.push(uid);
                } else {
                    dispatcher.dispatch(MioOutputAction::TcpWrite {
                        uid,
                        connection: *connection,
                        data: (&data[*bytes_sent..]).into(),
                        on_result: callback!(|(uid: Uid, result: TcpWriteResult)| {
                            TcpInputAction::SendResult { uid, result }
                        }),
                    });

                    dispatched_requests.push(uid);
                }
            }
            ConnectionEvent::Ready {
                can_send: false, ..
            } => {
                if timed_out {
                    dispatcher.dispatch_back(&on_result, (uid, SendResult::Timeout));
                    purge_requests.push(uid);
                }
            }
            ConnectionEvent::Closed => {
                dispatcher.dispatch_back(
                    &on_result,
                    (uid, SendResult::Error("Connection closed".to_string())),
                );

                purge_requests.push(uid);
            }
            ConnectionEvent::Error => {
                dispatcher.dispatch_back(
                    &on_result,
                    (uid, SendResult::Error("Connection error".to_string())),
                );

                purge_requests.push(uid);
            }
        }
    }
}

fn process_pending_recv_requests(
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
            connection: connection_uid,
            data,
            bytes_received,
            recv_on_poll: _,
            timeout,
            on_result,
        },
    ) in tcp_state.pending_recv_requests()
    {
        let timed_out = match timeout {
            TimeoutAbsolute::Millis(ms) => current_time >= *ms,
            TimeoutAbsolute::Never => false,
        };
        let event = tcp_state.get_connection(&connection_uid).events();

        match event {
            ConnectionEvent::Ready { can_recv: true, .. } => {
                if timed_out {
                    dispatcher.dispatch_back(
                        &on_result,
                        (uid, RecvResult::Timeout(data[0..*bytes_received].to_vec())),
                    );
                    purge_requests.push(uid);
                } else {
                    dispatcher.dispatch(MioOutputAction::TcpRead {
                        uid,
                        connection: *connection_uid,
                        len: data.len().saturating_sub(*bytes_received),
                        on_result: callback!(|(uid: Uid, result: TcpReadResult)| {
                            TcpInputAction::RecvResult { uid, result }
                        }),
                    });

                    dispatched_requests.push(uid);
                }
            }
            ConnectionEvent::Ready {
                can_recv: false, ..
            } => {
                if timed_out {
                    dispatcher.dispatch_back(
                        &on_result,
                        (uid, RecvResult::Timeout(data[0..*bytes_received].to_vec())),
                    );
                    purge_requests.push(uid);
                }
            }
            ConnectionEvent::Closed => {
                dispatcher.dispatch_back(
                    &on_result,
                    (uid, RecvResult::Error("Connection closed".to_string())),
                );

                purge_requests.push(uid);
            }
            ConnectionEvent::Error => {
                dispatcher.dispatch_back(
                    &on_result,
                    (uid, RecvResult::Error("Connection error".to_string())),
                );

                purge_requests.push(uid);
            }
        }
    }
}

fn handle_poll_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    result: PollResult,
) {
    assert!(tcp_state.is_ready());

    match result {
        PollResult::Events(ref events) => {
            // update TCP object events (even for Uids that were not requested)
            for mio_event in events.iter() {
                tcp_state.update_events(mio_event)
            }

            process_pending_connections(current_time, tcp_state, dispatcher);
            process_pending_send_requests(current_time, tcp_state, dispatcher);
            process_pending_recv_requests(current_time, tcp_state, dispatcher);

            let request = tcp_state.get_poll_request(&uid);
            // Collect events from state for the requested objects
            let events: Vec<(Uid, Event)> = request
                .objects
                .iter()
                .filter_map(|uid| {
                    tcp_state.get_events(uid).and_then(|(uid, event)| {
                        if let Event::Listener(ListenerEvent::AllAccepted) = event {
                            None
                        } else {
                            Some((uid, event))
                        }
                    })
                })
                .collect();

            dispatcher.dispatch_back(
                &request.on_result,
                (uid, Ok::<Vec<(Uid, Event)>, String>(events)),
            );
            tcp_state.remove_poll_request(&uid)
        }
        PollResult::Error(err) => {
            let PollRequest { on_result, .. } = tcp_state.get_poll_request(&uid);
            dispatcher.dispatch_back(&on_result, (uid, Err(err)));
            tcp_state.remove_poll_request(&uid)
        }
        PollResult::Interrupted => {
            // if the syscall was interrupted we re-dispatch the MIO action
            let PollRequest { timeout, .. } = tcp_state.get_poll_request(&uid);
            let Status::Ready { poll, events, .. } = tcp_state.status else {
                unreachable!()
            };

            dispatcher.dispatch(MioOutputAction::PollEvents {
                uid,
                poll,
                events,
                timeout: timeout.clone(),
                on_result: callback!(|(uid: Uid, result: PollResult)| {
                    TcpInputAction::PollResult { uid, result }
                }),
            })
        }
    }
}

fn dispatch_send(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    set_send_on_poll: &mut bool,
) -> bool {
    let SendRequest {
        connection,
        data,
        bytes_sent,
        on_result,
        ..
    } = tcp_state.get_send_request(&uid);

    let conn = tcp_state.get_connection(connection);

    if conn.events.is_none() {
        *set_send_on_poll = true;
        return false;
    }

    match conn.events() {
        ConnectionEvent::Ready { can_send: true, .. } => {
            dispatcher.dispatch(MioOutputAction::TcpWrite {
                uid,
                connection: *connection,
                data: (&data[*bytes_sent..]).into(),
                on_result: callback!(|(uid: Uid, result: TcpWriteResult)| {
                    TcpInputAction::SendResult { uid, result }
                }),
            });
        }
        ConnectionEvent::Ready {
            can_send: false, ..
        } => {
            *set_send_on_poll = true;
        }
        ConnectionEvent::Closed => {
            dispatcher.dispatch_back(
                &on_result,
                (uid, SendResult::Error("Connection closed".to_string())),
            );
            return true;
        }
        ConnectionEvent::Error => {
            dispatcher.dispatch_back(
                &on_result,
                (uid, SendResult::Error("Connection error".to_string())),
            );
            return true;
        }
    }

    return false;
}

fn handle_send_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    result: TcpWriteResult,
) {
    assert!(tcp_state.is_ready());

    let request = tcp_state.get_send_request_mut(&uid);
    let connection = request.connection;

    let (completed, can_send) =
        handle_send_result_aux(current_time, uid, result, request, dispatcher);
    let mut remove_request = completed;

    // We need to redispatch if the previous send was incomplete/interrupted.
    if !completed {
        let conn = tcp_state.get_connection_mut(&connection);

        if conn.events.is_some() {
            let ConnectionEvent::Ready {
                can_send: can_send_mut,
                ..
            } = conn.events_mut()
            else {
                unreachable!()
            };

            *can_send_mut = can_send;

            let mut set_send_on_poll = false;
            remove_request = dispatch_send(tcp_state, dispatcher, uid, &mut set_send_on_poll);

            let SendRequest { send_on_poll, .. } = tcp_state.get_send_request_mut(&uid);
            *send_on_poll = set_send_on_poll;
        }
    }

    if remove_request {
        tcp_state.remove_send_request(&uid)
    }
}

fn handle_send_result_aux(
    current_time: u128,
    uid: Uid,
    result: TcpWriteResult,
    request: &mut SendRequest,
    dispatcher: &mut Dispatcher,
) -> (bool, bool) {
    let timed_out = match request.timeout {
        TimeoutAbsolute::Millis(ms) => current_time >= ms,
        TimeoutAbsolute::Never => false,
    };

    match result {
        // if there was a timeout but we already written all or got an error we will let it pass..
        TcpWriteResult::WrittenAll => {
            // Send complete, notify caller
            dispatcher.dispatch_back(&request.on_result, (uid, SendResult::Success));
            (true, true)
        }
        TcpWriteResult::Error(error) => {
            // Send failed, notify caller
            dispatcher.dispatch_back(&request.on_result, (uid, SendResult::Error(error)));
            (true, true)
        }
        TcpWriteResult::WrittenPartial(count) => {
            if timed_out {
                dispatcher.dispatch_back(&request.on_result, (uid, SendResult::Timeout));
                (true, true)
            } else {
                request.bytes_sent += count;
                (false, true)
            }
        }
        TcpWriteResult::Interrupted => {
            if timed_out {
                dispatcher.dispatch_back(&request.on_result, (uid, SendResult::Timeout));
                (true, true)
            } else {
                (false, true)
            }
        }
        TcpWriteResult::WouldBlock => {
            if timed_out {
                dispatcher.dispatch_back(&request.on_result, (uid, SendResult::Timeout));
                (true, false)
            } else {
                (false, false)
            }
        }
    }
}

fn dispatch_recv(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    set_recv_on_poll: &mut bool,
) -> bool {
    let RecvRequest {
        connection,
        data,
        bytes_received,
        on_result,
        ..
    } = tcp_state.get_recv_request(&uid);

    let conn = tcp_state.get_connection(connection);

    if conn.events.is_none() {
        *set_recv_on_poll = true;
        return false;
    }

    match conn.events() {
        ConnectionEvent::Ready { can_recv: true, .. } => {
            dispatcher.dispatch(MioOutputAction::TcpRead {
                uid,
                connection: *connection,
                len: data.len().saturating_sub(*bytes_received),
                on_result: callback!(|(uid: Uid, result: TcpReadResult)| {
                    TcpInputAction::RecvResult { uid, result }
                }),
            });
        }
        ConnectionEvent::Ready {
            can_recv: false, ..
        } => {
            // TODO: check timeouts and dispatch caller
            *set_recv_on_poll = true;
        }
        ConnectionEvent::Closed => {
            // Recv failed, notify caller
            dispatcher.dispatch_back(
                &on_result,
                (uid, RecvResult::Error("Connection closed".to_string())),
            );
            return true;
        }
        ConnectionEvent::Error => {
            // Recv failed, notify caller
            dispatcher.dispatch_back(
                &on_result,
                (uid, RecvResult::Error("Connection error".to_string())),
            );
            return true;
        }
    }

    return false;
}

fn handle_recv_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    result: TcpReadResult,
) {
    assert!(tcp_state.is_ready());

    let request = tcp_state.get_recv_request_mut(&uid);
    let connection = request.connection;

    let (completed, can_recv) =
        handle_recv_result_aux(current_time, uid, result, request, dispatcher);
    let mut remove_request = completed;

    // We need to redispatch if the previous recv was incomplete/interrupted.
    if !completed {
        let conn = tcp_state.get_connection_mut(&connection);

        if conn.events.is_some() {
            let ConnectionEvent::Ready {
                can_recv: can_recv_mut,
                ..
            } = conn.events_mut()
            else {
                unreachable!()
            };

            *can_recv_mut = can_recv;

            let mut set_recv_on_poll = false;
            remove_request = dispatch_recv(tcp_state, dispatcher, uid, &mut set_recv_on_poll);

            let RecvRequest { recv_on_poll, .. } = tcp_state.get_recv_request_mut(&uid);
            *recv_on_poll = set_recv_on_poll;
        }
    }

    if remove_request {
        tcp_state.remove_recv_request(&uid)
    }
}

fn handle_recv_result_aux(
    current_time: u128,
    uid: Uid,
    result: TcpReadResult,
    request: &mut RecvRequest,
    dispatcher: &mut Dispatcher,
) -> (bool, bool) {
    let timed_out = match request.timeout {
        TimeoutAbsolute::Millis(ms) => current_time >= ms,
        TimeoutAbsolute::Never => false,
    };

    match result {
        // if there was a timeout but we recevied all data or there was an error we let it pass...
        TcpReadResult::ReadAll(data) => {
            let start_offset = request.bytes_received;
            let end_offset = start_offset + data.len();
            request.data[start_offset..end_offset].copy_from_slice(&data[..]);
            request.bytes_received = end_offset;

            let data = request.data[0..request.bytes_received].to_vec();
            // Recv complete, notify caller
            dispatcher.dispatch_back(&request.on_result, (uid, RecvResult::Success(data)));
            (true, true)
        }
        TcpReadResult::Error(error) => {
            // Recv failed, notify caller
            dispatcher.dispatch_back(&request.on_result, (uid, RecvResult::Error(error)));
            (true, true)
        }
        TcpReadResult::ReadPartial(data) => {
            if timed_out {
                dispatcher.dispatch_back(&request.on_result, (uid, RecvResult::Timeout(data)));
                (true, true)
            } else {
                let start_offset = request.bytes_received;
                let end_offset = start_offset + data.len();
                request.data[start_offset..end_offset].copy_from_slice(&data[..]);
                request.bytes_received = end_offset;
                (false, true)
            }
        }
        TcpReadResult::Interrupted => {
            if timed_out {
                let data = request.data[0..request.bytes_received].to_vec();
                dispatcher.dispatch_back(&request.on_result, (uid, RecvResult::Timeout(data)));
                (true, true)
            } else {
                (false, true)
            }
        }
        TcpReadResult::WouldBlock => {
            if timed_out {
                let data = request.data[0..request.bytes_received].to_vec();
                dispatcher.dispatch_back(&request.on_result, (uid, RecvResult::Timeout(data)));
                (true, false)
            } else {
                (false, false)
            }
        }
    }
}

fn handle_peer_address_result(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    result: Result<String, String>,
) {
    let Connection {
        status,
        direction,
        on_result,
        ..
    } = tcp_state.get_connection_mut(&connection);
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

        assert!(matches!(direction, ConnectionDirection::Outgoing));
        dispatcher.dispatch_back(on_result, (connection, ConnectionResult::Outgoing(result)));

        if remove {
            tcp_state.remove_connection(&connection)
        }
    } else {
        panic!(
            "PeerAddress action received for connection {:?} with wrong status {:?}",
            connection, status
        )
    }
}

fn init(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    instance: Uid,
    poll: Uid,
    on_result: ResultDispatch<(Uid, Result<(), String>)>,
) {
    tcp_state.status = Status::InitPollCreate {
        instance,
        poll,
        on_result,
    };
    dispatcher.dispatch(MioOutputAction::PollCreate {
        poll,
        on_result: callback!(|(poll: Uid, result: Result<(), String>)| {
            TcpInputAction::PollCreateResult { poll, result }
        }),
    });
}

fn listen(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    tcp_listener: Uid,
    address: String,
    on_result: ResultDispatch<(Uid, Result<(), String>)>,
) {
    assert!(tcp_state.is_ready());

    tcp_state.new_listener(tcp_listener, address.clone(), on_result);
    dispatcher.dispatch(MioOutputAction::TcpListen {
        tcp_listener,
        address,
        on_result: callback!(|(tcp_listener: Uid, result: Result<(), String>)| {
            TcpInputAction::ListenResult {
                tcp_listener,
                result,
            }
        }),
    });
}

fn accept(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    tcp_listener: Uid,
    on_result: ResultDispatch<(Uid, ConnectionResult)>,
) {
    assert!(tcp_state.is_ready());
    assert!(matches!(
        tcp_state.get_listener(&tcp_listener).events(),
        ListenerEvent::AcceptPending
    ));
    let direction = ConnectionDirection::Incoming { tcp_listener };

    tcp_state.new_connection(connection, direction, TimeoutAbsolute::Never, on_result);
    dispatcher.dispatch(MioOutputAction::TcpAccept {
        connection,
        tcp_listener,
        on_result: callback!(|(connection: Uid, result: TcpAcceptResult)| {
            TcpInputAction::AcceptResult { connection, result }
        }),
    });
}

fn connect(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    address: String,
    timeout: TimeoutAbsolute,
    on_result: ResultDispatch<(Uid, ConnectionResult)>,
) {
    assert!(tcp_state.is_ready());

    tcp_state.new_connection(
        connection,
        ConnectionDirection::Outgoing,
        timeout,
        on_result,
    );
    dispatcher.dispatch(MioOutputAction::TcpConnect {
        connection,
        address,
        on_result: callback!(|(connection: Uid, result: Result<(), String>)| {
            TcpInputAction::ConnectResult { connection, result }
        }),
    });
}

fn close(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    on_result: ResultDispatch<Uid>,
) {
    let Status::Ready { poll, .. } = tcp_state.status else {
        unreachable!()
    };

    let Connection { status, .. } = tcp_state.get_connection_mut(&connection);

    *status = ConnectionStatus::CloseRequest {
        maybe_on_result: Some(on_result),
    };

    // before closing the stream we remove it from the poll object
    dispatcher.dispatch(MioOutputAction::PollDeregisterTcpConnection {
        poll,
        connection,
        on_result: callback!(|(connection: Uid, result: Result<(), String>)| {
            TcpInputAction::DeregisterConnectionResult { connection, result }
        }),
    });
}

fn poll(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    objects: Vec<Uid>,
    timeout: Timeout,
    on_result: ResultDispatch<(Uid, TcpPollResult)>,
) {
    let Status::Ready { poll, events, .. } = tcp_state.status else {
        unreachable!()
    };

    tcp_state.new_poll(uid, objects, timeout.clone(), on_result);
    dispatcher.dispatch(MioOutputAction::PollEvents {
        uid,
        poll,
        events,
        timeout,
        on_result: callback!(|(uid: Uid, result: PollResult)| {
            TcpInputAction::PollResult { uid, result }
        }),
    })
}

fn send(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection: Uid,
    data: Rc<[u8]>,
    timeout: TimeoutAbsolute,
    on_result: ResultDispatch<(Uid, SendResult)>,
) {
    assert!(tcp_state.is_ready());

    if !tcp_state.has_connection(&connection) {
        dispatcher.dispatch_back(
            &on_result,
            (uid, SendResult::Error("No such connection".to_string())),
        );
        return;
    }

    let mut set_send_on_poll = false;

    tcp_state.new_send_request(
        uid,
        connection,
        data,
        set_send_on_poll,
        timeout,
        on_result.clone(),
    );

    let remove_request = dispatch_send(tcp_state, dispatcher, uid, &mut set_send_on_poll);

    let SendRequest { send_on_poll, .. } = tcp_state.get_send_request_mut(&uid);
    *send_on_poll = set_send_on_poll;

    if remove_request {
        tcp_state.remove_send_request(&uid)
    }
}

fn recv(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection: Uid,
    count: usize,
    timeout: TimeoutAbsolute,
    on_result: ResultDispatch<(Uid, RecvResult)>,
) {
    assert!(tcp_state.is_ready());

    if !tcp_state.has_connection(&connection) {
        dispatcher.dispatch_back(
            &on_result,
            (uid, RecvResult::Error("No such connection".to_string())),
        );
        return;
    }

    let mut set_recv_on_poll = false;

    tcp_state.new_recv_request(
        uid,
        connection,
        count,
        set_recv_on_poll,
        timeout,
        on_result.clone(),
    );

    let remove_request = dispatch_recv(tcp_state, dispatcher, uid, &mut set_recv_on_poll);

    let RecvRequest { recv_on_poll, .. } = tcp_state.get_recv_request_mut(&uid);
    *recv_on_poll = set_recv_on_poll;

    if remove_request {
        tcp_state.remove_recv_request(&uid)
    }
}
