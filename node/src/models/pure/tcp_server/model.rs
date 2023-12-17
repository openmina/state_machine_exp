use std::{collections::BTreeSet, rc::Rc};

use crate::{
    automaton::{
        action::{AnyAction, CompletionRoutine, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    models::pure::tcp::{
        action::{ConnectResult, Event, ListenerEvent, PollResult, RecvResult, TcpPureAction},
        state::SendResult,
    },
};

use super::{
    action::{TcpServerInputAction, TcpServerPureAction},
    state::{PollRequest, RecvRequest, SendRequest, TcpServerState},
};

fn input_new(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: Result<(), String>,
) {
    let server = server_state.get_server(&uid);

    dispatcher.completion_dispatch(&server.on_completion, (uid, result.clone()));

    if result.is_err() {
        server_state.remove_server(&uid)
    }
}

fn input_poll(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: PollResult,
) -> BTreeSet<Uid> {
    let request = server_state.take_poll_request();
    let mut accept_list = BTreeSet::new();
    let mut removed_list = BTreeSet::new();

    match result {
        Ok(events) => {
            for (uid, event) in events {
                let Event::Listener(listener_event) = event else {
                    panic!("Unrequested event type {:?} for Uid {:?}", event, uid)
                };

                match listener_event {
                    ListenerEvent::AcceptPending => {
                        let server = server_state.get_server(&uid);

                        if server.connections.len() < server.max_connections {
                            accept_list.insert(uid);
                        }
                    }
                    ListenerEvent::ConnectionAccepted => (),
                    ListenerEvent::Closed | ListenerEvent::Error => {
                        removed_list.insert(uid);
                    }
                }
            }
        }
        Err(err) => dispatcher.completion_dispatch(&request.on_completion, (uid, Err(err))),
    }

    if !removed_list.is_empty() {
        let err = format!(
            "Server(s) (Uid(s): {:?}) in closed or invalid state",
            removed_list
        );
        dispatcher.completion_dispatch(&request.on_completion, (uid, Err(err)));
        removed_list
            .iter()
            .for_each(|uid| server_state.remove_server(uid));
        accept_list.clear();
    } else {
        dispatcher.completion_dispatch(&request.on_completion, (uid, Ok(())));
    }

    accept_list
}

fn accept_connections(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    accept_pending: Vec<(Uid, Uid)>,
) {
    // Dispatch (multiple) TCP accept actions to accept a new pending connection for each server instance
    for (listener_uid, connection_uid) in accept_pending {
        server_state.new_connection(connection_uid, listener_uid);
        dispatcher.dispatch(TcpPureAction::Accept {
            uid: connection_uid,
            listener_uid,
            on_completion: CompletionRoutine::new(|(uid, result)| {
                AnyAction::from(TcpServerInputAction::Accept { uid, result })
            }),
        });
    }
}

fn input_accept(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: ConnectResult,
) {
    let (server_uid, server) = server_state.get_connection_server_mut(&uid);

    if let ConnectResult::Success = result {
        // Notify registered callback of new connection
        dispatcher.completion_dispatch(&server.on_new_connection, (*server_uid, uid));
    } else {
        // TODO: add log of bad result
        server.remove_connection(&uid);
    }
}

fn input_close(server_state: &mut TcpServerState, dispatcher: &mut Dispatcher, uid: Uid) {
    let (server_uid, server) = server_state.get_connection_server_mut(&uid);

    dispatcher.completion_dispatch(&server.on_close_connection, (*server_uid, uid));
    server.remove_connection(&uid);
}

fn input_send(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: SendResult,
) {
    let SendRequest {
        connection_uid,
        on_completion,
    } = server_state.take_send_request(&uid);

    match result {
        SendResult::Success | SendResult::Timeout => {
            dispatcher.completion_dispatch(&on_completion, (uid, result))
        }
        SendResult::Error(_) => {
            dispatcher.dispatch(TcpPureAction::Close {
                connection_uid,
                on_completion: CompletionRoutine::new(|uid| {
                    AnyAction::from(TcpServerInputAction::Close { uid })
                }),
            });
            dispatcher.completion_dispatch(&on_completion, (uid, result))
        }
    }
}

fn input_recv(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: RecvResult,
) {
    let RecvRequest {
        connection_uid,
        on_completion,
    } = server_state.take_recv_request(&uid);

    match result {
        RecvResult::Success(_) | RecvResult::Timeout(_) => {
            dispatcher.completion_dispatch(&on_completion, (uid, result))
        }
        RecvResult::Error(_) => {
            dispatcher.dispatch(TcpPureAction::Close {
                connection_uid,
                on_completion: CompletionRoutine::new(|uid| {
                    AnyAction::from(TcpServerInputAction::Close { uid })
                }),
            });
            dispatcher.completion_dispatch(&on_completion, (uid, result))
        }
    }
}

impl InputModel for TcpServerState {
    type Action = TcpServerInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpServerInputAction::New { uid, result } => {
                input_new(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpServerInputAction::Poll { uid, result } => {
                let accept_pending: Vec<_> =
                    input_poll(state.models.state_mut(), dispatcher, uid, result)
                        .iter()
                        .map(|uid| (*uid, state.new_uid()))
                        .collect();
                accept_connections(state.models.state_mut(), dispatcher, accept_pending)
            }
            TcpServerInputAction::Accept { uid, result } => {
                input_accept(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpServerInputAction::Close { uid } => {
                input_close(state.models.state_mut(), dispatcher, uid)
            }
            TcpServerInputAction::Send { uid, result } => {
                input_send(state.models.state_mut(), dispatcher, uid, result)
            }
            TcpServerInputAction::Recv { uid, result } => {
                input_recv(state.models.state_mut(), dispatcher, uid, result)
            }
        }
    }
}

fn pure_new(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    address: String,
    max_connections: usize,
    on_new_connection: CompletionRoutine<(Uid, Uid)>,
    on_close_connection: CompletionRoutine<(Uid, Uid)>,
    on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
) {
    server_state.new_server(
        uid,
        address.clone(),
        max_connections,
        on_new_connection,
        on_close_connection,
        on_completion,
    );

    dispatcher.dispatch(TcpPureAction::Listen {
        uid,
        address,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpServerInputAction::New { uid, result })
        }),
    });
}

fn pure_poll(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    timeout: Option<u64>,
    on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
) {
    let objects = server_state.server_objects.keys().cloned().collect();

    server_state.set_poll_request(PollRequest {
        timeout,
        on_completion,
    });

    dispatcher.dispatch(TcpPureAction::Poll {
        uid,
        objects,
        timeout,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpServerInputAction::Poll { uid, result })
        }),
    })
}

fn pure_close(dispatcher: &mut Dispatcher, connection_uid: Uid) {
    dispatcher.dispatch(TcpPureAction::Close {
        connection_uid,
        on_completion: CompletionRoutine::new(|uid| {
            AnyAction::from(TcpServerInputAction::Close { uid })
        }),
    })
}

fn pure_send(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection_uid: Uid,
    data: Rc<[u8]>,
    timeout: Option<u64>,
    on_completion: CompletionRoutine<(Uid, SendResult)>,
) {
    server_state.new_send_request(&uid, connection_uid, on_completion);
    dispatcher.dispatch(TcpPureAction::Send {
        uid,
        connection_uid,
        data,
        timeout,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpServerInputAction::Send { uid, result })
        }),
    });
}

fn pure_recv(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    connection_uid: Uid,
    count: usize,
    timeout: Option<u64>,
    on_completion: CompletionRoutine<(Uid, RecvResult)>,
) {
    server_state.new_recv_request(&uid, connection_uid, on_completion);
    dispatcher.dispatch(TcpPureAction::Recv {
        uid,
        connection_uid,
        count,
        timeout,
        on_completion: CompletionRoutine::new(|(uid, result)| {
            AnyAction::from(TcpServerInputAction::Recv { uid, result })
        }),
    });
}

impl PureModel for TcpServerState {
    type Action = TcpServerPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpServerPureAction::New {
                uid,
                address,
                max_connections,
                on_new_connection,
                on_close_connection,
                on_completion,
            } => pure_new(
                state.models.state_mut(),
                dispatcher,
                uid,
                address,
                max_connections,
                on_new_connection,
                on_close_connection,
                on_completion,
            ),
            TcpServerPureAction::Poll {
                uid,
                timeout,
                on_completion,
            } => pure_poll(
                state.models.state_mut(),
                dispatcher,
                uid,
                timeout,
                on_completion,
            ),
            TcpServerPureAction::Close { connection_uid } => pure_close(dispatcher, connection_uid),
            TcpServerPureAction::Send {
                uid,
                connection_uid,
                data,
                timeout,
                on_completion,
            } => pure_send(
                state.models.state_mut(),
                dispatcher,
                uid,
                connection_uid,
                data,
                timeout,
                on_completion,
            ),
            TcpServerPureAction::Recv {
                uid,
                connection_uid,
                count,
                timeout,
                on_completion,
            } => pure_recv(
                state.models.state_mut(),
                dispatcher,
                uid,
                connection_uid,
                count,
                timeout,
                on_completion,
            ),
        }
    }
}
