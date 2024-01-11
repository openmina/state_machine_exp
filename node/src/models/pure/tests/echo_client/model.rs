use super::{
    action::{EchoClientInputAction, EchoClientTickAction},
    state::EchoClientState,
};
use crate::{
    automaton::{
        action::{Dispatcher, ResultDispatch, Timeout},
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    models::pure::{
        prng::state::PRNGState,
        tcp::action::{ConnectResult, ConnectionResult, RecvResult, SendResult, TcpPureAction},
        tcp_client::{action::TcpClientPureAction, state::TcpClientState},
        tests::echo_client::state::{EchoClientConfig, RecvRequest, SendRequest},
        time::model::update_time,
    },
};
use core::panic;
use log::{info, warn};
use rand::{Rng, RngCore};
use std::rc::Rc;

// The `EchoClientState` acts as a simulated echo client, used for testing the
// functionality of the state-machine and its related models (`TcpClientState`,
// `TcpState`, `MioState`, `TimeState`). The echo client communicates with an
// echo server, which sends back any data it receives from the client.
//
// The `PureModel` implementation of the `EchoClientState` model processes
// `EchoClientTickAction` actions that are dispatched on each "tick" of the
// state-machine loop.
//
// During each "tick", the model performs two key tasks:
//
// 1. Updates the current time tracked by the state-machine.
//
// 2. Checks if the TCP client is ready. If it's not, the model initializes
//    the TCP client. If it is ready, a poll action is dispatched.
//
// The `InputModel` implementation of the `EchoClientState` model handles the
// rest of the model's logic:
//
// - It completes the initialization of the TCP client and connects it to the
//   echo server. If the connection request fails, the client makes up to
//   `max_connection_attempts` attempts to reconnect.
//   If this limit is exceeded, the client panics.
//
// - For each poll result the client sends random data to the echo server.
//   The function `send_random_data_to_server` is used for this purpose. The
//   size and content of this data are randomly generated using the `PRNGState`
//   model.
//
// - After sending data, the client dispatches a receive action to read the
//   server's response. The function `recv_from_server_with_random_timeout` is
//   used to receive data from the server. This function uses a random timeout
//   (generated using the `PRNGState` model) to simulate different network
//   conditions.
//
// - When it receives data from the server, the client checks if the received
//   data matches the sent data. If not, the client panics.
//

// This model depends on `PRNGState` and `TcpClientState`.
impl RegisterModel for EchoClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PRNGState>()
            .register::<TcpClientState>()
            .model_pure_and_input::<Self>()
    }
}

impl PureModel for EchoClientState {
    type Action = EchoClientTickAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        _action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        // Top-most model first task is to update the state-machine time.
        if update_time(state, dispatcher) {
            // The next `EchoClientPureAction::Tick` will have the updated time.
            return;
        }

        let EchoClientState { ready, config, .. } = state.substate_mut();

        if !*ready {
            // Init TCP model
            dispatcher.dispatch(TcpPureAction::Init {
                instance: state.new_uid(),
                on_result: ResultDispatch::new(|(instance, result)| {
                    EchoClientInputAction::InitResult { instance, result }.into()
                }),
            })
        } else {
            let timeout = Timeout::Millis(config.poll_timeout);
            // If the client is already initialized then we poll on each "tick".
            dispatcher.dispatch(TcpClientPureAction::Poll {
                uid: state.new_uid(),
                timeout,
                on_result: ResultDispatch::new(|(uid, result)| {
                    EchoClientInputAction::PollResult { uid, result }.into()
                }),
            })
        }
    }
}

impl InputModel for EchoClientState {
    type Action = EchoClientInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            EchoClientInputAction::InitResult { result, .. } => match result {
                Ok(_) => {
                    let connection = state.new_uid();
                    let client_state: &mut EchoClientState = state.substate_mut();

                    client_state.ready = true;
                    connect(client_state, dispatcher, connection);
                }
                Err(error) => panic!("Client initialization failed: {}", error),
            },
            EchoClientInputAction::ConnectResult { connection, result } => {
                let ConnectionResult::Outgoing(result) = result else {
                    unreachable!()
                };

                match result {
                    ConnectResult::Success => {
                        let client_state: &mut EchoClientState = state.substate_mut();

                        client_state.connection_attempt = 0;
                        client_state.connection = Some(connection);
                    }
                    ConnectResult::Timeout => {
                        let new_connection_uid = state.new_uid();

                        reconnect(
                            state.substate_mut(),
                            dispatcher,
                            connection,
                            new_connection_uid,
                            "timeout".to_string(),
                        )
                    }
                    ConnectResult::Error(error) => {
                        let new_connection_uid = state.new_uid();

                        reconnect(
                            state.substate_mut(),
                            dispatcher,
                            connection,
                            new_connection_uid,
                            error,
                        )
                    }
                }
            }
            EchoClientInputAction::Closed { connection } => {
                info!("|ECHO_CLIENT| connection {:?} closed", connection);

                let connection = state.new_uid();
                let client_state: &mut EchoClientState = state.substate_mut();

                client_state.connection = None;
                connect(client_state, dispatcher, connection);
            }
            EchoClientInputAction::PollResult { uid, result, .. } => match result {
                Ok(_) => {
                    // Send random data on every poll if there are no pending send/recv requests.
                    if let EchoClientState {
                        connection: Some(connection),
                        send_request: None,
                        recv_request: None,
                        config: EchoClientConfig { max_send_size, .. },
                        ..
                    } = state.substate()
                    {
                        send_random_data_to_server(state, dispatcher, *connection, *max_send_size)
                    }
                }
                Err(error) => panic!("Poll {:?} failed: {}", uid, error),
            },
            EchoClientInputAction::SendResult { uid, result } => {
                let client_state: &mut EchoClientState = state.substate_mut();
                let connection = client_state
                    .connection
                    .expect("client not connected during SendResult action");
                let request = client_state
                    .send_request
                    .take()
                    .expect("no SendRequest for this SendResult");

                assert_eq!(request.uid, uid);

                match result {
                    // if random data was sent successfully, we wait for the server's response
                    SendResult::Success => recv_from_server_with_random_timeout(
                        state,
                        dispatcher,
                        connection,
                        request.data,
                    ),
                    SendResult::Timeout => {
                        dispatcher.dispatch(TcpClientPureAction::Close { connection });
                        warn!("|ECHO_CLIENT| send {:?} timeout", uid)
                    }
                    SendResult::Error(error) => {
                        warn!("|ECHO_CLIENT| send {:?} error: {:?}", uid, error)
                    }
                };
            }
            EchoClientInputAction::RecvResult { uid, result } => {
                let client_state: &mut EchoClientState = state.substate_mut();
                let connection = client_state
                    .connection
                    .expect("client not connected during RecvResult action");
                let request = client_state
                    .recv_request
                    .take()
                    .expect("no RecvRequest for this RecvResult");

                assert_eq!(request.uid, uid);

                match result {
                    RecvResult::Success(data_received) => {
                        let data_sent = request.data.as_ref();

                        if data_sent != data_received {
                            panic!(
                                "Data mismatch:\nsent({:?})\nreceived({:?})",
                                data_sent, data_received
                            )
                        }
                        info!("|ECHO_CLIENT| recv {:?} data matches what was sent", uid);
                    }
                    RecvResult::Timeout(_) => {
                        dispatcher.dispatch(TcpClientPureAction::Close { connection });
                        warn!("|ECHO_CLIENT| recv {:?} timeout", uid)
                    }
                    RecvResult::Error(error) => {
                        warn!("|ECHO_CLIENT| recv {:?} error: {:?}", uid, error)
                    }
                }
            }
        }
    }
}

fn connect(client_state: &EchoClientState, dispatcher: &mut Dispatcher, connection: Uid) {
    let EchoClientState {
        config:
            EchoClientConfig {
                connect_to_address,
                connect_timeout: timeout,
                ..
            },
        ..
    } = client_state;

    dispatcher.dispatch(TcpClientPureAction::Connect {
        connection,
        address: connect_to_address.clone(),
        timeout: timeout.clone(),
        on_close_connection: ResultDispatch::new(|connection| {
            EchoClientInputAction::Closed { connection }.into()
        }),
        on_result: ResultDispatch::new(|(connection, result)| {
            EchoClientInputAction::ConnectResult { connection, result }.into()
        }),
    });
}

fn reconnect(
    client_state: &mut EchoClientState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    new_connection_uid: Uid,
    error: String,
) {
    client_state.connection_attempt += 1;

    warn!(
        "|ECHO_CLIENT| connection {:?} error: {}, reconnection attempt {}",
        connection, error, client_state.connection_attempt
    );

    if client_state.connection_attempt == client_state.config.max_connection_attempts {
        panic!(
            "Max reconnection attempts: {}",
            client_state.config.max_connection_attempts
        )
    }

    connect(client_state, dispatcher, new_connection_uid);
}

fn send_random_data_to_server<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    max_send_size: u64,
) {
    let prng_state: &mut PRNGState = state.substate_mut();
    let rnd_len = prng_state.rng.gen_range(1..max_send_size) as usize;
    let mut random_data: Vec<u8> = vec![0; rnd_len];

    prng_state.rng.fill_bytes(&mut random_data[..]);

    let request = SendRequest {
        uid: state.new_uid(),
        data: random_data.into(),
    };

    dispatcher.dispatch(TcpClientPureAction::Send {
        uid: request.uid.clone(),
        connection,
        data: request.data.clone(),
        timeout: Timeout::Millis(200),
        on_result: ResultDispatch::new(|(uid, result)| {
            EchoClientInputAction::SendResult { uid, result }.into()
        }),
    });

    state.substate_mut::<EchoClientState>().send_request = Some(request);
}

fn recv_from_server_with_random_timeout<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    data: Rc<[u8]>,
) {
    let uid = state.new_uid();
    let EchoClientState {
        config:
            EchoClientConfig {
                min_rnd_timeout,
                max_rnd_timeout,
                ..
            },
        ..
    } = state.substate();

    // We randomize client's recv timeout to force it fail sometimes
    let timeout_range = *min_rnd_timeout..*max_rnd_timeout;
    let prng_state: &mut PRNGState = state.substate_mut();
    let timeout = Timeout::Millis(prng_state.rng.gen_range(timeout_range));
    let count = data.len();

    info!(
        "|ECHO_CLIENT| dispatching recv request {:?} ({} bytes) from connection {:?} with timeout {:?}",
        uid, count, connection, timeout
    );

    dispatcher.dispatch(TcpClientPureAction::Recv {
        uid,
        connection,
        count,
        timeout,
        on_result: ResultDispatch::new(|(uid, result)| {
            EchoClientInputAction::RecvResult { uid, result }.into()
        }),
    });

    state.substate_mut::<EchoClientState>().recv_request = Some(RecvRequest { uid, data });
}
