use super::{
    action::{TcpServerInputAction, TcpServerPureAction},
    state::{PollRequest, RecvRequest, SendRequest, Server, TcpServerState},
};
use crate::{
    automaton::{
        action::Dispatcher,
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::tcp::{
        action::{
            AcceptResult, ConnectionResult, Event, ListenerEvent, RecvResult, SendResult,
            TcpPollResult, TcpPureAction,
        },
        state::TcpState,
    },
};
use log::warn;
use std::collections::BTreeSet;

// The `TcpServerState` model is an abstraction layer over the `TcpState` model
// providing a simpler interface for working with TCP server operations.

// This model depends on the `TcpState` model.
impl RegisterModel for TcpServerState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<TcpState>()
            .model_pure_and_input::<Self>()
    }
}

enum Act {
    In(TcpServerInputAction),
    Pure(TcpServerPureAction),
}

fn process_action<Substate: ModelState>(
    state: &mut State<Substate>,
    action: Act,
    dispatcher: &mut Dispatcher,
) {
    match action {
        Act::Pure(TcpServerPureAction::New {
            address,
            server,
            max_connections,
            on_new_connection,
            on_close_connection,
            on_result,
        }) => {
            state.substate_mut::<TcpServerState>().new_server(
                server,
                max_connections,
                on_new_connection,
                on_close_connection,
                on_result,
            );

            dispatcher.dispatch(TcpPureAction::Listen {
                tcp_listener: server,
                address,
                on_result: callback!(|(server: Uid, result: Result<(), String>)| {
                    TcpServerInputAction::NewResult { server, result }
                }),
            });
        }
        Act::In(TcpServerInputAction::NewResult { server, result }) => {
            let server_state: &mut TcpServerState = state.substate_mut();
            let Server { on_result, .. } = server_state.get_server(&server);

            dispatcher.dispatch_back(on_result, (server, result.clone()));

            if result.is_err() {
                server_state.remove_server(&server)
            }
        }
        Act::Pure(TcpServerPureAction::Poll {
            uid,
            timeout,
            on_result,
        }) => {
            let server_state: &mut TcpServerState = state.substate_mut();
            let objects = server_state.server_objects.keys().cloned().collect();

            server_state.set_poll_request(PollRequest { on_result });
            dispatcher.dispatch(TcpPureAction::Poll {
                uid,
                objects,
                timeout,
                on_result: callback!(|(uid: Uid, result: Result<Vec<(Uid, Event)>, String>)| {
                    TcpServerInputAction::PollResult { uid, result }
                }),
            })
        }
        Act::In(TcpServerInputAction::PollResult { uid, result }) => {
            let accept_pending: Vec<_> =
                handle_poll_result(state.substate_mut(), dispatcher, uid, result)
                    .iter()
                    .map(|listener| (*listener, state.new_uid()))
                    .collect();

            for (tcp_listener, connection) in accept_pending {
                state
                    .substate_mut::<TcpServerState>()
                    .new_connection(connection, tcp_listener);

                dispatcher.dispatch(TcpPureAction::Accept {
                    connection,
                    tcp_listener,
                    on_result: callback!(|(connection: Uid, result: ConnectionResult)| {
                        TcpServerInputAction::AcceptResult { connection, result }
                    }),
                });
            }
        }
        Act::In(TcpServerInputAction::AcceptResult { connection, result }) => {
            let (server_uid, server) = state
                .substate_mut::<TcpServerState>()
                .get_connection_server_mut(&connection);
            let ConnectionResult::Incoming(result) = result else {
                unreachable!()
            };

            match result {
                // when reach max allowed connections we close it without notifications
                // TODO: this could probably better handled at low-level by changing the
                // TcpListener backlog. Currently, MIO sets a fixed value of 1024.
                AcceptResult::Success if server.connections.len() > server.max_connections => {
                    dispatcher.dispatch(TcpPureAction::Close {
                        connection,
                        on_result: callback!(|connection: Uid| {
                            TcpServerInputAction::CloseInternalResult { connection }
                        }),
                    })
                }
                // otherwise we notify the model user of the new connection.
                AcceptResult::Success => {
                    dispatcher.dispatch_back(&server.on_new_connection, (*server_uid, connection))
                }
                // No new connections, ignore.
                AcceptResult::WouldBlock => server.remove_connection(&connection),
                // Warn about accept errors, but no user notification.
                AcceptResult::Error(error) => {
                    warn!(
                        "|TCP_SERVER| accept connection {:?} failed: {:?}",
                        connection, error
                    );
                    server.remove_connection(&connection);
                }
            }
        }
        Act::In(TcpServerInputAction::CloseInternalResult { connection }) => {
            let (_, server) = state
                .substate_mut::<TcpServerState>()
                .get_connection_server_mut(&connection);

            server.remove_connection(&connection);
        }
        Act::Pure(TcpServerPureAction::Close { connection }) => {
            dispatcher.dispatch(TcpPureAction::Close {
                connection,
                on_result: callback!(|connection: Uid| {
                    TcpServerInputAction::CloseResult { connection }
                }),
            })
        }
        Act::In(TcpServerInputAction::CloseResult { connection }) => {
            let (&uid, server) = state
                .substate_mut::<TcpServerState>()
                .get_connection_server_mut(&connection);

            dispatcher.dispatch_back(&server.on_close_connection, (uid, connection));
            server.remove_connection(&connection);
        }
        Act::Pure(TcpServerPureAction::Send {
            uid,
            connection,
            data,
            timeout,
            on_result,
        }) => {
            state
                .substate_mut::<TcpServerState>()
                .new_send_request(&uid, connection, on_result);

            dispatcher.dispatch(TcpPureAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result: callback!(|(uid: Uid, result: SendResult)| {
                    TcpServerInputAction::SendResult { uid, result }
                }),
            });
        }
        Act::In(TcpServerInputAction::SendResult { uid, result }) => {
            let SendRequest {
                connection,
                on_result,
            } = state
                .substate_mut::<TcpServerState>()
                .take_send_request(&uid);

            if let SendResult::Error(_) = result {
                dispatcher.dispatch(TcpPureAction::Close {
                    connection,
                    on_result: callback!(|connection: Uid| {
                        TcpServerInputAction::CloseResult { connection }
                    }),
                });
            }

            dispatcher.dispatch_back(&on_result, (uid, result))
        }
        Act::Pure(TcpServerPureAction::Recv {
            uid,
            connection,
            count,
            timeout,
            on_result,
        }) => {
            state
                .substate_mut::<TcpServerState>()
                .new_recv_request(&uid, connection, on_result);

            dispatcher.dispatch(TcpPureAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result: callback!(|(uid: Uid, result: RecvResult)| {
                    TcpServerInputAction::RecvResult { uid, result }
                }),
            });
        }
        Act::In(TcpServerInputAction::RecvResult { uid, result }) => {
            let RecvRequest {
                connection,
                on_result,
            } = state
                .substate_mut::<TcpServerState>()
                .take_recv_request(&uid);

            if let RecvResult::Error(_) = result {
                dispatcher.dispatch(TcpPureAction::Close {
                    connection,
                    on_result: callback!(|connection: Uid| {
                        TcpServerInputAction::CloseResult { connection }
                    }),
                });
            }

            dispatcher.dispatch_back(&on_result, (uid, result))
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
        process_action(state, Act::In(action), dispatcher)
    }
}

impl PureModel for TcpServerState {
    type Action = TcpServerPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::Pure(action), dispatcher)
    }
}



fn handle_poll_result(
    server_state: &mut TcpServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: TcpPollResult,
) -> BTreeSet<Uid> {
    let PollRequest { on_result, .. } = server_state.take_poll_request();
    let mut accept_list = BTreeSet::new();

    match result {
        Ok(events) => {
            let mut removed_list = BTreeSet::new();

            for (server, event) in events {
                let Event::Listener(listener_event) = event else {
                    panic!("Unrequested event type {:?} for {:?}", event, server)
                };

                match listener_event {
                    ListenerEvent::AcceptPending => {
                        accept_list.insert(server);
                    }
                    ListenerEvent::AllAccepted => (),
                    ListenerEvent::Closed | ListenerEvent::Error => {
                        removed_list.insert(server);
                    }
                }
            }

            let result = if !removed_list.is_empty() {
                removed_list
                    .iter()
                    .for_each(|uid| server_state.remove_server(uid));

                accept_list.clear();

                Err(format!(
                    "Server(s) (Uid(s): {:?}) in closed or invalid state",
                    removed_list
                ))
            } else {
                Ok(())
            };

            dispatcher.dispatch_back(&on_result, (uid, result));
        }
        Err(err) => dispatcher.dispatch_back(&on_result, (uid, Err::<(), String>(err))),
    }

    accept_list
}

