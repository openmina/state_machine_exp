use super::{
    action::{SimpleClientInputAction, SimpleClientTickAction},
    state::{SimpleClientConfig, SimpleClientState},
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
        net::pnet::client::action::PnetClientPureAction,
        net::{
            pnet::client::state::PnetClientState,
            tcp::action::{ConnectResult, Event, RecvResult, SendResult, TcpPureAction},
        },
        prng::state::PRNGState,
        time::model::update_time,
    },
};
use core::panic;
use log::{info, warn};

impl RegisterModel for SimpleClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PRNGState>()
            .register::<PnetClientState>()
            .model_pure_and_input::<Self>()
    }
}

impl PureModel for SimpleClientState {
    type Action = SimpleClientTickAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        _action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        if update_time(state, dispatcher) {
            return;
        }

        let SimpleClientState { ready, config, .. } = state.substate_mut();

        if !*ready {
            // Init TCP model
            dispatcher.dispatch(TcpPureAction::Init {
                instance: state.new_uid(),
                on_result: callback!(|(instance: Uid, result: OrError<()>)| {
                    SimpleClientInputAction::InitResult { instance, result }
                }),
            })
        } else {
            let timeout = Timeout::Millis(config.poll_timeout);
            // If the client is already initialized then we poll on each "tick".
            dispatcher.dispatch(PnetClientPureAction::Poll {
                uid: state.new_uid(),
                timeout,
                on_result: callback!(|(uid: Uid, result: OrError<Vec<(Uid, Event)>>)| {
                    SimpleClientInputAction::PollResult { uid, result }
                }),
            })
        }
    }
}

impl InputModel for SimpleClientState {
    type Action = SimpleClientInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            SimpleClientInputAction::InitResult { result, .. } => match result {
                Ok(_) => {
                    let connection = state.new_uid();
                    let client_state: &mut SimpleClientState = state.substate_mut();

                    client_state.ready = true;
                    connect(client_state, dispatcher, connection);
                }
                Err(error) => panic!("Client initialization failed: {}", error),
            },
            SimpleClientInputAction::ConnectResult { connection, result } => match result {
                ConnectResult::Success => {
                    let client_state: &mut SimpleClientState = state.substate_mut();

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
            SimpleClientInputAction::Closed { connection } => {
                info!("|PNET_CLIENT| connection {:?} closed", connection);
                dispatcher.halt()
            }
            SimpleClientInputAction::PollResult { uid, result, .. } => match result {
                Ok(_) => {
                    if let SimpleClientState {
                        connection: Some(connection),
                        send_request,
                        recv_request,
                        config:
                            SimpleClientConfig {
                                recv_size,
                                send_data,
                                ..
                            },
                        ..
                    } = state.substate()
                    {
                        let connection = *connection;
                        let send_request = *send_request;
                        let recv_request = *recv_request;
                        let count = *recv_size;

                        if send_request.is_none() {
                            let send_data = send_data.clone();
                            let uid = state.new_uid();

                            dispatcher.dispatch(PnetClientPureAction::Send {
                                uid,
                                connection,
                                data: send_data,
                                timeout: Timeout::Millis(2000),
                                on_result: callback!(|(uid: Uid, result: SendResult)| {
                                    SimpleClientInputAction::SendResult { uid, result }
                                }),
                            });

                            state.substate_mut::<SimpleClientState>().send_request = Some(uid);
                        }

                        if recv_request.is_none() {
                            let uid = state.new_uid();

                            dispatcher.dispatch(PnetClientPureAction::Recv {
                                uid,
                                connection,
                                count,
                                timeout: Timeout::Millis(2000),
                                on_result: callback!(|(uid: Uid, result: RecvResult)| {
                                    SimpleClientInputAction::RecvResult { uid, result }
                                }),
                            });

                            state.substate_mut::<SimpleClientState>().recv_request = Some(uid);
                        }
                    }
                }
                Err(error) => panic!("Poll {:?} failed: {}", uid, error),
            },
            SimpleClientInputAction::SendResult { uid, result } => {
                let client_state: &mut SimpleClientState = state.substate_mut();
                let connection = client_state
                    .connection
                    .expect("client not connected during SendResult action");
                let request = client_state
                    .send_request
                    .expect("no request for this SendResult");

                assert_eq!(request, uid);

                match result {
                    SendResult::Success => (),
                    SendResult::Timeout => {
                        dispatcher.dispatch(PnetClientPureAction::Close { connection });
                        warn!("|PNET_CLIENT| send {:?} timeout", uid)
                    }
                    SendResult::Error(error) => {
                        warn!("|PNET_CLIENT| send {:?} error: {:?}", uid, error)
                    }
                };
            }
            SimpleClientInputAction::RecvResult { uid, result } => {
                let client_state: &mut SimpleClientState = state.substate_mut();
                let connection = client_state
                    .connection
                    .expect("client not connected during RecvResult action");
                let request = client_state
                    .recv_request
                    .expect("no request for this RecvResult");

                assert_eq!(request, uid);

                match result {
                    RecvResult::Success(data_received) => {
                        dispatcher.dispatch(PnetClientPureAction::Close { connection });
                        info!(
                            "|PNET_CLIENT| recv: {}",
                            String::from_utf8(data_received).unwrap()
                        )
                    }
                    RecvResult::Timeout(partial_data) => {
                        dispatcher.dispatch(PnetClientPureAction::Close { connection });
                        info!(
                            "|PNET_CLIENT| recv (timeout): {}",
                            String::from_utf8(partial_data).unwrap()
                        )
                    }
                    RecvResult::Error(error) => {
                        warn!("|PNET_CLIENT| error: {:?}", error)
                    }
                }
            }
        }
    }
}

fn connect(client_state: &SimpleClientState, dispatcher: &mut Dispatcher, connection: Uid) {
    let SimpleClientState {
        config:
            SimpleClientConfig {
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
            SimpleClientInputAction::Closed { connection }
        }),
        on_result: callback!(|(connection: Uid, result: ConnectResult)| {
            SimpleClientInputAction::ConnectResult {
                connection,
                result
            }
        }),
    });
}

fn reconnect(
    client_state: &mut SimpleClientState,
    dispatcher: &mut Dispatcher,
    connection: Uid,
    new_connection_uid: Uid,
    error: String,
) {
    client_state.connection_attempt += 1;

    warn!(
        "|PNET_CLIENT| connection {:?} error: {}, reconnection attempt {}",
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
