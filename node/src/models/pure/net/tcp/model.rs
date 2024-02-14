use super::{
    action::{ConnectResult, ConnectionEvent, RecvResult, TcpAction, TcpPollResult},
    state::{ConnectionStatus, Listener, RecvRequest, SendRequest, Status, TcpState},
};
use crate::{
    automaton::{
        action::{Dispatcher, OrError, Redispatch, Timeout, TimeoutAbsolute},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::{
        effectful::mio::{
            action::{
                MioEffectfulAction, MioEvent, PollResult, TcpAcceptResult, TcpReadResult,
                TcpWriteResult,
            },
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
use std::rc::Rc;

// The `TcpState` model handles the state of a TCP connection system, which is
// built on top of the `MioState` model. It processes the outcomes of external
// inputs (the results of `MioState` actions).
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

// This model depends on the `TimeState` (pure) and `MioState` (effectful).
impl RegisterModel for TcpState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<TimeState>()
            .register::<MioState>()
            .model_pure::<Self>()
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
                instance,
                on_result,
            } => {
                let poll = state.new_uid();
                let tcp_state = state.substate_mut();
                init(tcp_state, dispatcher, instance, poll, on_result)
            }
            // dispatched back from init()
            TcpAction::PollCreateSuccess { poll: _ } => {
                let events_uid = state.new_uid();
                let tcp_state = state.substate_mut();
                handle_poll_create_success(tcp_state, dispatcher, events_uid)
            }
            TcpAction::PollCreateError { error, .. } => {
                handle_poll_create_error(state.substate_mut(), dispatcher, error)
            }
            // dispatched back from handle_poll_create_success()
            TcpAction::EventsCreate { uid: _ } => {
                let tcp_state = state.substate_mut();
                handle_events_create(tcp_state, dispatcher)
            }
            TcpAction::Listen {
                tcp_listener,
                address,
                on_result,
            } => {
                let tcp_state = state.substate_mut();
                listen(tcp_state, dispatcher, tcp_listener, address, on_result)
            }
            TcpAction::ListenSuccess { listener } => {
                let tcp_state = state.substate_mut();
                handle_listen_success(tcp_state, dispatcher, listener)
            }
            TcpAction::ListenError { listener, error } => {
                let tcp_state = state.substate_mut();
                handle_listen_error(tcp_state, dispatcher, listener, error)
            }
            // dispatched from handle_listen_success()
            TcpAction::RegisterListenerSuccess { listener } => {
                let tcp_state = state.substate::<TcpState>();
                let Listener { on_result, .. } = tcp_state.get_listener(&listener);
                dispatcher.dispatch_back(&on_result, (listener, Ok(())));
            }
            TcpAction::RegisterListenerError { listener, error } => {
                let tcp_state = state.substate_mut::<TcpState>();
                let Listener { on_result, .. } = tcp_state.get_listener(&listener);
                dispatcher.dispatch_back(&on_result, (listener, Err(error)));
                tcp_state.remove_listener(&listener)
            }
            TcpAction::Accept {
                connection,
                listener: tcp_listener,
                on_result,
            } => {
                let tcp_state = state.substate_mut();
                accept(tcp_state, dispatcher, connection, tcp_listener, on_result)
            }
            TcpAction::AcceptSuccess { connection } => {
                let tcp_state = state.substate_mut();
                handle_accept_success(tcp_state, dispatcher, connection)
            }
            TcpAction::AcceptTryAgain { connection } => {
                let tcp_state = state.substate_mut();
                handle_accept_try_again(tcp_state, dispatcher, connection)
            }
            TcpAction::AcceptError { connection, error } => {
                let tcp_state = state.substate_mut();
                handle_accept_error(tcp_state, dispatcher, connection, error)
            }
            TcpAction::Connect {
                connection,
                address,
                timeout,
                on_result,
            } => {
                let timeout = get_timeout_absolute(state, timeout);
                let tcp_state = state.substate_mut();
                connect(
                    tcp_state, dispatcher, connection, address, timeout, on_result,
                )
            }
            TcpAction::ConnectSuccess { connection } => {
                let tcp_state = state.substate();
                handle_connect_success(tcp_state, dispatcher, connection)
            }
            TcpAction::ConnectError { connection, error } => {
                let tcp_state = state.substate_mut();
                handle_connect_error(tcp_state, dispatcher, connection, error)
            }
            // dispatched back from: handle_accept_success(), handle_connect_success()
            TcpAction::RegisterConnectionSuccess { connection } => {
                let tcp_state = state.substate_mut();
                handle_register_connection_success(tcp_state, dispatcher, connection)
            }
            TcpAction::RegisterConnectionError { connection, error } => {
                let tcp_state = state.substate_mut();
                handle_register_connection_error(tcp_state, dispatcher, connection, error)
            }
            TcpAction::Close {
                connection,
                on_result,
            } => {
                let tcp_state = state.substate_mut();
                close(tcp_state, dispatcher, connection, on_result)
            }
            TcpAction::CloseResult { connection } => {
                let tcp_state = state.substate_mut();
                handle_close_result(tcp_state, dispatcher, connection)
            }
            // dispatched from close()
            TcpAction::DeregisterConnectionSuccess { connection } => {
                dispatcher.dispatch_effect(MioEffectfulAction::TcpClose {
                    connection,
                    on_result: callback!(|connection: Uid| TcpAction::CloseResult { connection }),
                })
            }
            TcpAction::DeregisterConnectionError { connection, error } => {
                panic!(
                    "Error de-registering connection {:?}: {}",
                    connection, error
                )
            }
            TcpAction::Poll {
                uid,
                objects,
                timeout,
                on_result,
            } => {
                let tcp_state = state.substate_mut();
                poll(tcp_state, dispatcher, uid, objects, timeout, on_result)
            }
            TcpAction::PollSuccess { uid, events } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut();
                handle_poll_success(tcp_state, dispatcher, current_time, uid, events)
            }
            TcpAction::PollInterrupted { uid } => {
                let tcp_state = state.substate_mut();
                handle_poll_interrupted(tcp_state, dispatcher, uid)
            }
            TcpAction::PollError { uid, error } => {
                let tcp_state = state.substate_mut::<TcpState>();
                let PollRequest { on_result, .. } = tcp_state.get_poll_request(&uid);
                dispatcher.dispatch_back(&on_result, (uid, Err(error)));
                tcp_state.remove_poll_request(&uid)
            }
            // dispatched back from process_pending_connections()
            TcpAction::GetPeerAddressSuccess {
                connection,
                address,
            } => {
                let tcp_state = state.substate_mut();
                handle_peer_address_success(tcp_state, dispatcher, connection, address)
            }
            TcpAction::GetPeerAddressError { connection, error } => {
                let tcp_state = state.substate_mut();
                handle_peer_address_error(tcp_state, dispatcher, connection, error)
            }
            TcpAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result,
            } => {
                let timeout = get_timeout_absolute(state, timeout);
                let tcp_state = state.substate_mut();
                send(
                    tcp_state, dispatcher, uid, connection, data, timeout, on_result,
                )
            }
            TcpAction::SendSuccess { uid } => {
                let tcp_state = state.substate_mut::<TcpState>();

                dispatcher.dispatch_back(
                    &tcp_state.get_send_request(&uid).on_result,
                    (uid, SendResult::Success),
                );
                tcp_state.remove_send_request(&uid)
            }
            TcpAction::SendSuccessPartial { uid, written } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut::<TcpState>();
                tcp_state.get_send_request_mut(&uid).bytes_sent += written;
                handle_send_common(tcp_state, dispatcher, current_time, uid, true)
            }
            TcpAction::SendErrorInterrupted { uid } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut();
                handle_send_common(tcp_state, dispatcher, current_time, uid, true)
            }
            TcpAction::SendErrorTryAgain { uid } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut();
                handle_send_common(tcp_state, dispatcher, current_time, uid, false)
            }
            TcpAction::SendError { uid, error } => {
                let tcp_state = state.substate_mut::<TcpState>();

                dispatcher.dispatch_back(
                    &tcp_state.get_send_request(&uid).on_result,
                    (uid, SendResult::Error(error)),
                );
                tcp_state.remove_send_request(&uid)
            }
            TcpAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result,
            } => {
                let timeout = get_timeout_absolute(state, timeout);
                let tcp_state = state.substate_mut();
                recv(
                    tcp_state, dispatcher, uid, connection, count, timeout, on_result,
                )
            }
            TcpAction::RecvSuccess { uid, data } => {
                let tcp_state = state.substate_mut::<TcpState>();
                let RecvRequest {
                    buffered_data,
                    remaining_bytes,
                    on_result,
                    ..
                } = tcp_state.get_recv_request_mut(&uid);

                *remaining_bytes = remaining_bytes
                    .checked_sub(data.len())
                    .expect("Received more data than requested");
                buffered_data.extend_from_slice(&data);
                dispatcher.dispatch_back(
                    &on_result,
                    (uid, RecvResult::Success(buffered_data.clone())),
                );
                tcp_state.remove_recv_request(&uid);
            }
            TcpAction::RecvSuccessPartial { uid, data } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut::<TcpState>();
                let RecvRequest {
                    buffered_data,
                    remaining_bytes,
                    ..
                } = tcp_state.get_recv_request_mut(&uid);

                *remaining_bytes = remaining_bytes
                    .checked_sub(data.len())
                    .expect("Received more data than requested");
                buffered_data.extend_from_slice(&data);
                handle_recv_common(tcp_state, dispatcher, current_time, uid, true)
            }
            TcpAction::RecvErrorInterrupted { uid } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut();
                handle_recv_common(tcp_state, dispatcher, current_time, uid, true)
            }
            TcpAction::RecvErrorTryAgain { uid } => {
                let current_time = get_current_time(state);
                let tcp_state = state.substate_mut();
                handle_recv_common(tcp_state, dispatcher, current_time, uid, false)
            }
            TcpAction::RecvError { uid, error } => {
                let tcp_state = state.substate_mut::<TcpState>();

                dispatcher.dispatch_back(
                    &tcp_state.get_recv_request(&uid).on_result,
                    (uid, RecvResult::Error(error)),
                );
                tcp_state.remove_recv_request(&uid)
            }
        }
    }
}

fn handle_poll_create_success(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, events: Uid) {
    if let Status::InitPollCreate {
        instance,
        poll,
        ref on_result,
    } = tcp_state.status
    {
        // Dispatch next action to continue initialization
        dispatcher.dispatch_effect(MioEffectfulAction::EventsCreate {
            uid: events,
            capacity: 1024,
            on_result: callback!(|uid: Uid| TcpAction::EventsCreate { uid }),
        });

        // next state
        tcp_state.status = Status::InitEventsCreate {
            instance,
            poll,
            events,
            on_result: on_result.clone(),
        };
    } else {
        unreachable!()
    }
}

fn handle_poll_create_error(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, error: String) {
    if let Status::InitPollCreate {
        instance,
        poll: _,
        ref on_result,
    } = tcp_state.status
    {
        // dispatch error to caller
        dispatcher.dispatch_back(on_result, (instance, OrError::Err(error)));

        // set init error state
        tcp_state.status = Status::InitError { instance };
    } else {
        unreachable!()
    }
}

fn handle_events_create(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher) {
    if let Status::InitEventsCreate {
        instance,
        poll,
        events,
        ref on_result,
    } = tcp_state.status
    {
        dispatcher.dispatch_back(&on_result, (instance, OrError::<()>::Ok(())));
        tcp_state.status = Status::Ready {
            instance,
            poll,
            events,
        };
    } else {
        unreachable!()
    }
}

fn handle_listen_success(tcp_state: &TcpState, dispatcher: &mut Dispatcher, listener: Uid) {
    // If the listen operation was successful we register the listener in the MIO poll object.
    let Status::Ready { poll, .. } = tcp_state.status else {
        unreachable!()
    };

    dispatcher.dispatch_effect(MioEffectfulAction::PollRegisterTcpServer {
        poll,
        listener,
        on_result: callback!(|(listener: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::RegisterListenerSuccess { listener },
            Err(error) => TcpAction::RegisterListenerError { listener, error },
        }),
    });
}

fn handle_listen_error(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    listener: Uid,
    error: String,
) {
    dispatcher.dispatch_back(
        &tcp_state.get_listener(&listener).on_result,
        (listener, Err(error)),
    );
    tcp_state.remove_listener(&listener);
}

fn handle_accept_success(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, connection: Uid) {
    let Connection {
        direction: ConnectionDirection::Incoming { listener: _ },
        ..
    } = tcp_state.get_connection(&connection)
    else {
        unreachable!()
    };
    let Status::Ready { poll, .. } = tcp_state.status else {
        panic!("Wrong TCP state: {:?}", tcp_state.status)
    };

    // We will dispatch-back to the caller from `handle_register_connection_result`
    dispatcher.dispatch_effect(MioEffectfulAction::PollRegisterTcpConnection {
        poll,
        connection,
        on_result: callback!(|(connection: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::RegisterConnectionSuccess { connection },
            Err(error) => TcpAction::RegisterConnectionError { connection, error },
        }),
    });
}

fn handle_accept_try_again(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, connection: Uid) {
    let Connection {
        direction: ConnectionDirection::Incoming { listener },
        on_result,
        ..
    } = tcp_state.get_connection(&connection)
    else {
        unreachable!()
    };

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
    tcp_state.remove_connection(&connection)
}

fn handle_accept_error(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    error: String,
) {
    let Connection {
        direction: ConnectionDirection::Incoming { listener: _ },
        on_result,
        ..
    } = tcp_state.get_connection(&connection)
    else {
        unreachable!()
    };

    dispatcher.dispatch_back(
        &on_result,
        (
            connection,
            ConnectionResult::Incoming(AcceptResult::Error(error)),
        ),
    );
    tcp_state.remove_connection(&connection)
}

fn handle_close_result(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, connection: Uid) {
    let conn = tcp_state.get_connection(&connection);
    if let Connection {
        status:
            ConnectionStatus::CloseRequest {
                maybe_on_result: Some(on_result),
            },
        ..
    } = conn
    {
        dispatcher.dispatch_back(&on_result, connection)
    } else {
        panic!(
            "Close callback called on connection {:?} with invalid status {:?}",
            connection, conn.status
        )
    }

    tcp_state.remove_connection(&connection);
}

fn handle_register_connection_success(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
) {
    if let Connection {
        direction: ConnectionDirection::Incoming { .. },
        on_result,
        ..
    } = tcp_state.get_connection(&connection)
    {
        dispatcher.dispatch_back(
            &on_result,
            (
                connection,
                ConnectionResult::Incoming(AcceptResult::Success),
            ),
        );
    }
}

fn handle_register_connection_error(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    error: String,
) {
    let Connection {
        status,
        direction,
        on_result,
        ..
    } = tcp_state.get_connection_mut(&connection);

    *status = ConnectionStatus::CloseRequest {
        maybe_on_result: None,
    };
    dispatcher.dispatch_effect(MioEffectfulAction::TcpClose {
        connection,
        on_result: callback!(|connection: Uid| TcpAction::CloseResult { connection }),
    });

    let error = format!("Error registering connection {:?}: {}", connection, error);
    let connection_result = match direction {
        ConnectionDirection::Incoming { .. } => {
            ConnectionResult::Incoming(AcceptResult::Error(error))
        }
        ConnectionDirection::Outgoing => ConnectionResult::Outgoing(ConnectResult::Error(error)),
    };

    dispatcher.dispatch_back(&on_result, (connection, connection_result));
}

fn handle_connect_success(tcp_state: &TcpState, dispatcher: &mut Dispatcher, connection: Uid) {
    let Status::Ready { poll, .. } = tcp_state.status else {
        unreachable!()
    };

    dispatcher.dispatch_effect(MioEffectfulAction::PollRegisterTcpConnection {
        poll,
        connection,
        on_result: callback!(|(connection: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::RegisterConnectionSuccess { connection },
            Err(error) => TcpAction::RegisterConnectionError { connection, error }
        }),
    });
}

fn handle_connect_error(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    error: String,
) {
    let Connection {
        direction: ConnectionDirection::Outgoing,
        on_result,
        ..
    } = tcp_state.get_connection(&connection)
    else {
        unreachable!()
    };

    dispatcher.dispatch_back(
        &on_result,
        (
            connection,
            ConnectionResult::Outgoing(ConnectResult::Error(error)),
        ),
    );
    tcp_state.remove_connection(&connection);
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
                    dispatcher.dispatch_effect(MioEffectfulAction::TcpGetPeerAddress {
                        connection: uid,
                        on_result: callback!(|(connection: Uid, result: OrError<String>)| match result {
                            Ok(address) => TcpAction::GetPeerAddressSuccess { connection, address },
                            Err(error) => TcpAction::GetPeerAddressError { connection, error },
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
                    dispatcher.dispatch_effect(MioEffectfulAction::TcpWrite {
                        uid,
                        connection: *connection,
                        data: (&data[*bytes_sent..]).into(),
                        on_result: callback!(|(uid: Uid, result: TcpWriteResult)| match result {
                            TcpWriteResult::WrittenAll => TcpAction::SendSuccess { uid },
                            TcpWriteResult::WrittenPartial(written) => {
                                TcpAction::SendSuccessPartial { uid, written }
                            }
                            TcpWriteResult::Interrupted => TcpAction::SendErrorInterrupted { uid },
                            TcpWriteResult::WouldBlock => TcpAction::SendErrorTryAgain { uid },
                            TcpWriteResult::Error(error) => TcpAction::SendError { uid, error },
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
            connection,
            buffered_data,
            remaining_bytes,
            recv_on_poll: _,
            timeout,
            on_result,
        },
    ) in tcp_state.pending_recv_requests()
    {
        let connection = *connection;
        let timed_out = match timeout {
            TimeoutAbsolute::Millis(ms) => current_time >= *ms,
            TimeoutAbsolute::Never => false,
        };
        let event = tcp_state.get_connection(&connection).events();

        match event {
            ConnectionEvent::Ready { can_recv: true, .. } => {
                if timed_out {
                    dispatcher.dispatch_back(
                        &on_result,
                        (uid, RecvResult::Timeout(buffered_data.clone())),
                    );
                    purge_requests.push(uid);
                } else {
                    dispatcher.dispatch_effect(MioEffectfulAction::TcpRead {
                        uid,
                        connection,
                        len: *remaining_bytes,
                        on_result: callback!(|(uid: Uid, result: TcpReadResult)| match result {
                            TcpReadResult::ReadAll(data) => TcpAction::RecvSuccess { uid, data },
                            TcpReadResult::ReadPartial(data) => TcpAction::RecvSuccessPartial { uid, data },
                            TcpReadResult::Interrupted => TcpAction::RecvErrorInterrupted { uid },
                            TcpReadResult::WouldBlock => TcpAction::RecvErrorTryAgain { uid },
                            TcpReadResult::Error(error) => TcpAction::RecvError { uid, error },
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
                        (uid, RecvResult::Timeout(buffered_data.clone())),
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

fn handle_poll_success(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    events: Vec<MioEvent>,
) {
    // update TCP object events (even for Uids that were not requested)
    for mio_event in events.iter() {
        tcp_state.update_events(mio_event)
    }

    //if events.len() > 0 {
        process_pending_connections(current_time, tcp_state, dispatcher);
        process_pending_send_requests(current_time, tcp_state, dispatcher);
        process_pending_recv_requests(current_time, tcp_state, dispatcher);
    //}

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

fn handle_poll_interrupted(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, uid: Uid) {
    // if the syscall was interrupted we re-dispatch the MIO action
    let PollRequest { timeout, .. } = tcp_state.get_poll_request(&uid);
    let Status::Ready { poll, events, .. } = tcp_state.status else {
        unreachable!()
    };

    dispatcher.dispatch_effect(MioEffectfulAction::PollEvents {
        uid,
        poll,
        events,
        timeout: timeout.clone(),
        on_result: callback!(|(uid: Uid, result: PollResult)| match result {
            PollResult::Events(events) => TcpAction::PollSuccess { uid, events },
            PollResult::Interrupted => TcpAction::PollInterrupted { uid },
            PollResult::Error(error) => TcpAction::PollError { uid, error }
        }),
    })
}

fn handle_send_common(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    can_send_value: bool,
) {
    let SendRequest {
        connection,
        timeout,
        on_result,
        ..
    } = tcp_state.get_send_request_mut(&uid);

    let timed_out = match *timeout {
        TimeoutAbsolute::Millis(ms) => current_time >= ms,
        TimeoutAbsolute::Never => false,
    };

    if timed_out {
        dispatcher.dispatch_back(on_result, (uid, SendResult::Timeout));
        tcp_state.remove_send_request(&uid)
    } else {
        if can_send_value == false {
            tcp_state.get_send_request_mut(&uid).send_on_poll = true;
            return;
        }

        let connection = *connection;
        let conn = tcp_state.get_connection_mut(&connection);

        if conn.events.is_some() {
            let ConnectionEvent::Ready { can_send, .. } = conn.events_mut() else {
                unreachable!()
            };

            *can_send = can_send_value;
            dispatch_send(tcp_state, dispatcher, uid);
        } else {
            tcp_state.get_send_request_mut(&uid).send_on_poll = true;
        }
    }
}

fn handle_recv_common(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    current_time: u128,
    uid: Uid,
    can_recv_value: bool,
) {
    let RecvRequest {
        connection,
        buffered_data,
        timeout,
        on_result,
        ..
    } = tcp_state.get_recv_request_mut(&uid);

    let timed_out = match *timeout {
        TimeoutAbsolute::Millis(ms) => current_time >= ms,
        TimeoutAbsolute::Never => false,
    };

    if timed_out {
        dispatcher.dispatch_back(on_result, (uid, RecvResult::Timeout(buffered_data.clone())));
        tcp_state.remove_recv_request(&uid)
    } else {
        if can_recv_value == false {
            tcp_state.get_recv_request_mut(&uid).recv_on_poll = true;
            return;
        }

        let connection = *connection;
        let conn = tcp_state.get_connection_mut(&connection);

        if conn.events.is_some() {
            let ConnectionEvent::Ready { can_recv, .. } = conn.events_mut() else {
                unreachable!()
            };

            *can_recv = can_recv_value;
            dispatch_recv(tcp_state, dispatcher, uid);
        } else {
            tcp_state.get_recv_request_mut(&uid).recv_on_poll = true;
        }
    }
}

fn dispatch_send(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, uid: Uid) {
    let connection = tcp_state.get_send_request(&uid).connection;
    let conn = tcp_state.get_connection(&connection);

    if conn.events.is_none() {
        tcp_state.get_send_request_mut(&uid).send_on_poll = true;
        return;
    }

    match conn.events() {
        ConnectionEvent::Ready { can_send: true, .. } => {
            let SendRequest {
                data, bytes_sent, ..
            } = tcp_state.get_send_request(&uid);

            dispatcher.dispatch_effect(MioEffectfulAction::TcpWrite {
                uid,
                connection,
                data: (&data[*bytes_sent..]).into(),
                on_result: callback!(|(uid: Uid, result: TcpWriteResult)| match result {
                    TcpWriteResult::WrittenAll => TcpAction::SendSuccess { uid },
                    TcpWriteResult::WrittenPartial(written) => {
                        TcpAction::SendSuccessPartial { uid, written }
                    }
                    TcpWriteResult::Interrupted => TcpAction::SendErrorInterrupted { uid },
                    TcpWriteResult::WouldBlock => TcpAction::SendErrorTryAgain { uid },
                    TcpWriteResult::Error(error) => TcpAction::SendError { uid, error },
                }),
            });
        }
        ConnectionEvent::Ready {
            can_send: false, ..
        } => tcp_state.get_send_request_mut(&uid).send_on_poll = true,
        ConnectionEvent::Closed => {
            dispatcher.dispatch_back(
                &tcp_state.get_send_request(&uid).on_result,
                (uid, SendResult::Error("Connection closed".to_string())),
            );
            tcp_state.remove_send_request(&uid)
        }
        ConnectionEvent::Error => {
            dispatcher.dispatch_back(
                &tcp_state.get_send_request(&uid).on_result,
                (uid, SendResult::Error("Connection error".to_string())),
            );
            tcp_state.remove_send_request(&uid)
        }
    };
}

fn dispatch_recv(tcp_state: &mut TcpState, dispatcher: &mut Dispatcher, uid: Uid) {
    let connection = tcp_state.get_recv_request(&uid).connection;
    let conn = tcp_state.get_connection(&connection);

    if conn.events.is_none() {
        tcp_state.get_recv_request_mut(&uid).recv_on_poll = true;
        return;
    }

    match conn.events() {
        ConnectionEvent::Ready { can_recv: true, .. } => {
            dispatcher.dispatch_effect(MioEffectfulAction::TcpRead {
                uid,
                connection,
                len: tcp_state.get_recv_request(&uid).remaining_bytes,
                on_result: callback!(|(uid: Uid, result: TcpReadResult)| match result {
                    TcpReadResult::ReadAll(data) => TcpAction::RecvSuccess { uid, data },
                    TcpReadResult::ReadPartial(data) => TcpAction::RecvSuccessPartial { uid, data },
                    TcpReadResult::Interrupted => TcpAction::RecvErrorInterrupted { uid },
                    TcpReadResult::WouldBlock => TcpAction::RecvErrorTryAgain { uid },
                    TcpReadResult::Error(error) => TcpAction::RecvError { uid, error },
                }),
            });
        }
        ConnectionEvent::Ready {
            can_recv: false, ..
        } => tcp_state.get_recv_request_mut(&uid).recv_on_poll = true,
        ConnectionEvent::Closed => {
            // Recv failed, notify caller
            dispatcher.dispatch_back(
                &tcp_state.get_recv_request_mut(&uid).on_result,
                (uid, RecvResult::Error("Connection closed".to_string())),
            );
            tcp_state.remove_recv_request(&uid)
        }
        ConnectionEvent::Error => {
            // Recv failed, notify caller
            dispatcher.dispatch_back(
                &tcp_state.get_recv_request_mut(&uid).on_result,
                (uid, RecvResult::Error("Connection error".to_string())),
            );
            tcp_state.remove_recv_request(&uid)
        }
    }
}

fn handle_peer_address_success(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    _address: String,
) {
    let conn = tcp_state.get_connection_mut(&connection);
    let Connection {
        status: ConnectionStatus::PendingCheck,
        direction: ConnectionDirection::Outgoing,
        on_result,
        ..
    } = conn
    else {
        unreachable!()
    };

    conn.status = ConnectionStatus::Established;
    dispatcher.dispatch_back(
        on_result,
        (
            connection,
            ConnectionResult::Outgoing(ConnectResult::Success),
        ),
    );
}

fn handle_peer_address_error(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    error: String,
) {
    let Connection {
        status: ConnectionStatus::PendingCheck,
        direction: ConnectionDirection::Outgoing,
        on_result,
        ..
    } = tcp_state.get_connection_mut(&connection)
    else {
        unreachable!()
    };

    dispatcher.dispatch_back(
        on_result,
        (
            connection,
            ConnectionResult::Outgoing(ConnectResult::Error(error)),
        ),
    );

    tcp_state.remove_connection(&connection)
}

fn init(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    instance: Uid,
    poll: Uid,
    on_result: Redispatch<(Uid, OrError<()>)>,
) {
    tcp_state.status = Status::InitPollCreate {
        instance,
        poll,
        on_result,
    };
    dispatcher.dispatch_effect(MioEffectfulAction::PollCreate {
        poll,
        on_result: callback!(|(poll: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::PollCreateSuccess { poll },
            Err(error) => TcpAction::PollCreateError { poll, error }
        }),
    });
}

fn listen(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    tcp_listener: Uid,
    address: String,
    on_result: Redispatch<(Uid, OrError<()>)>,
) {
    assert!(tcp_state.is_ready());
    tcp_state.new_listener(tcp_listener, address.clone(), on_result);
    dispatcher.dispatch_effect(MioEffectfulAction::TcpListen {
        listener: tcp_listener,
        address,
        on_result: callback!(|(listener: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::ListenSuccess { listener },
            Err(error) => TcpAction::ListenError { listener, error }
        }),
    });
}

fn accept(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    tcp_listener: Uid,
    on_result: Redispatch<(Uid, ConnectionResult)>,
) {
    assert!(tcp_state.is_ready());
    assert!(matches!(
        tcp_state.get_listener(&tcp_listener).events(),
        ListenerEvent::AcceptPending
    ));
    let direction = ConnectionDirection::Incoming {
        listener: tcp_listener,
    };

    tcp_state.new_connection(connection, direction, TimeoutAbsolute::Never, on_result);
    dispatcher.dispatch_effect(MioEffectfulAction::TcpAccept {
        connection,
        listener: tcp_listener,
        on_result: callback!(|(connection: Uid, result: TcpAcceptResult)| match result {
            TcpAcceptResult::Success => TcpAction::AcceptSuccess { connection },
            TcpAcceptResult::WouldBlock => TcpAction::AcceptTryAgain { connection },
            TcpAcceptResult::Error(error) => TcpAction::AcceptError { connection, error }
        }),
    });
}

fn connect(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    address: String,
    timeout: TimeoutAbsolute,
    on_result: Redispatch<(Uid, ConnectionResult)>,
) {
    assert!(tcp_state.is_ready());

    tcp_state.new_connection(
        connection,
        ConnectionDirection::Outgoing,
        timeout,
        on_result,
    );
    dispatcher.dispatch_effect(MioEffectfulAction::TcpConnect {
        connection,
        address,
        on_result: callback!(|(connection: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::ConnectSuccess { connection },
            Err(error) => TcpAction::ConnectError { connection, error }
        }),
    });
}

fn close(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    on_result: Redispatch<Uid>,
) {
    let Status::Ready { poll, .. } = tcp_state.status else {
        unreachable!()
    };

    let Connection { status, .. } = tcp_state.get_connection_mut(&connection);

    *status = ConnectionStatus::CloseRequest {
        maybe_on_result: Some(on_result),
    };

    // before closing the stream we remove it from the poll object
    dispatcher.dispatch_effect(MioEffectfulAction::PollDeregisterTcpConnection {
        poll,
        connection,
        on_result: callback!(|(connection: Uid, result: OrError<()>)| match result {
            Ok(_) => TcpAction::DeregisterConnectionSuccess { connection },
            Err(error) => TcpAction::DeregisterConnectionError { connection, error }
        }),
    });
}

fn poll(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    objects: Vec<Uid>,
    timeout: Timeout,
    on_result: Redispatch<(Uid, TcpPollResult)>,
) {
    let Status::Ready { poll, events, .. } = tcp_state.status else {
        unreachable!()
    };

    tcp_state.new_poll(uid, objects, timeout.clone(), on_result);
    dispatcher.dispatch_effect(MioEffectfulAction::PollEvents {
        uid,
        poll,
        events,
        timeout,
        on_result: callback!(|(uid: Uid, result: PollResult)| match result {
            PollResult::Events(events) => TcpAction::PollSuccess { uid, events },
            PollResult::Interrupted => TcpAction::PollInterrupted { uid },
            PollResult::Error(error) => TcpAction::PollError { uid, error }
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
    on_result: Redispatch<(Uid, SendResult)>,
) {
    assert!(tcp_state.is_ready());

    if !tcp_state.has_connection(&connection) {
        dispatcher.dispatch_back(
            &on_result,
            (uid, SendResult::Error("No such connection".to_string())),
        );
        return;
    }

    tcp_state.new_send_request(uid, connection, data, false, timeout, on_result.clone());
    dispatch_send(tcp_state, dispatcher, uid)
}

fn recv(
    tcp_state: &mut TcpState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection: Uid,
    count: usize,
    timeout: TimeoutAbsolute,
    on_result: Redispatch<(Uid, RecvResult)>,
) {
    assert!(tcp_state.is_ready());

    if !tcp_state.has_connection(&connection) {
        dispatcher.dispatch_back(
            &on_result,
            (uid, RecvResult::Error("No such connection".to_string())),
        );
        return;
    }

    tcp_state.new_recv_request(uid, connection, count, false, timeout, on_result.clone());

    dispatch_recv(tcp_state, dispatcher, uid)
}
