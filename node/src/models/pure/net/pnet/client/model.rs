use super::{
    action::PnetClientAction,
    state::{Connection, PnetClientState},
};
use crate::{
    automaton::{
        action::{Action, ActionKind, Dispatcher, Redispatch, Timeout},
        model::PureModel,
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::{
            pnet::common::{ConnectionState, XSalsa20Wrapper},
            tcp::action::{ConnectResult, ConnectionResult, RecvResult, SendResult},
            tcp_client::{action::TcpClientAction, state::TcpClientState},
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
                on_result,
            } => dispatcher.dispatch(TcpClientAction::Poll {
                uid,
                timeout,
                on_result,
            }),
            PnetClientAction::Connect {
                connection,
                address,
                timeout,
                on_close_connection,
                on_result,
            } => {
                state.substate_mut::<PnetClientState>().new_connection(
                    connection,
                    on_close_connection,
                    on_result,
                );

                dispatcher.dispatch(TcpClientAction::Connect {
                    connection,
                    address,
                    timeout,
                    on_close_connection: callback!(|connection: Uid| {
                        PnetClientAction::CloseEvent { connection }
                    }),
                    on_result: callback!(|(connection: Uid, result: ConnectionResult)| {
                        let ConnectionResult::Outgoing(result) = result else { unreachable!() };
                        PnetClientAction::ConnectResult { connection, result }
                    }),
                })
            }
            PnetClientAction::ConnectResult { connection, result } => match result {
                ConnectResult::Success => send_nonce(state, connection, dispatcher),
                ConnectResult::Timeout | ConnectResult::Error(_) => {
                    println!("conenct result {:?}", result);
                    handle_connect_error(state, connection, result, dispatcher)
                }
            },
            PnetClientAction::SendNonceResult { uid, result } => match result {
                SendResult::Success => recv_nonce(state, uid, dispatcher),
                SendResult::Timeout => handle_handshake_timeout(state, uid, dispatcher),
                // at this point the connection is closed by TcpClient model
                // and we get notified with `PnetClientInputAction::Closed`,
                // so we handle this case in `handle_connection_closed()`
                SendResult::Error(_) => (),
            },
            PnetClientAction::RecvNonceResult { uid, result } => match result {
                RecvResult::Success(nonce) => complete_connection(state, uid, nonce, dispatcher),
                RecvResult::Timeout(_) => handle_handshake_timeout(state, uid, dispatcher),
                RecvResult::Error(_) => (),
            },
            PnetClientAction::CloseEvent { connection } => {
                let client_state = state.substate_mut::<PnetClientState>();
                handle_connection_closed(client_state, connection, dispatcher)
            }
            PnetClientAction::Close { connection } => {
                dispatcher.dispatch(TcpClientAction::Close { connection })
            }
            PnetClientAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result,
            } => encrypt_and_send(state, uid, connection, data, timeout, on_result, dispatcher),
            PnetClientAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<PnetClientState>()
                    .new_recv_request(&uid, connection, on_result);

                dispatcher.dispatch(TcpClientAction::Recv {
                    uid,
                    connection,
                    count,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: RecvResult)| {
                        PnetClientAction::RecvResult { uid, result }
                    }),
                })
            }
            PnetClientAction::RecvResult { uid, result } => {
                recv_and_decrypt(state, uid, result, dispatcher)
            }
        }
    }
}

fn send_nonce<Substate: ModelState>(
    state: &mut State<Substate>,
    connection: Uid,
    dispatcher: &mut Dispatcher,
) {
    let uid = state.new_uid();
    // TODO: use safe (effectful) prng
    let nonce = state.substate_mut::<PRNGState>().rng.gen::<[u8; 24]>();
    let client_state = state.substate_mut::<PnetClientState>();
    let timeout = client_state.config.send_nonce_timeout.clone();
    let conn = client_state.get_connection_mut(&connection);

    assert!(matches!(conn.state, ConnectionState::Init));

    dispatcher.dispatch(TcpClientAction::Send {
        uid,
        connection,
        data: nonce.into(),
        timeout,
        on_result: callback!(|(uid: Uid, result: SendResult)| {
            PnetClientAction::SendNonceResult { uid, result }
        }),
    });

    conn.state = ConnectionState::NonceSent {
        send_request: uid,
        nonce,
    };
}

fn handle_connect_error<Substate: ModelState>(
    state: &mut State<Substate>,
    connection: Uid,
    result: ConnectResult,
    dispatcher: &mut Dispatcher,
) {
    let client_state = state.substate_mut::<PnetClientState>();
    let Connection { on_result, .. } = client_state.get_connection(&connection);
    println!("handle_connect_error {:?}", connection);

    dispatcher.dispatch_back(on_result, (connection, result.clone()));
    client_state.remove_connection(&connection);
}

fn recv_nonce<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    dispatcher: &mut Dispatcher,
) {
    let client_state = state.substate::<PnetClientState>();
    let timeout = client_state.config.recv_nonce_timeout.clone();
    let (&connection, _) = client_state.find_connection_by_nonce_request(&uid);

    let uid = state.new_uid();

    dispatcher.dispatch(TcpClientAction::Recv {
        uid,
        connection,
        count: 24,
        timeout,
        on_result: callback!(|(uid: Uid, result: RecvResult)| {
            PnetClientAction::RecvNonceResult { uid, result }
        }),
    });

    let conn = state
        .substate_mut::<PnetClientState>()
        .get_connection_mut(&connection);

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
    let client_state = state.substate_mut::<PnetClientState>();
    let shared_secret = client_state.config.pnet_key.0.clone();
    let (&connection, _) = client_state.find_connection_by_nonce_request(&uid);
    let conn = client_state.get_connection_mut(&connection);

    let ConnectionState::NonceWait { nonce_sent, .. } = conn.state else {
        unreachable!()
    };

    let send_cipher = XSalsa20Wrapper::new(&shared_secret, &nonce_sent);
    let recv_cipher = XSalsa20Wrapper::new(&shared_secret, nonce[..24].try_into().unwrap());

    conn.state = ConnectionState::Ready {
        send_cipher,
        recv_cipher,
    };

    dispatcher.dispatch_back(&conn.on_result, (connection, ConnectResult::Success));
}

fn handle_handshake_timeout<Substate: ModelState>(
    state: &mut State<Substate>,
    uid: Uid,
    dispatcher: &mut Dispatcher,
) {
    println!("handle_handshake_timeout {:?}", uid);
    let client_state = state.substate_mut::<PnetClientState>();
    let (&connection, Connection { on_result, .. }) =
        client_state.find_connection_by_nonce_request(&uid);

    dispatcher.dispatch_back(on_result, (connection, ConnectResult::Timeout));
    // Rest of logic handled by `PnetClientInputAction::Closed`
    dispatcher.dispatch(TcpClientAction::Close { connection });
}

fn handle_connection_closed(
    client_state: &mut PnetClientState,
    connection: Uid,
    dispatcher: &mut Dispatcher,
) {
    println!("handle_connection_closed {:?}", connection);
    let conn = client_state.get_connection(&connection);

    match conn.state {
        ConnectionState::Init => unreachable!(),
        ConnectionState::NonceSent { .. } => {
            dispatcher.dispatch_back(
                &conn.on_result,
                (
                    connection,
                    ConnectResult::Error("error sending nonce".to_string()),
                ),
            );
        }
        ConnectionState::NonceWait { .. } => {
            dispatcher.dispatch_back(
                &conn.on_result,
                (
                    connection,
                    ConnectResult::Error("error receiving nonce".to_string()),
                ),
            );
        }
        ConnectionState::Ready { .. } => {
            dispatcher.dispatch_back(&conn.on_close_connection, connection)
        }
    }

    client_state.remove_connection(&connection);
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
    let conn = state
        .substate_mut::<PnetClientState>()
        .get_connection_mut(&connection);

    let ConnectionState::Ready { send_cipher, .. } = &mut conn.state else {
        unreachable!()
    };
    let mut data = data.clone();
    send_cipher.apply_keystream(&mut data);
    //
    dispatcher.dispatch(TcpClientAction::Send {
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
    let client_state = state.substate_mut::<PnetClientState>();
    let request = client_state.take_recv_request(&uid);
    let conn = client_state.get_connection_mut(&request.connection);

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
