use super::{
    action::{PnetEchoClientInputAction, PnetEchoClientTickAction},
    state::{EchoClientConfig, PnetEchoClientState, SendRequest},
};
use crate::{
    automaton::{
        action::{Dispatcher, OrError, Timeout},
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::pnet::client::{action::PnetClientPureAction, state::PnetClientState},
        net::tcp::action::{ConnectResult, Event, RecvResult, SendResult, TcpPureAction},
        prng::state::PRNGState,
        tests::echo_client_pnet::state::RecvRequest,
        time::model::update_time,
    },
};
use core::panic;
use log::{info, warn};
use rand::{Rng, RngCore};

impl RegisterModel for PnetEchoClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PRNGState>()
            .register::<PnetClientState>()
            .model_pure_and_input::<Self>()
    }
}

impl PureModel for PnetEchoClientState {
    type Action = PnetEchoClientTickAction;

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

        let PnetEchoClientState { ready, config, .. } = state.substate_mut();

        if !*ready {
            // Init TCP model
            dispatcher.dispatch(TcpPureAction::Init {
                instance: state.new_uid(),
                on_result: callback!(|(instance: Uid, result: OrError<()>)| {
                    PnetEchoClientInputAction::InitResult { instance, result }
                }),
            })
        } else {
            let timeout = Timeout::Millis(config.poll_timeout);
            // If the client is already initialized then we poll on each "tick".
            dispatcher.dispatch(PnetClientPureAction::Poll {
                uid: state.new_uid(),
                timeout,
                on_result: callback!(|(uid: Uid, result: OrError<Vec<(Uid, Event)>>)| {
                    PnetEchoClientInputAction::PollResult { uid, result }
                }),
            })
        }
    }
}

impl InputModel for PnetEchoClientState {
    type Action = PnetEchoClientInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetEchoClientInputAction::InitResult { result, .. } => match result {
                Ok(_) => {
                    let connection = state.new_uid();
                    let client_state: &mut PnetEchoClientState = state.substate_mut();

                    client_state.ready = true;
                    connect(client_state, dispatcher, connection);
                }
                Err(error) => panic!("Client initialization failed: {}", error),
            },
            PnetEchoClientInputAction::ConnectResult { connection, result } => match result {
                ConnectResult::Success => {
                    let client_state: &mut PnetEchoClientState = state.substate_mut();

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
            },
            PnetEchoClientInputAction::Closed { connection } => {
                info!("|ECHO_CLIENT| connection {:?} closed", connection);

                let connection = state.new_uid();
                let client_state: &mut PnetEchoClientState = state.substate_mut();

                client_state.connection = None;
                connect(client_state, dispatcher, connection);
            }
            PnetEchoClientInputAction::PollResult { uid, result, .. } => match result {
                Ok(_) => {
                    // Send random data on every poll if there are no pending send/recv requests.
                    if let PnetEchoClientState {
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
            PnetEchoClientInputAction::SendResult { uid, result } => {
                let client_state: &mut PnetEchoClientState = state.substate_mut();
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
                        dispatcher.dispatch(PnetClientPureAction::Close { connection });
                        warn!("|ECHO_CLIENT| send {:?} timeout", uid)
                    }
                    SendResult::Error(error) => {
                        warn!("|ECHO_CLIENT| send {:?} error: {:?}", uid, error)
                    }
                };
            }
            PnetEchoClientInputAction::RecvResult { uid, result } => {
                let client_state: &mut PnetEchoClientState = state.substate_mut();
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
                        dispatcher.dispatch(PnetClientPureAction::Close { connection });
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

fn connect(client_state: &PnetEchoClientState, dispatcher: &mut Dispatcher, connection: Uid) {
    let PnetEchoClientState {
        config:
            EchoClientConfig {
                connect_to_address,
                connect_timeout: timeout,
                ..
            },
        ..
    } = client_state;

    dispatcher.dispatch(PnetClientPureAction::Connect {
        connection,
        address: connect_to_address.clone(),
        timeout: timeout.clone(),
        on_close_connection: callback!(|connection: Uid| {
            PnetEchoClientInputAction::Closed { connection }
        }),
        on_result: callback!(|(connection: Uid, result: ConnectResult)| {
            PnetEchoClientInputAction::ConnectResult {
                connection,
                result
            }
        }),
    });
}

fn reconnect(
    client_state: &mut PnetEchoClientState,
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

    dispatcher.dispatch(PnetClientPureAction::Send {
        uid: request.uid.clone(),
        connection,
        data: request.data.clone(),
        timeout: Timeout::Millis(200),
        on_result: callback!(|(uid: Uid, result: SendResult)| {
            PnetEchoClientInputAction::SendResult { uid, result }
        }),
    });

    state.substate_mut::<PnetEchoClientState>().send_request = Some(request);
}

fn recv_from_server_with_random_timeout<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    data: Vec<u8>,
) {
    let uid = state.new_uid();
    let PnetEchoClientState {
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

    dispatcher.dispatch(PnetClientPureAction::Recv {
        uid,
        connection,
        count,
        timeout,
        on_result: callback!(|(uid: Uid, result: RecvResult)| {
            PnetEchoClientInputAction::RecvResult { uid, result }
        }),
    });

    state.substate_mut::<PnetEchoClientState>().recv_request = Some(RecvRequest { uid, data });
}
