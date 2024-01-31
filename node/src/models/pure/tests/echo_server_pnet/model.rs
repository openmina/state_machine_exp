use super::{action::PnetEchoServerAction, state::PnetEchoServerState};
use crate::models::pure::tests::echo_server_pnet::state::Connection;
use crate::{automaton::action::OrError, models::pure::net::pnet::server::state::PnetServerState};
use crate::{
    automaton::{
        action::{Dispatcher, Timeout},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::pnet::server::action::PnetServerAction,
        net::tcp::action::{RecvResult, SendResult, TcpAction},
        time::model::update_time,
    },
};
use log::{info, warn};

impl RegisterModel for PnetEchoServerState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.register::<PnetServerState>().model_pure::<Self>()
    }
}

impl PureModel for PnetEchoServerState {
    type Action = PnetEchoServerAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetEchoServerAction::Tick => {
                if update_time(state, dispatcher) {
                    return;
                }

                let PnetEchoServerState { ready, config, .. } = state.substate_mut();

                if !*ready {
                    dispatcher.dispatch(TcpAction::Init {
                        instance: state.new_uid(),
                        on_result: callback!(|(instance: Uid, result: OrError<()>)| {
                            PnetEchoServerAction::InitResult { instance, result }
                        }),
                    })
                } else {
                    let timeout = Timeout::Millis(config.poll_timeout);

                    dispatcher.dispatch(PnetServerAction::Poll {
                        uid: state.new_uid(),
                        timeout,
                        on_result: callback!(|(uid: Uid, result: OrError<()>)| {
                            PnetEchoServerAction::PollResult { uid, result }
                        }),
                    })
                }
            }
            PnetEchoServerAction::InitResult { result, .. } => match result {
                Ok(_) => {
                    let PnetEchoServerState { config, .. } = state.substate();
                    let address = config.address.clone();
                    let max_connections = config.max_connections;

                    // Init PnetServer model
                    dispatcher.dispatch(PnetServerAction::New {
                        server: state.new_uid(),
                        address,
                        max_connections,
                        on_new_connection: callback!(|(_server: Uid, connection: Uid)| {
                            PnetEchoServerAction::NewConnection { connection }
                        }),
                        on_close_connection: callback!(|(_server: Uid, connection: Uid)| {
                            PnetEchoServerAction::Closed { connection }
                        }),
                        on_result: callback!(|(server: Uid, result: OrError<()>)| {
                            PnetEchoServerAction::NewServerResult { server, result }
                        }),
                    });
                }
                Err(error) => panic!("Server initialization failed: {}", error),
            },
            PnetEchoServerAction::NewServerResult { result, .. } => match result {
                Ok(_) => {
                    // Complete EchoServerState initialization
                    state.substate_mut::<PnetEchoServerState>().ready = true;
                }
                Err(error) => panic!("Server initialization failed: {}", error),
            },
            PnetEchoServerAction::NewConnection { connection } => {
                info!("|ECHO_SERVER| new connection {:?}", connection);
                state
                    .substate_mut::<PnetEchoServerState>()
                    .new_connection(connection)
            }
            PnetEchoServerAction::Closed { connection } => {
                info!("|ECHO_SERVER| connection {:?} closed", connection);
                state
                    .substate_mut::<PnetEchoServerState>()
                    .remove_connection(&connection);
            }
            PnetEchoServerAction::PollResult { uid, result, .. } => match result {
                Ok(_) => receive_data_from_clients(state, dispatcher),
                Err(error) => panic!("Poll {:?} failed: {}", uid, error),
            },
            PnetEchoServerAction::RecvResult { uid, result } => {
                send_back_received_data_to_client(state.substate_mut(), dispatcher, uid, result)
            }
            PnetEchoServerAction::SendResult { uid, result } => {
                let (&connection, Connection { recv_uid }) = state
                    .substate_mut::<PnetEchoServerState>()
                    .find_connection_by_recv_uid(uid);

                *recv_uid = None;

                match result {
                    SendResult::Success => (),
                    SendResult::Timeout => {
                        dispatcher.dispatch(PnetServerAction::Close { connection });
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

        dispatcher.dispatch(PnetServerAction::Recv {
            uid,
            connection,
            count,
            timeout: timeout.clone(),
            on_result: callback!(|(uid: Uid, result: RecvResult)| {
                PnetEchoServerAction::RecvResult { uid, result }
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
                dispatcher.dispatch(PnetServerAction::Send {
                    uid,
                    connection,
                    data: data.into(),
                    timeout: Timeout::Millis(100),
                    on_result: callback!(|(uid: Uid, result: SendResult)| {
                        PnetEchoServerAction::SendResult { uid, result }
                    }),
                });
            } else {
                // On recv errors the connection is closed automatically by the TcpServer model.
                // Timeouts are not errors, so here we close it explicitly.
                dispatcher.dispatch(PnetServerAction::Close { connection });
                warn!("|ECHO_SERVER| recv {:?} timeout", uid)
            }
        }
        RecvResult::Error(error) => warn!("|ECHO_SERVER| recv {:?} error: {:?}", uid, error),
    }
}
