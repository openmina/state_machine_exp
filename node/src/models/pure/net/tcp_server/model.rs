use super::{
    action::TcpServerAction,
    state::{PollRequest, RecvRequest, SendRequest, Server, TcpServerState},
};
use crate::{
    automaton::{
        action::{Dispatcher, OrError},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::net::tcp::{
        action::{
            AcceptResult, ConnectionResult, Event, ListenerEvent, RecvResult, SendResult,
            TcpAction, TcpPollResult,
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
        builder.register::<TcpState>().model_pure::<Self>()
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
                address,
                server,
                max_connections,
                on_new_connection,
                on_close_connection,
                on_result,
            } => {
                state.substate_mut::<TcpServerState>().new_server(
                    server,
                    max_connections,
                    on_new_connection,
                    on_close_connection,
                    on_result,
                );

                dispatcher.dispatch(TcpAction::Listen {
                    tcp_listener: server,
                    address,
                    on_result: callback!(|(server: Uid, result: OrError<()>)| {
                        TcpServerAction::NewResult { server, result }
                    }),
                });
            }
            TcpServerAction::NewResult { server, result } => {
                let server_state: &mut TcpServerState = state.substate_mut();
                let Server { on_result, .. } = server_state.get_server(&server);

                dispatcher.dispatch_back(on_result, (server, result.clone()));

                if result.is_err() {
                    server_state.remove_server(&server)
                }
            }
            TcpServerAction::Poll {
                uid,
                timeout,
                on_result,
            } => {
                let server_state: &mut TcpServerState = state.substate_mut();
                let objects = server_state.server_objects.keys().cloned().collect();

                server_state.set_poll_request(PollRequest { on_result });
                dispatcher.dispatch(TcpAction::Poll {
                    uid,
                    objects,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: OrError<Vec<(Uid, Event)>>)| {
                        TcpServerAction::PollResult { uid, result }
                    }),
                })
            }
            TcpServerAction::PollResult { uid, result } => {
                let accept_pending: Vec<_> =
                    handle_poll_result(state.substate_mut(), dispatcher, uid, result)
                        .iter()
                        .map(|listener| (*listener, state.new_uid()))
                        .collect();

                for (tcp_listener, connection) in accept_pending {
                    state
                        .substate_mut::<TcpServerState>()
                        .new_connection(connection, tcp_listener);

                    dispatcher.dispatch(TcpAction::Accept {
                        connection,
                        tcp_listener,
                        on_result: callback!(|(connection: Uid, result: ConnectionResult)| {
                            TcpServerAction::AcceptResult { connection, result }
                        }),
                    });
                }
            }
            TcpServerAction::AcceptResult { connection, result } => {
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
                        dispatcher.dispatch(TcpAction::Close {
                            connection,
                            on_result: callback!(|connection: Uid| {
                                TcpServerAction::CloseResult {
                                    connection,
                                    notify: false,
                                }
                            }),
                        })
                    }
                    // otherwise we notify the model user of the new connection.
                    AcceptResult::Success => dispatcher
                        .dispatch_back(&server.on_new_connection, (*server_uid, connection)),
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
            TcpServerAction::Close { connection } => dispatcher.dispatch(TcpAction::Close {
                connection,
                on_result: callback!(|connection: Uid| {
                    TcpServerAction::CloseResult {
                        connection,
                        notify: true,
                    }
                }),
            }),
            TcpServerAction::CloseResult { connection, notify } => {
                let (&uid, server) = state
                    .substate_mut::<TcpServerState>()
                    .get_connection_server_mut(&connection);

                if notify {
                    dispatcher.dispatch_back(&server.on_close_connection, (uid, connection));
                }

                server.remove_connection(&connection);
            }
            TcpServerAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<TcpServerState>()
                    .new_send_request(&uid, connection, on_result);

                dispatcher.dispatch(TcpAction::Send {
                    uid,
                    connection,
                    data,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: SendResult)| {
                        TcpServerAction::SendResult { uid, result }
                    }),
                });
            }
            TcpServerAction::SendResult { uid, result } => {
                let SendRequest {
                    connection,
                    on_result,
                } = state
                    .substate_mut::<TcpServerState>()
                    .take_send_request(&uid);

                if let SendResult::Error(_) = result {
                    dispatcher.dispatch(TcpAction::Close {
                        connection,
                        on_result: callback!(|connection: Uid| {
                            TcpServerAction::CloseResult {
                                connection,
                                notify: true,
                            }
                        }),
                    });
                }

                dispatcher.dispatch_back(&on_result, (uid, result))
            }
            TcpServerAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<TcpServerState>()
                    .new_recv_request(&uid, connection, on_result);

                dispatcher.dispatch(TcpAction::Recv {
                    uid,
                    connection,
                    count,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: RecvResult)| {
                        TcpServerAction::RecvResult { uid, result }
                    }),
                });
            }
            TcpServerAction::RecvResult { uid, result } => {
                let RecvRequest {
                    connection,
                    on_result,
                } = state
                    .substate_mut::<TcpServerState>()
                    .take_recv_request(&uid);

                if let RecvResult::Error(_) = result {
                    dispatcher.dispatch(TcpAction::Close {
                        connection,
                        on_result: callback!(|connection: Uid| {
                            TcpServerAction::CloseResult {
                                connection,
                                notify: true,
                            }
                        }),
                    });
                }

                dispatcher.dispatch_back(&on_result, (uid, result))
            }
        }
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
