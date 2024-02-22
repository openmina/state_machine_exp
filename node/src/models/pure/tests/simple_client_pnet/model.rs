use super::{
    action::PnetSimpleClientAction,
    state::{ClientStatus, PnetSimpleClientConfig, PnetSimpleClientState},
};
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
        time::model::update_time,
    },
};
use core::panic;
use log::{info, warn};

impl RegisterModel for PnetSimpleClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PRNGState>()
            .register::<PnetClientState>()
            .model_pure::<Self>()
    }
}

impl PureModel for PnetSimpleClientState {
    type Action = PnetSimpleClientAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetSimpleClientAction::Tick => {
                // Top-most model first task is to update the state-machine time.
                if update_time(state, dispatcher) {
                    // The next `EchoClientAction::Tick` will have the updated time.
                    return;
                }

                let PnetSimpleClientState {
                    status,
                    config: PnetSimpleClientConfig { poll_timeout, .. },
                    ..
                } = state.substate_mut();

                match status {
                    ClientStatus::Init => {
                        // Init TCP model
                        dispatcher.dispatch(TcpAction::Init {
                            instance: state.new_uid(),
                            on_success: callback!(|instance: Uid| PnetSimpleClientAction::InitSuccess { instance }),
                            on_error: callback!(|(instance: Uid, error: String)| PnetSimpleClientAction::InitError { instance, error }),
                        })
                    }
                    ClientStatus::Connecting
                    | ClientStatus::Connected { .. }
                    | ClientStatus::Sending { .. }
                    | ClientStatus::Receiving { .. } => {
                        let timeout = Timeout::Millis(*poll_timeout);
                        // If the client is already initialized then we poll on each "tick".
                        dispatcher.dispatch(PnetClientAction::Poll {
                            uid: state.new_uid(),
                            timeout,
                            on_success: callback!(|(uid: Uid, events: TcpPollEvents)| PnetSimpleClientAction::PollSuccess { uid, events }),
                            on_error: callback!(|(uid: Uid, error: String)| PnetSimpleClientAction::PollError { uid, error }),
                        })
                    }
                    ClientStatus::TestCompleted => unreachable!(),
                }
            }
            PnetSimpleClientAction::InitSuccess { .. } => {
                let connection = state.new_uid();
                let client_state: &mut PnetSimpleClientState = state.substate_mut();

                client_state.status = ClientStatus::Connecting;
                connect(client_state, connection, dispatcher);
            }
            PnetSimpleClientAction::InitError { error, .. } => {
                panic!("Client initialization failed: {}", error)
            }
            PnetSimpleClientAction::ConnectSuccess { connection } => {
                let PnetSimpleClientState {
                    status,
                    connection_attempt,
                    ..
                } = state.substate_mut();

                if let ClientStatus::Connecting = status {
                    *status = ClientStatus::Connected { connection };
                    *connection_attempt = 0;
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::ConnectTimeout { connection } => {
                let new_connection_uid = state.new_uid();
                let PnetSimpleClientState {
                    status,
                    connection_attempt,
                    config:
                        PnetSimpleClientConfig {
                            max_connection_attempts,
                            ..
                        },
                    ..
                } = state.substate_mut();

                if let ClientStatus::Connecting = status {
                    *connection_attempt += 1;

                    warn!(
                        "|PNET_SIMPLE_CLIENT| connection {:?} timeout, reconnection attempt {}",
                        connection, connection_attempt
                    );

                    assert!(connection_attempt < max_connection_attempts);
                    connect(state.substate_mut(), new_connection_uid, dispatcher);
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::ConnectError { connection, error } => {
                let new_connection_uid = state.new_uid();
                let PnetSimpleClientState {
                    status,
                    connection_attempt,
                    config:
                        PnetSimpleClientConfig {
                            max_connection_attempts,
                            ..
                        },
                    ..
                } = state.substate_mut();

                if let ClientStatus::Connecting = status {
                    *connection_attempt += 1;

                    warn!(
                        "|PNET_SIMPLE_CLIENT| connection {:?} error: {}, reconnection attempt {}",
                        connection, error, connection_attempt
                    );

                    assert!(connection_attempt < max_connection_attempts);
                    connect(state.substate_mut(), new_connection_uid, dispatcher);
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::CloseEvent { connection } => {
                info!("|PNET_SIMPLE_CLIENT| connection {:?} closed", connection);

                if let PnetSimpleClientState {
                    status: ClientStatus::TestCompleted,
                    ..
                } = state.substate()
                {
                    dispatcher.halt()
                } else {
                    panic!("Connection lost without completing the test")
                }
            }
            PnetSimpleClientAction::PollSuccess { .. } => {
                if let PnetSimpleClientState {
                    status: ClientStatus::Connected { connection },
                    config: PnetSimpleClientConfig { send_data, .. },
                    ..
                } = state.substate()
                {
                    let connection = *connection;
                    let data = send_data.clone();
                    let request = state.new_uid();

                    state.substate_mut::<PnetSimpleClientState>().status = ClientStatus::Sending {
                        connection,
                        request,
                    };

                    dispatcher.dispatch(PnetClientAction::Send {
                        uid: request,
                        connection,
                        data,
                        timeout: Timeout::Millis(200),
                        on_success: callback!(|uid: Uid| PnetSimpleClientAction::SendSuccess { uid }),
                        on_timeout: callback!(|uid: Uid| PnetSimpleClientAction::SendTimeout { uid }),
                        on_error: callback!(|(uid: Uid, error: String)| PnetSimpleClientAction::SendError { uid, error })
                    });
                }
            }
            PnetSimpleClientAction::PollError { uid, error } => {
                panic!("Poll {:?} failed: {}", uid, error)
            }
            PnetSimpleClientAction::SendSuccess { uid } => {
                if let PnetSimpleClientState {
                    status:
                        ClientStatus::Sending {
                            connection,
                            request,
                        },
                    config:
                        PnetSimpleClientConfig {
                            recv_data,
                            recv_timeout,
                            ..
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;
                    let count = recv_data.len();
                    let timeout = recv_timeout.clone();
                    let request = state.new_uid();

                    state.substate_mut::<PnetSimpleClientState>().status =
                        ClientStatus::Receiving {
                            connection,
                            request,
                        };

                    dispatcher.dispatch(PnetClientAction::Recv {
                        uid: request,
                        connection,
                        count,
                        timeout,
                        on_success: callback!(|(uid: Uid, data: Vec<u8>)| PnetSimpleClientAction::RecvSuccess { uid, data }),
                        on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetSimpleClientAction::RecvTimeout { uid, partial_data }),
                        on_error: callback!(|(uid: Uid, error: String)| PnetSimpleClientAction::RecvError { uid, error }),
                    });
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::SendTimeout { uid } => {
                if let PnetSimpleClientState {
                    status: ClientStatus::Sending { connection, .. },
                    ..
                } = state.substate()
                {
                    let connection = *connection;
                    warn!(
                        "|PNET_SIMPLE_CLIENT| send {:?} timeout to connection {:?}",
                        uid, connection
                    );
                    dispatcher.dispatch(PnetClientAction::Close { connection })
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::SendError { uid, error } => {
                if let PnetSimpleClientState {
                    status: ClientStatus::Sending { connection, .. },
                    ..
                } = state.substate()
                {
                    warn!(
                        "|PNET_SIMPLE_CLIENT| send {:?} to connection {:?} error: {}",
                        uid, connection, error
                    );
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::RecvSuccess { uid, data } => {
                if let PnetSimpleClientState {
                    status:
                        ClientStatus::Receiving {
                            connection,
                            request,
                        },
                    config: PnetSimpleClientConfig { recv_data, .. },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;

                    if data == *recv_data {
                        state.substate_mut::<PnetSimpleClientState>().status =
                            ClientStatus::TestCompleted;
                    }

                    info!(
                        "|PNET_SIMPLE_CLIENT| recv {:?} from connection {:?}, data: {}",
                        uid,
                        connection,
                        String::from_utf8(data).unwrap()
                    );

                    dispatcher.dispatch(PnetClientAction::Close { connection });
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::RecvTimeout { uid, .. } => {
                if let PnetSimpleClientState {
                    status:
                        ClientStatus::Receiving {
                            connection,
                            request,
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;

                    warn!(
                        "|PNET_SIMPLE_CLIENT| recv {:?} timeout from connection {:?}",
                        uid, connection
                    );
                    dispatcher.dispatch(PnetClientAction::Close { connection })
                } else {
                    unreachable!()
                }
            }
            PnetSimpleClientAction::RecvError { uid, error } => {
                if let PnetSimpleClientState {
                    status:
                        ClientStatus::Receiving {
                            connection,
                            request,
                        },
                    ..
                } = state.substate()
                {
                    assert_eq!(uid, *request);
                    let connection = *connection;

                    warn!(
                        "|PNET_SIMPLE_CLIENT| recv {:?} from connection {:?} error: {}",
                        uid, connection, error
                    );
                } else {
                    unreachable!()
                }
            }
        }
    }
}

fn connect(client_state: &PnetSimpleClientState, connection: Uid, dispatcher: &mut Dispatcher) {
    let PnetSimpleClientState {
        config:
            PnetSimpleClientConfig {
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
        on_success: callback!(|connection: Uid| PnetSimpleClientAction::ConnectSuccess { connection }),
        on_timeout: callback!(|connection: Uid| PnetSimpleClientAction::ConnectTimeout { connection }),
        on_error: callback!(|(connection: Uid, error: String)| PnetSimpleClientAction::ConnectError { connection, error }),
        on_close: callback!(|connection: Uid| PnetSimpleClientAction::CloseEvent { connection })
    });
}
