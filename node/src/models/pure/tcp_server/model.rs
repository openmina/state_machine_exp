use std::collections::BTreeSet;

use crate::{
    automaton::{
        action::{AnyAction, CompletionRoutine, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    models::pure::tcp::action::{Event, ListenerEvent, PollResult, TcpAction},
};

use super::{
    action::{TcpServerAction, TcpServerCallbackAction},
    state::{PollRequest, SendRequest, TcpServerState, RecvRequest},
};

fn handle_poll(
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

fn close_connection(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    server_uid: &Uid,
    connection_uid: &Uid,
) {
    let server = server_state.get_server_mut(server_uid);

    dispatcher.completion_dispatch(&server.on_close_connection, (*server_uid, *connection_uid));
    server.remove_connection(connection_uid);
}

impl InputModel for TcpServerState {
    type Action = TcpServerCallbackAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpServerCallbackAction::New { uid, result } => {
                let server_state: &mut TcpServerState = state.models.state_mut();
                let server = server_state.get_server(&uid);

                dispatcher.completion_dispatch(&server.on_completion, (uid, result.clone()));

                if result.is_err() {
                    server_state.remove_server(&uid)
                }
            }
            TcpServerCallbackAction::Poll { uid, result } => {
                let accept_pending = handle_poll(state.models.state_mut(), dispatcher, uid, result);
                let accept_new: Vec<_> = accept_pending
                    .iter()
                    .map(|uid| (*uid, state.new_uid()))
                    .collect();

                let server_state: &mut TcpServerState = state.models.state_mut();

                // Dispatch (multiple) TCP accept actions to accept a new pending connection for each server instance
                for (listener_uid, connection_uid) in accept_new {
                    server_state.new_connection(connection_uid, listener_uid);
                    dispatcher.dispatch(TcpAction::Accept {
                        uid: connection_uid,
                        listener_uid,
                        on_completion: CompletionRoutine::new(|(uid, result)| {
                            AnyAction::from(TcpServerCallbackAction::Accept { uid, result })
                        }),
                    });
                }
            }
            TcpServerCallbackAction::Accept { uid, result } => {
                let server_state: &mut TcpServerState = state.models.state_mut();
                let (server_uid, server) = server_state.get_connection_server_mut(&uid);

                if result.is_ok() {
                    // Notify registered callback of new connection
                    dispatcher.completion_dispatch(&server.on_new_connection, (*server_uid, uid));
                } else {
                    // TODO: add log of bad result
                    server.remove_connection(&uid);
                }
            }
            TcpServerCallbackAction::Send { uid, result } => {
                let server_state: &mut TcpServerState = state.models.state_mut();
                let SendRequest {
                    server_uid,
                    connection_uid,
                    on_completion,
                } = server_state.take_send_request(&uid);

                match result {
                    Ok(_) => dispatcher.completion_dispatch(&on_completion, (uid, Ok(()))),
                    Err(error) => {
                        close_connection(server_state, dispatcher, &server_uid, &connection_uid);
                        dispatcher.completion_dispatch(&on_completion, (uid, Err(error)))
                    }
                }
            }
            TcpServerCallbackAction::Recv { uid, result } => {
                let server_state: &mut TcpServerState = state.models.state_mut();
                let RecvRequest {
                    server_uid,
                    connection_uid,
                    on_completion,
                } = server_state.take_recv_request(&uid);

                match result {
                    Ok(data) => dispatcher.completion_dispatch(&on_completion, (uid, Ok(data))),
                    Err(error) => {
                        close_connection(server_state, dispatcher, &server_uid, &connection_uid);
                        dispatcher.completion_dispatch(&on_completion, (uid, Err(error)))
                    }
                }
            }
        }
    }
}

impl PureModel for TcpServerState {
    type Action = TcpServerAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpServerAction::New {
                uid,
                address,
                max_connections,
                on_new_connection,
                on_close_connection,
                on_completion,
            } => {
                let server_state: &mut TcpServerState = state.models.state_mut();

                server_state.new_server(
                    uid,
                    address.clone(),
                    max_connections,
                    on_new_connection,
                    on_close_connection,
                    on_completion,
                );

                dispatcher.dispatch(TcpAction::Listen {
                    uid,
                    address,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpServerCallbackAction::New { uid, result })
                    }),
                });
            }
            TcpServerAction::Poll {
                uid,
                timeout,
                on_completion,
            } => {
                let server_state: &mut TcpServerState = state.models.state_mut();
                let objects = server_state.server_objects.keys().cloned().collect();

                server_state.set_poll_request(PollRequest {
                    timeout,
                    on_completion,
                });

                dispatcher.dispatch(TcpAction::Poll {
                    uid,
                    objects,
                    timeout,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpServerCallbackAction::Poll { uid, result })
                    }),
                })
            }
            TcpServerAction::Send {
                uid,
                connection_uid,
                data,
                on_completion,
            } => {
                let server_state: &mut TcpServerState = state.models.state_mut();

                server_state.new_send_request(&uid, connection_uid, on_completion);
                dispatcher.dispatch(TcpAction::Send {
                    uid,
                    connection_uid,
                    data,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpServerCallbackAction::Send { uid, result })
                    }),
                });
            }
            TcpServerAction::Recv {
                uid,
                connection_uid,
                count,
                on_completion,
            } => {
                let server_state: &mut TcpServerState = state.models.state_mut();

                server_state.new_recv_request(&uid, connection_uid, on_completion);
                dispatcher.dispatch(TcpAction::Recv {
                    uid,
                    connection_uid,
                    count,
                    on_completion: CompletionRoutine::new(|(uid, result)| {
                        AnyAction::from(TcpServerCallbackAction::Recv { uid, result })
                    }),
                });
            }
        }
    }
}
