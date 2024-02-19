use super::{action::PnetEchoServerAction, state::PnetEchoServerState};
use crate::automaton::state::Objects;
use crate::models::pure::net::pnet::server::state::PnetServerState;
use crate::models::pure::net::tcp::action::TcpAction;
use crate::models::pure::tests::echo_server::state::{
    Connection, EchoServerConfig, EchoServerStatus,
};
use crate::{
    automaton::{
        action::{Dispatcher, Timeout},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{net::pnet::server::action::PnetServerAction, time::model::update_time},
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
                // Top-most model first task is to update the state-machine time.
                if update_time(state, dispatcher) {
                    return;
                }

                let PnetEchoServerState {
                    status,
                    config: EchoServerConfig { poll_timeout, .. },
                    ..
                } = state.substate_mut();

                match status {
                    EchoServerStatus::Init => {
                        // Init TCP model
                        dispatcher.dispatch(TcpAction::Init {
                            instance: state.new_uid(),
                            on_success: callback!(|instance: Uid| PnetEchoServerAction::InitSuccess { instance }),
                            on_error: callback!(|(instance: Uid, error: String)| PnetEchoServerAction::InitError { instance, error }),
                        })
                    }
                    EchoServerStatus::Listening { .. } => {
                        let timeout = Timeout::Millis(*poll_timeout);

                        dispatcher.dispatch(PnetServerAction::Poll {
                            uid: state.new_uid(),
                            timeout,
                            on_success: callback!(|uid: Uid| PnetEchoServerAction::PollSuccess { uid }),
                            on_error: callback!(|(uid: Uid, error: String)| PnetEchoServerAction::PollError { uid, error }),
                        })
                    }
                }

                if update_time(state, dispatcher) {
                    return;
                }
            }
            PnetEchoServerAction::InitSuccess { .. } => {
                let PnetEchoServerState { config, .. } = state.substate();
                let address = config.address.clone();
                let max_connections = config.max_connections;

                // Init TcpServer model
                dispatcher.dispatch(PnetServerAction::New {
                    listener: state.new_uid(),
                    address,
                    max_connections,
                    on_success: callback!(|listener: Uid| PnetEchoServerAction::InitListenerSuccess { listener }),
                    on_error: callback!(|(listener: Uid, error: String)| PnetEchoServerAction::InitListenerError { listener, error }),
                    on_new_connection: callback!(|(listener: Uid, connection: Uid)| PnetEchoServerAction::ConnectionEvent { listener, connection }),
                    on_new_connection_error: callback!(|(listener: Uid, connection: Uid, error: String)| PnetEchoServerAction::ConnectionErrorEvent { listener, connection, error }),
                    on_connection_closed: callback!(|(listener: Uid, connection: Uid)| PnetEchoServerAction::CloseEvent { listener, connection }),
                    on_listener_closed: callback!(|listener: Uid| PnetEchoServerAction::ListenerCloseEvent { listener }),
                });
            }
            PnetEchoServerAction::InitError { error, .. } => {
                panic!("Server initialization failed: {}", error)
            }
            PnetEchoServerAction::InitListenerSuccess { .. } => {
                state.substate_mut::<PnetEchoServerState>().status = EchoServerStatus::Listening {
                    connections: Objects::<Connection>::new(),
                }
            }
            PnetEchoServerAction::InitListenerError { listener, error } => {
                panic!("Listener {:?} initialization failed: {}", listener, error)
            }
            PnetEchoServerAction::ConnectionEvent { connection, .. } => {
                state
                    .substate_mut::<PnetEchoServerState>()
                    .new_connection(connection);

                info!("|PNET_ECHO_SERVER| new connection {:?}", connection);
            }
            PnetEchoServerAction::ConnectionErrorEvent {
                connection, error, ..
            } => {
                warn!(
                    "|PNET_ECHO_SERVER| incoming connection {:?} error {}",
                    connection, error
                )
            }
            PnetEchoServerAction::ListenerCloseEvent { .. } => {
                todo!()
            }
            PnetEchoServerAction::CloseEvent { connection, .. } => {
                state
                    .substate_mut::<PnetEchoServerState>()
                    .remove_connection(&connection);

                info!("|PNET_ECHO_SERVER| connection {:?} closed", connection);
            }
            PnetEchoServerAction::PollSuccess { .. } => {
                let server_state: &PnetEchoServerState = state.substate();
                let timeout = Timeout::Millis(server_state.config.recv_timeout);
                let count = 1024;

                for connection in server_state.connections_ready_to_recv() {
                    let uid = state.new_uid();

                    info!(
                        "|PNET_ECHO_SERVER| dispatching recv request {:?} ({} bytes), connection {:?}, timeout {:?}",
                        uid, count, connection, timeout
                    );

                    dispatcher.dispatch(PnetServerAction::Recv {
                        uid,
                        connection,
                        count,
                        timeout: timeout.clone(),
                        on_success: callback!(|(uid: Uid, data: Vec<u8>)| PnetEchoServerAction::RecvSuccess { uid, data }),
                        on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetEchoServerAction::RecvTimeout { uid, partial_data }),
                        on_error: callback!(|(uid: Uid, error: String)| PnetEchoServerAction::RecvError { uid, error }),
                    });

                    *state
                        .substate_mut::<PnetEchoServerState>()
                        .get_connection_mut(&connection) = Connection::Receiving { request: uid };
                }
            }
            PnetEchoServerAction::PollError { uid, error } => {
                panic!("Poll {:?} failed: {}", uid, error)
            }
            PnetEchoServerAction::RecvSuccess { uid, data } => {
                let connection = state
                    .substate::<PnetEchoServerState>()
                    .find_connection_uid_by_recv_uid(uid);

                let request = state.new_uid();

                // send data back to client
                dispatcher.dispatch(PnetServerAction::Send {
                    uid: request,
                    connection,
                    data: data.into(),
                    timeout: Timeout::Millis(100), // TODO: configurable
                    on_success: callback!(|uid: Uid| PnetEchoServerAction::SendSuccess { uid }),
                    on_timeout: callback!(|uid: Uid| PnetEchoServerAction::SendTimeout { uid }),
                    on_error: callback!(|(uid: Uid, error: String)| PnetEchoServerAction::SendError { uid, error }),
                });

                *state
                    .substate_mut::<PnetEchoServerState>()
                    .get_connection_mut(&connection) = Connection::Sending { request };
            }
            PnetEchoServerAction::RecvTimeout { uid, partial_data } => {
                let connection = state
                    .substate::<PnetEchoServerState>()
                    .find_connection_uid_by_recv_uid(uid);

                if partial_data.len() > 0 {
                    let request = state.new_uid();

                    // send partial data back to client
                    dispatcher.dispatch(PnetServerAction::Send {
                            uid: request,
                            connection,
                            data: partial_data.into(),
                            timeout: Timeout::Millis(100), // TODO: configurable
                            on_success: callback!(|uid: Uid| PnetEchoServerAction::SendSuccess { uid }),
                            on_timeout: callback!(|uid: Uid| PnetEchoServerAction::SendTimeout { uid }),
                            on_error: callback!(|(uid: Uid, error: String)| PnetEchoServerAction::SendError { uid, error }),
                        });

                    *state
                        .substate_mut::<PnetEchoServerState>()
                        .get_connection_mut(&connection) = Connection::Sending { request };
                } else {
                    // if we didn't receive anything in the time span close the connection
                    dispatcher.dispatch(PnetServerAction::Close { connection });
                    warn!("|PNET_ECHO_SERVER| recv {:?} timeout", uid)
                }
            }
            PnetEchoServerAction::RecvError { uid, error } => {
                // CloseEvent is dispatched by the PnetServer model and handles the rest
                warn!("|PNET_ECHO_SERVER| recv {:?} error: {:?}", uid, error);
            }
            PnetEchoServerAction::SendSuccess { uid } => {
                let server_state: &mut PnetEchoServerState = state.substate_mut();
                let connection = server_state.find_connection_uid_by_send_uid(uid);

                *server_state.get_connection_mut(&connection) = Connection::Ready;
            }
            PnetEchoServerAction::SendTimeout { uid } => {
                let connection = state
                    .substate_mut::<PnetEchoServerState>()
                    .find_connection_uid_by_send_uid(uid);

                dispatcher.dispatch(PnetServerAction::Close { connection });
                warn!("|PNET_ECHO_SERVER| send {:?} timeout", uid)
            }
            PnetEchoServerAction::SendError { uid, error } => {
                // CloseEvent is dispatched by the PnetServer model and handles the rest
                warn!("|PNET_ECHO_SERVER| send {:?} error: {:?}", uid, error)
            }
        }
    }
}
