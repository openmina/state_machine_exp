use super::{action::EchoServerAction, state::EchoServerState};
use crate::{
    automaton::{
        action::{Dispatcher, OrError, Timeout},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::tcp::action::{RecvResult, SendResult, TcpAction},
        net::tcp_server::{action::TcpServerAction, state::TcpServerState},
        tests::echo_server::state::Connection,
        time::model::update_time,
    },
};
use log::{info, warn};

// The `EchoServerState` model simulates an echo server, used for testing the
// functionality of the state-machine and its models (`TcpServerState`,
// `TcpState`, `MioState`, `TimeState`). The echo server receives data from an
// echo client and sends the same data back to the client.
//
// This model provides a high-level interface for testing the state-machine's
// handling of server-side TCP operations, which includes managing multiple
// client connections.
//
// The `PureModel` implementation of the `EchoServerState` model processes
// `EchoServerAction::Tick` actions that are dispatched at each "tick" of the
// state-machine loop.
//
// During each "tick", the model performs two key tasks:
//
// 1. Updates the current time tracked by the state-machine.
//
// 2. Checks if the server is ready. If it's not, the model initializes the
//    server. If the server is ready, a poll action is dispatched to check for
//    incoming data.
//
// The rest of the server's logic performs the following actions:
//
// - It completes the initialization of the server and starts listening for
//   incoming connections. If the initialization fails, the server panics.
//
// - For each poll result, the server receives data from connected clients.
//   The function `receive_data_from_clients` is used for this purpose.
//
// - After receiving data, the server sends the same data back to the client.
//   The function `send_back_received_data_to_client` is used for this purpose.
//
// - When it receives a `SendResult`, the server checks the result, closing the
//   connection if the result is a timeout with no partial data or an error.
//

// This model depends on `TcpServerState`.
impl RegisterModel for EchoServerState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.register::<TcpServerState>().model_pure::<Self>()
    }
}


impl PureModel for EchoServerState {
    type Action = EchoServerAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            EchoServerAction::Tick => {
                if update_time(state, dispatcher) {
                    return;
                }

                let EchoServerState { ready, config, .. } = state.substate_mut();

                if !*ready {
                    dispatcher.dispatch(TcpAction::Init {
                        instance: state.new_uid(),
                        on_result: callback!(|(instance: Uid, result: OrError<()>)| {
                            EchoServerAction::InitResult { instance, result }
                        }),
                    })
                } else {
                    let timeout = Timeout::Millis(config.poll_timeout);

                    dispatcher.dispatch(TcpServerAction::Poll {
                        uid: state.new_uid(),
                        timeout,
                        on_result: callback!(|(uid: Uid, result: OrError<()>)| {
                            EchoServerAction::PollResult { uid, result }
                        }),
                    })
                }
            }
            EchoServerAction::InitResult { result, .. } => match result {
                Ok(_) => {
                    let EchoServerState { config, .. } = state.substate();
                    let address = config.address.clone();
                    let max_connections = config.max_connections;

                    // Init TcpServer model
                    dispatcher.dispatch(TcpServerAction::New {
                        server: state.new_uid(),
                        address,
                        max_connections,
                        on_new_connection: callback!(|(_server: Uid, connection: Uid)| {
                            EchoServerAction::NewConnection { connection }
                        }),
                        on_close_connection: callback!(|(_server: Uid, connection: Uid)| {
                            EchoServerAction::Closed { connection }
                        }),
                        on_result: callback!(|(server: Uid, result: OrError<()>)| {
                            EchoServerAction::NewServerResult { server, result }
                        }),
                    });
                }
                Err(error) => panic!("Server initialization failed: {}", error),
            },
            EchoServerAction::NewServerResult { result, .. } => match result {
                Ok(_) => {
                    // Complete EchoServerState initialization
                    state.substate_mut::<EchoServerState>().ready = true;
                }
                Err(error) => panic!("Server initialization failed: {}", error),
            },
            EchoServerAction::NewConnection { connection } => {
                info!("|ECHO_SERVER| new connection {:?}", connection);
                state
                    .substate_mut::<EchoServerState>()
                    .new_connection(connection)
            }
            EchoServerAction::Closed { connection } => {
                info!("|ECHO_SERVER| connection {:?} closed", connection);
                state
                    .substate_mut::<EchoServerState>()
                    .remove_connection(&connection);
            }
            EchoServerAction::PollResult { uid, result, .. } => match result {
                Ok(_) => receive_data_from_clients(state, dispatcher),
                Err(error) => panic!("Poll {:?} failed: {}", uid, error),
            },
            EchoServerAction::RecvResult { uid, result } => {
                send_back_received_data_to_client(state.substate_mut(), dispatcher, uid, result)
            }
            EchoServerAction::SendResult { uid, result } => {
                let (&connection, Connection { recv_uid }) = state
                    .substate_mut::<EchoServerState>()
                    .find_connection_by_recv_uid(uid);

                *recv_uid = None;

                match result {
                    SendResult::Success => (),
                    SendResult::Timeout => {
                        dispatcher.dispatch(TcpServerAction::Close { connection });
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
    let server_state: &EchoServerState = state.substate();
    let timeout = Timeout::Millis(server_state.config.recv_timeout);
    let count = 1024;

    for connection in server_state.connections_to_recv() {
        let uid = state.new_uid();

        info!(
            "|ECHO_SERVER| dispatching recv request {:?} ({} bytes) from connection {:?} with timeout {:?}",
            uid, count, connection, timeout
        );

        dispatcher.dispatch(TcpServerAction::Recv {
            uid,
            connection,
            count,
            timeout: timeout.clone(),
            on_result: callback!(|(uid: Uid, result: RecvResult)| {
                EchoServerAction::RecvResult { uid, result }
            }),
        });

        state
            .substate_mut::<EchoServerState>()
            .get_connection_mut(&connection)
            .recv_uid = Some(uid);
    }
}

fn send_back_received_data_to_client(
    server_state: &mut EchoServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: RecvResult,
) {
    let (&connection, _) = server_state.find_connection_by_recv_uid(uid);

    match result {
        RecvResult::Success(data) | RecvResult::Timeout(data) => {
            // It is OK to get a timeout if it contains partial data (< 1024 bytes)
            if data.len() != 0 {
                dispatcher.dispatch(TcpServerAction::Send {
                    uid,
                    connection,
                    data: data.into(),
                    timeout: Timeout::Millis(100),
                    on_result: callback!(|(uid: Uid, result: SendResult)| {
                        EchoServerAction::SendResult { uid, result }
                    }),
                });
            } else {
                // On recv errors the connection is closed automatically by the TcpServer model.
                // Timeouts are not errors, so here we close it explicitly.
                dispatcher.dispatch(TcpServerAction::Close { connection });
                warn!("|ECHO_SERVER| recv {:?} timeout", uid)
            }
        }
        RecvResult::Error(error) => warn!("|ECHO_SERVER| recv {:?} error: {:?}", uid, error),
    }
}
