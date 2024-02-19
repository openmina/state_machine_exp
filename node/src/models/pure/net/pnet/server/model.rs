use super::{
    action::PnetServerAction,
    state::{Connection, Listener, PnetServerState},
};
use crate::{
    automaton::{
        action::Dispatcher,
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::{
            pnet::common::{ConnectionState, XSalsa20Wrapper},
            tcp_server::{
                action::TcpServerAction,
                state::{RecvRequest, TcpServerState},
            },
        },
        prng::state::PRNGState,
    },
};
use rand::Rng;
use salsa20::cipher::StreamCipher;

impl RegisterModel for PnetServerState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PRNGState>() // FIXME: replace with effectful
            .register::<TcpServerState>()
            .model_pure::<Self>()
    }
}

impl PureModel for PnetServerState {
    type Action = PnetServerAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetServerAction::Poll {
                uid,
                timeout,
                on_success,
                on_error,
            } => dispatcher.dispatch(TcpServerAction::Poll {
                uid,
                timeout,
                on_success,
                on_error,
            }),
            PnetServerAction::New {
                address,
                listener,
                max_connections,
                on_success,
                on_error,
                on_new_connection,
                on_new_connection_error,
                on_connection_closed,
                on_listener_closed,
            } => {
                state.substate_mut::<PnetServerState>().new_listener(
                    listener,
                    on_success,
                    on_error,
                    on_new_connection,
                    on_new_connection_error,
                    on_connection_closed,
                    on_listener_closed,
                );

                dispatcher.dispatch(TcpServerAction::New {
                    address,
                    listener,
                    max_connections,
                    on_success: callback!(|listener: Uid| PnetServerAction::NewSuccess { listener }),
                    on_error: callback!(|(listener: Uid, error: String)| PnetServerAction::NewError { listener, error }),
                    on_new_connection: callback!(|(listener: Uid, connection: Uid)| PnetServerAction::ConnectionEvent { listener, connection }),
                    on_connection_closed: callback!(|(listener: Uid, connection: Uid)| PnetServerAction::CloseEvent { listener, connection }),
                    on_listener_closed: callback!(|listener: Uid| PnetServerAction::ListenerCloseEvent { listener })
                });
            }
            PnetServerAction::NewSuccess { listener } => {
                let Listener { on_success, .. } =
                    state.substate::<PnetServerState>().get_listener(&listener);

                dispatcher.dispatch_back(on_success, listener);
            }
            PnetServerAction::NewError { listener, error } => {
                let server_state: &mut PnetServerState = state.substate_mut();
                let Listener { on_error, .. } = server_state.get_listener(&listener);

                dispatcher.dispatch_back(on_error, (listener, error));
                server_state.remove_listener(&listener)
            }
            PnetServerAction::ConnectionEvent {
                listener,
                connection,
            } => {
                let uid = state.new_uid();
                // Generate and send a random nonce
                // TODO: use safe (effectful) prng
                let prng: &mut PRNGState = state.substate_mut();
                let nonce = prng.rng.gen::<[u8; 24]>();
                let server_state: &mut PnetServerState = state.substate_mut();

                server_state.new_connection(listener, connection);
                send_nonce(server_state, connection, uid, nonce, dispatcher)
            }
            PnetServerAction::ListenerCloseEvent { .. } => {
                todo!()
            }
            // dispatched from send_nonce()
            PnetServerAction::SendNonceSuccess { uid: send_request } => {
                let uid = state.new_uid();

                recv_nonce(state.substate_mut(), uid, send_request, dispatcher)
            }
            PnetServerAction::SendNonceTimeout { uid } => {
                let connection = state
                    .substate::<PnetServerState>()
                    .find_connection_uid_by_nonce_request(&uid);

                // The rest is handled by `PnetServerAction::CloseEvent`
                dispatcher.dispatch(TcpServerAction::Close { connection });
            }
            PnetServerAction::SendNonceError { .. } => {
                // The connection is closed by TcpServer model.
                // The rest is handled by `PnetServerAction::CloseEvent`
            }
            PnetServerAction::RecvNonceSuccess { uid, nonce } => {
                complete_handshake(state.substate_mut(), uid, nonce, dispatcher)
            }
            PnetServerAction::RecvNonceTimeout { uid, .. } => {
                let connection = state
                    .substate::<PnetServerState>()
                    .find_connection_uid_by_nonce_request(&uid);

                // Rest of logic handled by `PnetServerAction::CloseEvent`
                dispatcher.dispatch(TcpServerAction::Close { connection });
            }
            PnetServerAction::RecvNonceError { .. } => {
                // Same handling as described for the SendNonceError case
            }
            PnetServerAction::CloseEvent {
                listener,
                connection,
            } => {
                let server_state: &mut PnetServerState = state.substate_mut();
                let Connection { state, .. } = server_state.get_connection(&connection);

                //let listener = *server_state.find_listener_by_connection(&connection);
                let Listener {
                    on_new_connection_error,
                    on_connection_closed,
                    ..
                } = server_state.get_listener(&listener);

                match state {
                    ConnectionState::Init => unreachable!(),
                    ConnectionState::NonceSent { .. } | ConnectionState::NonceWait { .. } => {
                        dispatcher.dispatch_back(
                            &on_new_connection_error,
                            (listener, connection, "error during handshake".to_string()),
                        )
                    }
                    ConnectionState::Ready { .. } => {
                        dispatcher.dispatch_back(&on_connection_closed, (listener, connection))
                    }
                }

                server_state
                    .get_listener_mut(&listener)
                    .remove_connection(&connection);
            }
            PnetServerAction::Close { connection } => {
                dispatcher.dispatch(TcpServerAction::Close { connection })
            }
            PnetServerAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_success,
                on_timeout,
                on_error,
            } => {
                if let ConnectionState::Ready { send_cipher, .. } = &mut state
                    .substate_mut::<PnetServerState>()
                    .get_connection_mut(&connection)
                    .state
                {
                    let mut data = data.clone();

                    send_cipher.apply_keystream(&mut data);
                    dispatcher.dispatch(TcpServerAction::Send {
                        uid,
                        connection,
                        data: data.into(),
                        timeout,
                        on_success,
                        on_timeout,
                        on_error,
                    })
                } else {
                    unreachable!()
                }
            }
            PnetServerAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_success,
                on_timeout,
                on_error,
            } => {
                state
                    .substate_mut::<PnetServerState>()
                    .new_recv_request(&uid, connection, on_success, on_timeout, on_error);

                dispatcher.dispatch(TcpServerAction::Recv {
                    uid,
                    connection,
                    count,
                    timeout,
                    on_success: callback!(|(uid: Uid, data: Vec<u8>)| PnetServerAction::RecvSuccess { uid, data }),
                    on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetServerAction::RecvTimeout { uid, partial_data }),
                    on_error: callback!(|(uid: Uid, error: String)| PnetServerAction::RecvError { uid, error }),
                })
            }
            PnetServerAction::RecvSuccess { uid, data } => {
                let server_state: &mut PnetServerState = state.substate_mut();
                let RecvRequest {
                    connection,
                    on_success,
                    ..
                } = server_state.take_recv_request(&uid);

                dispatcher
                    .dispatch_back(&on_success, (uid, decrypt(server_state, connection, &data)))
            }
            PnetServerAction::RecvTimeout { uid, partial_data } => {
                let server_state: &mut PnetServerState = state.substate_mut();
                let RecvRequest {
                    connection,
                    on_timeout,
                    ..
                } = server_state.take_recv_request(&uid);

                dispatcher.dispatch_back(
                    &on_timeout,
                    (uid, decrypt(server_state, connection, &partial_data)),
                )
            }
            PnetServerAction::RecvError { uid, error } => {
                let RecvRequest { on_error, .. } = state
                    .substate_mut::<PnetServerState>()
                    .take_recv_request(&uid);

                dispatcher.dispatch_back(&on_error, (uid, error))
            }
        }
    }
}

fn send_nonce(
    server_state: &mut PnetServerState,
    connection: Uid,
    uid: Uid,
    nonce: [u8; 24],
    dispatcher: &mut Dispatcher,
) {
    let timeout = server_state.config.send_nonce_timeout.clone();
    let Connection { state, .. } = server_state.get_connection_mut(&connection);

    if let ConnectionState::Init = state {
        dispatcher.dispatch(TcpServerAction::Send {
            uid,
            connection,
            data: nonce.into(),
            timeout,
            on_success: callback!(|uid: Uid| PnetServerAction::SendNonceSuccess { uid }),
            on_timeout: callback!(|uid: Uid| PnetServerAction::SendNonceTimeout { uid }),
            on_error: callback!(|(uid: Uid, error: String)| PnetServerAction::SendNonceError { uid, error }),
        });

        *state = ConnectionState::NonceSent {
            send_request: uid,
            nonce,
        };
    } else {
        unreachable!()
    }
}

fn recv_nonce(
    server_state: &mut PnetServerState,
    uid: Uid,
    send_request: Uid,
    dispatcher: &mut Dispatcher,
) {
    let timeout = server_state.config.recv_nonce_timeout.clone();
    let (connection, Connection { state, .. }) =
        server_state.find_connection_mut_by_nonce_request(&send_request);
    let connection = *connection;

    if let ConnectionState::NonceSent { nonce, .. } = state {
        dispatcher.dispatch(TcpServerAction::Recv {
            uid,
            connection,
            count: 24,
            timeout,
            on_success: callback!(|(uid: Uid, nonce: Vec<u8>)| PnetServerAction::RecvNonceSuccess { uid, nonce }),
            on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetServerAction::RecvNonceTimeout { uid, partial_data }),
            on_error: callback!(|(uid: Uid, error: String)| PnetServerAction::RecvNonceError { uid, error }),
        });

        *state = ConnectionState::NonceWait {
            recv_request: uid,
            nonce_sent: *nonce,
        };
    } else {
        unreachable!()
    };
}

fn complete_handshake(
    server_state: &mut PnetServerState,
    uid: Uid,
    nonce: Vec<u8>,
    dispatcher: &mut Dispatcher,
) {
    let shared_secret = server_state.config.pnet_key.0.clone();
    let (connection, Connection { state, .. }) =
        server_state.find_connection_mut_by_nonce_request(&uid);
    let connection = *connection;

    if let ConnectionState::NonceWait { nonce_sent, .. } = state {
        let send_cipher = XSalsa20Wrapper::new(&shared_secret, &nonce_sent);
        let recv_cipher = XSalsa20Wrapper::new(&shared_secret, nonce[..24].try_into().unwrap());

        *state = ConnectionState::Ready {
            send_cipher,
            recv_cipher,
        };

        let listener = *server_state.find_listener_by_connection(&connection);
        let Listener {
            on_new_connection, ..
        } = server_state.get_listener(&listener);
        dispatcher.dispatch_back(&on_new_connection, (listener, connection));
    } else {
        unreachable!()
    };
}

fn decrypt(server_state: &mut PnetServerState, connection: Uid, data: &Vec<u8>) -> Vec<u8> {
    if let ConnectionState::Ready { recv_cipher, .. } =
        &mut server_state.get_connection_mut(&connection).state
    {
        let mut data = data.clone();

        recv_cipher.apply_keystream(&mut data);
        data
    } else {
        unreachable!()
    }
}
