use super::{
    action::{PnetEchoServerInputAction, PnetEchoServerTickAction},
    state::PnetEchoServerState,
};
use crate::models::pure::net::pnet::server::state::PnetServerState;
use crate::models::pure::tests::echo_server_pnet::state::Connection;
use crate::{
    automaton::{
        action::{Dispatcher, Timeout},
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::pnet::server::action::PnetServerPureAction,
        net::tcp::action::{RecvResult, SendResult, TcpPureAction},
        time::model::update_time,
    },
};
use log::{info, warn};

impl RegisterModel for PnetEchoServerState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PnetServerState>()
            .model_pure_and_input::<Self>()
    }
}

impl PureModel for PnetEchoServerState {
    type Action = PnetEchoServerTickAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        _action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        if update_time(state, dispatcher) {
            return;
        }

        let PnetEchoServerState { ready, config, .. } = state.substate_mut();

        if !*ready {
            dispatcher.dispatch(TcpPureAction::Init {
                instance: state.new_uid(),
                on_result: callback!(|(instance: Uid, result: Result<(), String>)| {
                    PnetEchoServerInputAction::InitResult { instance, result }
                }),
            })
        } else {
            let timeout = Timeout::Millis(config.poll_timeout);

            dispatcher.dispatch(PnetServerPureAction::Poll {
                uid: state.new_uid(),
                timeout,
                on_result: callback!(|(uid: Uid, result: Result<(), String>)| {
                    PnetEchoServerInputAction::PollResult { uid, result }
                }),
            })
        }
    }
}

impl InputModel for PnetEchoServerState {
    type Action = PnetEchoServerInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetEchoServerInputAction::InitResult { result, .. } => match result {
                Ok(_) => {
                    let PnetEchoServerState { config, .. } = state.substate();
                    let address = config.address.clone();
                    let max_connections = config.max_connections;

                    // Init PnetServer model
                    dispatcher.dispatch(PnetServerPureAction::New {
                        server: state.new_uid(),
                        address,
                        max_connections,
                        on_new_connection: callback!(|(_server: Uid, connection: Uid)| {
                            PnetEchoServerInputAction::NewConnection { connection }
                        }),
                        on_close_connection: callback!(|(_server: Uid, connection: Uid)| {
                            PnetEchoServerInputAction::Closed { connection }
                        }),
                        on_result: callback!(|(server: Uid, result: Result<(), String>)| {
                            PnetEchoServerInputAction::NewServerResult { server, result }
                        }),
                    });
                }
                Err(error) => panic!("Server initialization failed: {}", error),
            },
            PnetEchoServerInputAction::NewServerResult { result, .. } => match result {
                Ok(_) => {
                    // Complete EchoServerState initialization
                    state.substate_mut::<PnetEchoServerState>().ready = true;
                }
                Err(error) => panic!("Server initialization failed: {}", error),
            },
            PnetEchoServerInputAction::NewConnection { connection } => {
                info!("|ECHO_SERVER| new connection {:?}", connection);
                state
                    .substate_mut::<PnetEchoServerState>()
                    .new_connection(connection)
            }
            PnetEchoServerInputAction::Closed { connection } => {
                info!("|ECHO_SERVER| connection {:?} closed", connection);
                state
                    .substate_mut::<PnetEchoServerState>()
                    .remove_connection(&connection);
            }
            PnetEchoServerInputAction::PollResult { uid, result, .. } => match result {
                Ok(_) => receive_data_from_clients(state, dispatcher),
                Err(error) => panic!("Poll {:?} failed: {}", uid, error),
            },
            PnetEchoServerInputAction::RecvResult { uid, result } => {
                send_back_received_data_to_client(state.substate_mut(), dispatcher, uid, result)
            }
            PnetEchoServerInputAction::SendResult { uid, result } => {
                let (&connection, Connection { recv_uid }) = state
                    .substate_mut::<PnetEchoServerState>()
                    .find_connection_by_recv_uid(uid);

                *recv_uid = None;

                match result {
                    SendResult::Success => (),
                    SendResult::Timeout => {
                        dispatcher.dispatch(PnetServerPureAction::Close { connection });
                        warn!("|ECHO_SERVER| send {:?} timeout", uid)
                    }
                    SendResult::Error(error) => {
                        warn!("|ECHO_SERVER| send {:?} error: {:?}", uid, error)
                    }
                }
            }
        }
    }
}

fn receive_data_from_clients<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
) {
    let server_state: &PnetEchoServerState = state.substate();
    let timeout = Timeout::Millis(server_state.config.recv_timeout);
    let count = 1024;

    for connection in server_state.connections_to_recv() {
        let uid = state.new_uid();

        info!(
            "|ECHO_SERVER| dispatching recv request {:?} ({} bytes) from connection {:?} with timeout {:?}",
            uid, count, connection, timeout
        );

        dispatcher.dispatch(PnetServerPureAction::Recv {
            uid,
            connection,
            count,
            timeout: timeout.clone(),
            on_result: callback!(|(uid: Uid, result: RecvResult)| {
                PnetEchoServerInputAction::RecvResult { uid, result }
            }),
        });

        state
            .substate_mut::<PnetEchoServerState>()
            .get_connection_mut(&connection)
            .recv_uid = Some(uid);
    }
}

fn send_back_received_data_to_client(
    server_state: &mut PnetEchoServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: RecvResult,
) {
    let (&connection, _) = server_state.find_connection_by_recv_uid(uid);

    match result {
        RecvResult::Success(data) | RecvResult::Timeout(data) => {
            // It is OK to get a timeout if it contains partial data (< 1024 bytes)
            if data.len() != 0 {
                dispatcher.dispatch(PnetServerPureAction::Send {
                    uid,
                    connection,
                    data: data.into(),
                    timeout: Timeout::Millis(100),
                    on_result: callback!(|(uid: Uid, result: SendResult)| {
                        PnetEchoServerInputAction::SendResult { uid, result }
                    }),
                });
            } else {
                // On recv errors the connection is closed automatically by the TcpServer model.
                // Timeouts are not errors, so here we close it explicitly.
                dispatcher.dispatch(PnetServerPureAction::Close { connection });
                warn!("|ECHO_SERVER| recv {:?} timeout", uid)
            }
        }
        RecvResult::Error(error) => warn!("|ECHO_SERVER| recv {:?} error: {:?}", uid, error),
    }
}
