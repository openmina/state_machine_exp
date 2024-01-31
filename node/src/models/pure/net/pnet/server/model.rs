use super::{
    action::PnetServerAction,
    state::{Connection, PnetServerState, Server},
};
use crate::{
    automaton::{
        action::{Dispatcher, OrError, Redispatch, Timeout},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::{
            pnet::common::{ConnectionState, XSalsa20Wrapper},
            tcp::action::{RecvResult, SendResult},
            tcp_server::{action::TcpServerAction, state::TcpServerState},
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
                on_result,
            } => dispatcher.dispatch(TcpServerAction::Poll {
                uid,
                timeout,
                on_result,
            }),
            PnetServerAction::New {
                address,
                server,
                max_connections,
                on_new_connection,
                on_close_connection,
                on_result,
            } => {
                state.substate_mut::<PnetServerState>().new_server(
                    server,
                    on_new_connection,
                    on_close_connection,
                    on_result,
                );

                dispatcher.dispatch(TcpServerAction::New {
                    address,
                    server,
                    max_connections,
                    on_new_connection: callback!(|(server: Uid, connection: Uid)| {
                        PnetServerAction::NewConnectionEvent { server, connection }
                    }),
                    on_close_connection: callback!(|(_server: Uid, connection: Uid)| {
                        PnetServerAction::CloseEvent { connection }
                    }),
                    on_result: callback!(|(server: Uid, result: OrError<()>)| {
                        PnetServerAction::NewResult { server, result }
                    }),
                });
            }
            PnetServerAction::NewResult { server, result } => {
                let server_state: &mut PnetServerState = state.substate_mut();
                let Server { on_result, .. } = server_state.get_server(&server);

                dispatcher.dispatch_back(on_result, (server, result.clone()));

                if result.is_err() {
                    server_state.remove_server(&server)
                }
            }
            PnetServerAction::NewConnectionEvent { server, connection } => {
                let server_state: &mut PnetServerState = state.substate_mut();

                server_state.new_connection(server, connection);
                send_nonce(state, server, connection, dispatcher)
            }
            PnetServerAction::SendNonceResult { uid, result } => match result {
                SendResult::Success => recv_nonce(state, uid, dispatcher),
                SendResult::Timeout => handle_handshake_timeout(state, uid, dispatcher),
                SendResult::Error(_) => (),
            },
            PnetServerAction::RecvNonceResult { uid, result } => match result {
                RecvResult::Success(nonce) => complete_connection(state, uid, nonce, dispatcher),
                RecvResult::Timeout(_) => handle_handshake_timeout(state, uid, dispatcher),
                RecvResult::Error(_) => (),
            },
            PnetServerAction::CloseEvent { connection } => {
                let server_state = state.substate_mut::<PnetServerState>();
                handle_connection_closed(server_state, connection, dispatcher)
            }
            PnetServerAction::Close { connection } => {
                dispatcher.dispatch(TcpServerAction::Close { connection })
            }
            PnetServerAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result,
            } => encrypt_and_send(state, uid, connection, data, timeout, on_result, dispatcher),
            PnetServerAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<PnetServerState>()
                    .new_recv_request(&uid, connection, on_result);

                dispatcher.dispatch(TcpServerAction::Recv {
                    uid,
                    connection,
                    count,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: RecvResult)| {
                        PnetServerAction::RecvResult { uid, result }
                    }),
                })
            }
            PnetServerAction::RecvResult { uid, result } => {
                recv_and_decrypt(state, uid, result, dispatcher)
            }
        }
    }
}

fn send_nonce<Substate: ModelState>(
    state: &mut State<Substate>,
    server: Uid,
    connection: Uid,
    dispatcher: &mut Dispatcher,
) {
    let uid = state.new_uid();
    // TODO: use safe (effectful) prng
    let nonce = state.substate_mut::<PRNGState>().rng.gen::<[u8; 24]>();
    let server_state = state.substate_mut::<PnetServerState>();
    let timeout = server_state.config.send_nonce_timeout.clone();
    let conn = server_state.get_connection_mut(&server, &connection);

    assert!(matches!(conn.state, ConnectionState::Init));

    dispatcher.dispatch(TcpServerAction::Send {
        uid,
        connection,
        data: nonce.into(),
        timeout,
        on_result: callback!(|(uid: Uid, result: SendResult)| {
            PnetServerAction::SendNonceResult { uid, result }
        }),
    });

    conn.state = ConnectionState::NonceSent {
        send_request: uid,
        nonce,
    };
}

fn recv_nonce<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    dispatcher: &mut Dispatcher,
) {
    let server_state = state.substate::<PnetServerState>();
    let timeout = server_state.config.recv_nonce_timeout.clone();
    let (&server, &connection, _) = server_state.find_connection_by_nonce_request(&uid);
    let uid = state.new_uid();

    dispatcher.dispatch(TcpServerAction::Recv {
        uid,
        connection,
        count: 24,
        timeout,
        on_result: callback!(|(uid: Uid, result: RecvResult)| {
            PnetServerAction::RecvNonceResult { uid, result }
        }),
    });

    let conn = state
        .substate_mut::<PnetServerState>()
        .get_connection_mut(&server, &connection);

    let ConnectionState::NonceSent { nonce, .. } = conn.state else {
        unreachable!()
    };

    conn.state = ConnectionState::NonceWait {
        recv_request: uid,
        nonce_sent: nonce,
    };
}

fn complete_connection<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    nonce: Vec<u8>,
    dispatcher: &mut Dispatcher,
) {
    let server_state = state.substate_mut::<PnetServerState>();
    let shared_secret = server_state.config.pnet_key.0.clone();
    let (&server, &connection, _) = server_state.find_connection_by_nonce_request(&uid);
    let conn = server_state.get_connection_mut(&server, &connection);

    let ConnectionState::NonceWait { nonce_sent, .. } = conn.state else {
        unreachable!()
    };

    let send_cipher = XSalsa20Wrapper::new(&shared_secret, &nonce_sent);
    let recv_cipher = XSalsa20Wrapper::new(&shared_secret, nonce[..24].try_into().unwrap());

    conn.state = ConnectionState::Ready {
        send_cipher,
        recv_cipher,
    };

    let Server {
        on_new_connection, ..
    } = server_state.get_server(&server);

    dispatcher.dispatch_back(&on_new_connection, (server, connection));
}

fn handle_handshake_timeout<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    dispatcher: &mut Dispatcher,
) {
    let client_state = state.substate_mut::<PnetServerState>();
    let (_, &connection, _) = client_state.find_connection_by_nonce_request(&uid);

    // Rest of logic handled by `PnetServerAction::Closed`
    dispatcher.dispatch(TcpServerAction::Close { connection });
}

fn handle_connection_closed(
    server_state: &mut PnetServerState,
    connection: Uid,
    dispatcher: &mut Dispatcher,
) {
    let &server = server_state.find_server_by_connection(&connection);
    let Connection { state, .. } = server_state.get_connection(&server, &connection);

    match state {
        ConnectionState::Init => unreachable!(),
        ConnectionState::NonceSent { .. } | ConnectionState::NonceWait { .. } => (),
        ConnectionState::Ready { .. } => {
            let Server {
                on_close_connection,
                ..
            } = server_state.get_server(&server);
            dispatcher.dispatch_back(on_close_connection, (server, connection))
        }
    }

    server_state
        .get_server_mut(&server)
        .remove_connection(&connection);
}

fn encrypt_and_send<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    connection: Uid,
    data: Vec<u8>,
    timeout: Timeout,
    on_result: Redispatch<(Uid, SendResult)>,
    dispatcher: &mut Dispatcher,
) {
    let server_state = state.substate_mut::<PnetServerState>();
    let &server = server_state.find_server_by_connection(&connection);
    let conn = server_state.get_connection_mut(&server, &connection);
    let ConnectionState::Ready { send_cipher, .. } = &mut conn.state else {
        unreachable!()
    };
    let mut data = data.clone();
    send_cipher.apply_keystream(&mut data);
    dispatcher.dispatch(TcpServerAction::Send {
        uid,
        connection,
        data: data.into(),
        timeout,
        on_result,
    })
}

fn recv_and_decrypt<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    result: RecvResult,
    dispatcher: &mut Dispatcher,
) {
    let server_state = state.substate_mut::<PnetServerState>();
    let request = server_state.take_recv_request(&uid);
    let &server = server_state.find_server_by_connection(&request.connection);
    let conn = server_state.get_connection_mut(&server, &request.connection);

    let ConnectionState::Ready { recv_cipher, .. } = &mut conn.state else {
        unreachable!()
    };

    let result = match result {
        RecvResult::Success(data) => {
            let mut data = data.clone();
            recv_cipher.apply_keystream(&mut data);
            RecvResult::Success(data)
        }
        RecvResult::Timeout(data) => {
            let mut data = data.clone();
            recv_cipher.apply_keystream(&mut data);
            RecvResult::Timeout(data)
        }
        _ => result,
    };

    dispatcher.dispatch_back(&request.on_result, (uid, result))
}