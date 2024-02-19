use super::{action::PnetEchoClientAction, state::PnetEchoClientState};
use crate::{
    automaton::{
        action::{Dispatcher, Timeout},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::{
            pnet::client::{action::PnetClientAction, state::PnetClientState},
            tcp::action::{TcpAction, TcpPollEvents},
        },
        prng::state::PRNGState,
        tests::echo_client::state::{EchoClientConfig, EchoClientStatus},
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
            .model_pure::<Self>()
    }
}

impl PureModel for PnetEchoClientState {
    type Action = PnetEchoClientAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetEchoClientAction::Tick => {
                // Top-most model first task is to update the state-machine time.
                if update_time(state, dispatcher) {
                    // The next `EchoClientAction::Tick` will have the updated time.
                    return;
                }

                let PnetEchoClientState {
                    status,
                    config: EchoClientConfig { poll_timeout, .. },
                    ..
                } = state.substate_mut();

                match status {
                    EchoClientStatus::Init => {
                        // Init TCP model
                        dispatcher.dispatch(TcpAction::Init {
                            instance: state.new_uid(),
                            on_success: callback!(|instance: Uid| PnetEchoClientAction::InitSuccess { instance }),
                            on_error: callback!(|(instance: Uid, error: String)| PnetEchoClientAction::InitError { instance, error }),
                        })
                    }
                    EchoClientStatus::Connecting
                    | EchoClientStatus::Connected { .. }
                    | EchoClientStatus::Sending { .. }
                    | EchoClientStatus::Receiving { .. } => {
                        let timeout = Timeout::Millis(*poll_timeout);
                        // If the client is already initialized then we poll on each "tick".
                        dispatcher.dispatch(PnetClientAction::Poll {
                            uid: state.new_uid(),
                            timeout,
                            on_success: callback!(|(uid: Uid, events: TcpPollEvents)| PnetEchoClientAction::PollSuccess { uid, events }),
                            on_error: callback!(|(uid: Uid, error: String)| PnetEchoClientAction::PollError { uid, error }),
                        })
                    }
                }
            }
            PnetEchoClientAction::InitSuccess { .. } => {
                let connection = state.new_uid();
                let client_state: &mut PnetEchoClientState = state.substate_mut();

                client_state.status = EchoClientStatus::Connecting;
                connect(client_state, connection, dispatcher);
            }
            PnetEchoClientAction::InitError { error, .. } => {
                panic!("Client initialization failed: {}", error)
            }
            PnetEchoClientAction::ConnectSuccess { connection } => {
                let PnetEchoClientState {
                    status,
                    connection_attempt,
                    ..
                } = state.substate_mut();

                if let EchoClientStatus::Connecting = status {
                    *status = EchoClientStatus::Connected { connection };
                    *connection_attempt = 0;
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::ConnectTimeout { connection } => {
                let new_connection_uid = state.new_uid();
                let PnetEchoClientState {
                    status,
                    connection_attempt,
                    config:
                        EchoClientConfig {
                            max_connection_attempts,
                            ..
                        },
                    ..
                } = state.substate_mut();

                if let EchoClientStatus::Connecting = status {
                    *connection_attempt += 1;

                    warn!(
                        "|PNET_ECHO_CLIENT| connection {:?} timeout, reconnection attempt {}",
                        connection, connection_attempt
                    );

                    assert!(connection_attempt < max_connection_attempts);
                    connect(state.substate_mut(), new_connection_uid, dispatcher);
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::ConnectError { connection, error } => {
                let new_connection_uid = state.new_uid();
                let PnetEchoClientState {
                    status,
                    connection_attempt,
                    config:
                        EchoClientConfig {
                            max_connection_attempts,
                            ..
                        },
                    ..
                } = state.substate_mut();

                if let EchoClientStatus::Connecting = status {
                    *connection_attempt += 1;

                    warn!(
                        "|PNET_ECHO_CLIENT| connection {:?} error: {}, reconnection attempt {}",
                        connection, error, connection_attempt
                    );

                    assert!(connection_attempt < max_connection_attempts);
                    connect(state.substate_mut(), new_connection_uid, dispatcher);
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::CloseEvent { connection } => {
                info!("|PNET_ECHO_CLIENT| connection {:?} closed", connection);

                let new_connection_uid = state.new_uid();
                let client_state: &mut PnetEchoClientState = state.substate_mut();

                client_state.status = EchoClientStatus::Connecting;
                connect(client_state, new_connection_uid, dispatcher);
            }
            PnetEchoClientAction::PollSuccess { .. } => {
                // Send random data on every poll if there are no pending send/recv requests.
                if let PnetEchoClientState {
                    status: EchoClientStatus::Connected { connection },
                    config: EchoClientConfig { max_send_size, .. },
                    ..
                } = state.substate()
                {
                    let connection = *connection;
                    let max_send_size = *max_send_size;
                    let request = state.new_uid();
                    let prng: &mut PRNGState = state.substate_mut();
                    let random_size = prng.rng.gen_range(1..max_send_size) as usize;
                    let mut data: Vec<u8> = vec![0; random_size];

                    prng.rng.fill_bytes(&mut data[..]);

                    state.substate_mut::<PnetEchoClientState>().status =
                        EchoClientStatus::Sending {
                            connection,
                            request,
                            data: data.clone(),
                        };

                    dispatcher.dispatch(PnetClientAction::Send {
                        uid: request,
                        connection,
                        data: data.into(),
                        timeout: Timeout::Millis(200),
                        on_success: callback!(|uid: Uid| PnetEchoClientAction::SendSuccess { uid }),
                        on_timeout: callback!(|uid: Uid| PnetEchoClientAction::SendTimeout { uid }),
                        on_error: callback!(|(uid: Uid, error: String)| PnetEchoClientAction::SendError { uid, error })
                    });
                }
            }
            PnetEchoClientAction::PollError { uid, error } => {
                panic!("Poll {:?} failed: {}", uid, error)
            }
            PnetEchoClientAction::SendSuccess { uid } => {
                // Receive back what we sent
                if let PnetEchoClientState {
                    status:
                        EchoClientStatus::Sending {
                            connection,
                            request,
                            data,
                        },
                    config:
                        EchoClientConfig {
                            min_rnd_timeout,
                            max_rnd_timeout,
                            ..
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;
                    let sent_data = data.clone();
                    let count = sent_data.len();

                    // We randomize client's recv timeout to force it fail sometimes
                    let timeout_range = *min_rnd_timeout..*max_rnd_timeout;
                    let prng: &mut PRNGState = state.substate_mut();
                    let timeout = Timeout::Millis(prng.rng.gen_range(timeout_range));
                    let request = state.new_uid();

                    info!(
                        "|PNET_ECHO_CLIENT| dispatching recv request {:?} ({} bytes) from connection {:?} with timeout {:?}",
                        request, count, connection, timeout
                    );

                    state.substate_mut::<PnetEchoClientState>().status =
                        EchoClientStatus::Receiving {
                            connection,
                            request,
                            sent_data,
                        };

                    dispatcher.dispatch(PnetClientAction::Recv {
                        uid: request,
                        connection,
                        count,
                        timeout,
                        on_success: callback!(|(uid: Uid, data: Vec<u8>)| PnetEchoClientAction::RecvSuccess { uid, data }),
                        on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetEchoClientAction::RecvTimeout { uid, partial_data }),
                        on_error: callback!(|(uid: Uid, error: String)| PnetEchoClientAction::RecvError { uid, error }),
                    });
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::SendTimeout { uid } => {
                if let PnetEchoClientState {
                    status: EchoClientStatus::Sending { connection, .. },
                    ..
                } = state.substate()
                {
                    let connection = *connection;
                    warn!(
                        "|PNET_ECHO_CLIENT| send {:?} timeout to connection {:?}",
                        uid, connection
                    );
                    dispatcher.dispatch(PnetClientAction::Close { connection })
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::SendError { uid, error } => {
                if let PnetEchoClientState {
                    status: EchoClientStatus::Sending { connection, .. },
                    ..
                } = state.substate()
                {
                    warn!(
                        "|PNET_ECHO_CLIENT| send {:?} to connection {:?} error: {}",
                        uid, connection, error
                    );
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::RecvSuccess { uid, data } => {
                if let PnetEchoClientState {
                    status:
                        EchoClientStatus::Receiving {
                            connection,
                            request,
                            sent_data,
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;

                    if *sent_data != data {
                        panic!("Data mismatch: {:?} != {:?}", sent_data, data)
                    }

                    state.substate_mut::<PnetEchoClientState>().status =
                        EchoClientStatus::Connected { connection };

                    info!(
                        "|PNET_ECHO_CLIENT| recv {:?} from connection {:?}, data matches.",
                        uid, connection
                    );
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::RecvTimeout { uid, .. } => {
                if let PnetEchoClientState {
                    status:
                        EchoClientStatus::Receiving {
                            connection,
                            request,
                            ..
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;

                    warn!(
                        "|PNET_ECHO_CLIENT| recv {:?} timeout from connection {:?}",
                        uid, connection
                    );
                    dispatcher.dispatch(PnetClientAction::Close { connection })
                } else {
                    unreachable!()
                }
            }
            PnetEchoClientAction::RecvError { uid, error } => {
                if let PnetEchoClientState {
                    status:
                        EchoClientStatus::Receiving {
                            connection,
                            request,
                            ..
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;

                    warn!(
                        "|PNET_ECHO_CLIENT| recv {:?} from connection {:?} error: {}",
                        uid, connection, error
                    );
                } else {
                    unreachable!()
                }
            }
        }
    }
}

fn connect(client_state: &PnetEchoClientState, connection: Uid, dispatcher: &mut Dispatcher) {
    let PnetEchoClientState {
        config:
            EchoClientConfig {
                connect_to_address,
                connect_timeout,
                ..
            },
        ..
    } = client_state;

    dispatcher.dispatch(PnetClientAction::Connect {
        connection,
        address: connect_to_address.clone(),
        timeout: connect_timeout.clone(),
        on_success: callback!(|connection: Uid| PnetEchoClientAction::ConnectSuccess { connection }),
        on_timeout: callback!(|connection: Uid| PnetEchoClientAction::ConnectTimeout { connection }),
        on_error: callback!(|(connection: Uid, error: String)| PnetEchoClientAction::ConnectError { connection, error }),
        on_close: callback!(|connection: Uid| PnetEchoClientAction::CloseEvent { connection })
    });
}
