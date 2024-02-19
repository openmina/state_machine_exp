use super::{
    action::PnetClientAction,
    state::{Connection, PnetClientState},
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
            tcp_client::{
                action::TcpClientAction,
                state::{RecvRequest, TcpClientState},
            },
        },
        prng::state::PRNGState,
    },
};
use rand::Rng;
use salsa20::cipher::StreamCipher;

impl RegisterModel for PnetClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PRNGState>() // FIXME: replace with effectful
            .register::<TcpClientState>()
            .model_pure::<Self>()
    }
}

impl PureModel for PnetClientState {
    type Action = PnetClientAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            PnetClientAction::Poll {
                uid,
                timeout,
                on_success,
                on_error,
            } => dispatcher.dispatch(TcpClientAction::Poll {
                uid,
                timeout,
                on_success,
                on_error,
            }),
            PnetClientAction::Connect {
                connection,
                address,
                timeout,
                on_success,
                on_timeout,
                on_error,
                on_close,
            } => {
                state
                    .substate_mut::<PnetClientState>()
                    .new_connection(connection, on_success, on_timeout, on_error, on_close);

                dispatcher.dispatch(TcpClientAction::Connect {
                    connection,
                    address,
                    timeout,
                    on_success: callback!(|connection: Uid| PnetClientAction::ConnectSuccess { connection }),
                    on_timeout: callback!(|connection: Uid| PnetClientAction::ConnectTimeout { connection }),
                    on_error: callback!(|(connection: Uid, error: String)| PnetClientAction::ConnectError { connection, error }),
                    on_close: callback!(|connection: Uid| PnetClientAction::CloseEvent { connection }),
                })
            }
            PnetClientAction::ConnectSuccess { connection } => {
                let uid = state.new_uid();
                // Generate and send a random nonce
                // TODO: use safe (effectful) prng
                let prng: &mut PRNGState = state.substate_mut();
                let nonce = prng.rng.gen::<[u8; 24]>();

                send_nonce(state.substate_mut(), connection, uid, nonce, dispatcher)
            }
            PnetClientAction::ConnectTimeout { connection } => {
                let client_state: &mut PnetClientState = state.substate_mut();
                let Connection { on_timeout, .. } = client_state.get_connection(&connection);

                dispatcher.dispatch_back(on_timeout, connection);
                client_state.remove_connection(&connection);
            }
            PnetClientAction::ConnectError { connection, error } => {
                let client_state: &mut PnetClientState = state.substate_mut();
                let Connection { on_error, .. } = client_state.get_connection(&connection);

                dispatcher.dispatch_back(on_error, (connection, error));
                client_state.remove_connection(&connection);
            }
            // dispatched from send_nonce()
            PnetClientAction::SendNonceSuccess { uid: send_request } => {
                let uid = state.new_uid();

                recv_nonce(state.substate_mut(), uid, send_request, dispatcher)
            }
            PnetClientAction::SendNonceTimeout { uid } => {
                let (&connection, _) = state
                    .substate::<PnetClientState>()
                    .find_connection_by_nonce_request(&uid);

                // Rest of logic handled by `PnetClientInputAction::CloseEvent`
                dispatcher.dispatch(TcpClientAction::Close { connection });
            }
            PnetClientAction::SendNonceError { .. } => {
                // at this point the connection is closed by TcpClient model
                // and we get notified with `PnetClientInputAction::CloseEvent`
            }
            PnetClientAction::RecvNonceSuccess { uid, nonce } => {
                complete_handshake(state.substate_mut(), uid, nonce, dispatcher)
            }
            PnetClientAction::RecvNonceTimeout { uid, .. } => {
                let (&connection, _) = state
                    .substate::<PnetClientState>()
                    .find_connection_by_nonce_request(&uid);

                // Rest of logic handled by `PnetClientInputAction::CloseEvent`
                dispatcher.dispatch(TcpClientAction::Close { connection });
            }
            PnetClientAction::RecvNonceError { .. } => {
                // Same handling as described for the SendNonceError case
            }
            PnetClientAction::Close { connection } => {
                dispatcher.dispatch(TcpClientAction::Close { connection })
            }
            PnetClientAction::CloseEvent { connection } => {
                let client_state: &mut PnetClientState = state.substate_mut();
                let Connection {
                    state,
                    on_error,
                    on_close,
                    ..
                } = client_state.get_connection(&connection);

                match state {
                    ConnectionState::Init => unreachable!(),
                    ConnectionState::NonceSent { .. } | ConnectionState::NonceWait { .. } => {
                        dispatcher.dispatch_back(
                            &on_error,
                            (connection, "error during handshake".to_string()),
                        )
                    }
                    // dispatch to caller's on_close handler only after the handshake phase
                    ConnectionState::Ready { .. } => {
                        dispatcher.dispatch_back(&on_close, connection)
                    }
                }

                client_state.remove_connection(&connection);
            }
            PnetClientAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_success,
                on_timeout,
                on_error,
            } => {
                if let ConnectionState::Ready { send_cipher, .. } = &mut state
                    .substate_mut::<PnetClientState>()
                    .get_connection_mut(&connection)
                    .state
                {
                    let mut data = data.clone();

                    send_cipher.apply_keystream(&mut data);
                    dispatcher.dispatch(TcpClientAction::Send {
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
            PnetClientAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_success,
                on_timeout,
                on_error,
            } => {
                state
                    .substate_mut::<PnetClientState>()
                    .new_recv_request(&uid, connection, on_success, on_timeout, on_error);

                dispatcher.dispatch(TcpClientAction::Recv {
                    uid,
                    connection,
                    count,
                    timeout,
                    on_success: callback!(|(uid: Uid, data: Vec<u8>)| PnetClientAction::RecvSuccess { uid, data }),
                    on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetClientAction::RecvTimeout { uid, partial_data }),
                    on_error: callback!(|(uid: Uid, error: String)| PnetClientAction::RecvError { uid, error }),
                })
            }
            PnetClientAction::RecvSuccess { uid, data } => {
                let client_state: &mut PnetClientState = state.substate_mut();
                let RecvRequest {
                    connection,
                    on_success,
                    ..
                } = client_state.take_recv_request(&uid);

                dispatcher
                    .dispatch_back(&on_success, (uid, decrypt(client_state, connection, &data)))
            }
            PnetClientAction::RecvTimeout { uid, partial_data } => {
                let client_state: &mut PnetClientState = state.substate_mut();
                let RecvRequest {
                    connection,
                    on_timeout,
                    ..
                } = client_state.take_recv_request(&uid);

                dispatcher.dispatch_back(
                    &on_timeout,
                    (uid, decrypt(client_state, connection, &partial_data)),
                )
            }
            PnetClientAction::RecvError { uid, error } => {
                let RecvRequest { on_error, .. } = state
                    .substate_mut::<PnetClientState>()
                    .take_recv_request(&uid);

                dispatcher.dispatch_back(&on_error, (uid, error))
            }
        }
    }
}

fn send_nonce(
    client_state: &mut PnetClientState,
    connection: Uid,
    uid: Uid,
    nonce: [u8; 24],
    dispatcher: &mut Dispatcher,
) {
    let timeout = client_state.config.send_nonce_timeout.clone();
    let Connection { state, .. } = client_state.get_connection_mut(&connection);

    if let ConnectionState::Init = state {
        dispatcher.dispatch(TcpClientAction::Send {
            uid,
            connection,
            data: nonce.into(),
            timeout,
            on_success: callback!(|uid: Uid| PnetClientAction::SendNonceSuccess { uid }),
            on_timeout: callback!(|uid: Uid| PnetClientAction::SendNonceTimeout { uid }),
            on_error: callback!(|(uid: Uid, error: String)| PnetClientAction::SendNonceError { uid, error }),
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
    client_state: &mut PnetClientState,
    uid: Uid,
    send_request: Uid,
    dispatcher: &mut Dispatcher,
) {
    let timeout = client_state.config.recv_nonce_timeout.clone();
    let (connection, Connection { state, .. }) =
        client_state.find_connection_mut_by_nonce_request(&send_request);
    let connection = *connection;

    if let ConnectionState::NonceSent { nonce, .. } = state {
        dispatcher.dispatch(TcpClientAction::Recv {
            uid,
            connection,
            count: 24,
            timeout,
            on_success: callback!(|(uid: Uid, nonce: Vec<u8>)| PnetClientAction::RecvNonceSuccess { uid, nonce }),
            on_timeout: callback!(|(uid: Uid, partial_data: Vec<u8>)| PnetClientAction::RecvNonceTimeout { uid, partial_data }),
            on_error: callback!(|(uid: Uid, error: String)| PnetClientAction::RecvNonceError { uid, error }),
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
    client_state: &mut PnetClientState,
    uid: Uid,
    nonce: Vec<u8>,
    dispatcher: &mut Dispatcher,
) {
    let shared_secret = client_state.config.pnet_key.0.clone();
    let (
        connection,
        Connection {
            state, on_success, ..
        },
    ) = client_state.find_connection_mut_by_nonce_request(&uid);
    let connection = *connection;

    if let ConnectionState::NonceWait { nonce_sent, .. } = state {
        let send_cipher = XSalsa20Wrapper::new(&shared_secret, &nonce_sent);
        let recv_cipher = XSalsa20Wrapper::new(&shared_secret, nonce[..24].try_into().unwrap());

        *state = ConnectionState::Ready {
            send_cipher,
            recv_cipher,
        };
        dispatcher.dispatch_back(&on_success, connection);
    } else {
        unreachable!()
    };
}

fn decrypt(client_state: &mut PnetClientState, connection: Uid, data: &Vec<u8>) -> Vec<u8> {
    if let ConnectionState::Ready { recv_cipher, .. } =
        &mut client_state.get_connection_mut(&connection).state
    {
        let mut data = data.clone();

        recv_cipher.apply_keystream(&mut data);
        data
    } else {
        unreachable!()
    }
}
