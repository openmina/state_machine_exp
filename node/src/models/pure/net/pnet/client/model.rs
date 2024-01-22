use super::{
    action::{PnetClientInputAction, PnetClientPureAction},
    state::{Connection, PnetClientState, XSalsa20Wrapper},
};
use crate::{
    automaton::{
        action::{Dispatcher, ResultDispatch, Timeout},
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        net::{
            pnet::client::state::ConnectionState,
            tcp::action::{ConnectResult, ConnectionResult, RecvResult, SendResult},
            tcp_client::{action::TcpClientPureAction, state::TcpClientState},
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
            .model_pure_and_input::<Self>()
    }
}

enum Act {
    In(PnetClientInputAction),
    Pure(PnetClientPureAction),
}

fn process_action<Substate: ModelState>(
    state: &mut State<Substate>,
    action: Act,
    dispatcher: &mut Dispatcher,
) {
    match action {
        Act::Pure(PnetClientPureAction::Poll {
            uid,
            timeout,
            on_result,
        }) => dispatcher.dispatch(TcpClientPureAction::Poll {
            uid,
            timeout,
            on_result,
        }),
        Act::Pure(PnetClientPureAction::Connect {
            connection,
            address,
            timeout,
            on_close_connection,
            on_result,
        }) => {
            state.substate_mut::<PnetClientState>().new_connection(
                connection,
                on_close_connection,
                on_result,
            );

            dispatcher.dispatch(TcpClientPureAction::Connect {
                connection,
                address,
                timeout,
                on_close_connection: callback!(|connection: Uid| {
                    PnetClientInputAction::Closed { connection }
                }),
                on_result: callback!(|(connection: Uid, result: ConnectionResult)| {
                    PnetClientInputAction::ConnectResult { connection, result }
                }),
            })
        }
        Act::In(PnetClientInputAction::ConnectResult { connection, result }) => {
            let ConnectionResult::Outgoing(result) = result else {
                unreachable!()
            };

            match result {
                ConnectResult::Success => send_nonce(state, connection, dispatcher),
                ConnectResult::Timeout | ConnectResult::Error(_) => {
                    handle_connect_error(state, connection, result, dispatcher)
                }
            }
        }
        Act::In(PnetClientInputAction::SendNonceResult { uid, result }) => match result {
            SendResult::Success => recv_nonce(state, dispatcher),
            SendResult::Timeout => handle_handshake_timeout(state, uid, dispatcher),
            // at this point the connection is closed by TcpClient model
            // and we get notified with `PnetClientInputAction::Closed`,
            // so we handle this case in `handle_connection_closed()`
            SendResult::Error(_) => (),
        },
        Act::In(PnetClientInputAction::RecvNonceResult { uid, result }) => match result {
            RecvResult::Success(nonce) => complete_connection(state, uid, nonce, dispatcher),
            RecvResult::Timeout(_) => handle_handshake_timeout(state, uid, dispatcher),
            RecvResult::Error(_) => (),
        },
        Act::In(PnetClientInputAction::Closed { connection }) => {
            let client_state = state.substate_mut::<PnetClientState>();
            handle_connection_closed(client_state, connection, dispatcher)
        }
        Act::Pure(PnetClientPureAction::Close { connection }) => {
            dispatcher.dispatch(TcpClientPureAction::Close { connection })
        }
        Act::Pure(PnetClientPureAction::Send {
            uid,
            connection,
            data,
            timeout,
            on_result,
        }) => encrypt_and_send(state, uid, connection, data, timeout, on_result, dispatcher),
        Act::Pure(PnetClientPureAction::Recv {
            uid,
            connection,
            count,
            timeout,
            on_result,
        }) => {
            state
                .substate_mut::<PnetClientState>()
                .new_recv_request(&uid, connection, on_result);

            dispatcher.dispatch(TcpClientPureAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result: callback!(|(uid: Uid, result: RecvResult)| {
                    PnetClientInputAction::RecvResult { uid, result }
                }),
            })
        }
        Act::In(PnetClientInputAction::RecvResult { uid, result }) => recv_and_decrypt(state, uid, result, dispatcher)
    }
}

impl InputModel for PnetClientState {
    type Action = PnetClientInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::In(action), dispatcher)
    }
}

impl PureModel for PnetClientState {
    type Action = PnetClientPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::Pure(action), dispatcher)
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
    let conn = state
        .substate_mut::<PnetClientState>()
        .get_connection_mut(&connection);

    assert!(matches!(conn.state, ConnectionState::Init));

    dispatcher.dispatch(TcpClientPureAction::Send {
        uid,
        connection,
        data: nonce.into(),
        timeout: Timeout::Millis(200), // TODO: configurable
        on_result: callback!(|(uid: Uid, result: SendResult)| {
            PnetClientInputAction::SendNonceResult { uid, result }
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

    dispatcher.dispatch_back(on_result, (connection, result.clone()));
    client_state.remove_connection(&connection);
}

fn recv_nonce<Substate: ModelState>(state: &mut State<Substate>, dispatcher: &mut Dispatcher) {
    let uid = state.new_uid();
    let (&connection, _) = state
        .substate::<PnetClientState>()
        .find_connection_by_nonce_request(&uid);

    dispatcher.dispatch(TcpClientPureAction::Recv {
        uid,
        connection,
        count: 24,
        timeout: Timeout::Millis(200), // TODO: configurable
        on_result: callback!(|(uid: Uid, result: RecvResult)| {
            PnetClientInputAction::RecvNonceResult { uid, result }
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
    let client_state = state.substate_mut::<PnetClientState>();
    let (&connection, Connection { on_result, .. }) =
        client_state.find_connection_by_nonce_request(&uid);

    dispatcher.dispatch_back(on_result, (connection, ConnectResult::Timeout));
    // Rest of logic handled by `PnetClientInputAction::Closed`
    dispatcher.dispatch(TcpClientPureAction::Close { connection });
}

fn handle_connection_closed(
    client_state: &mut PnetClientState,
    connection: Uid,
    dispatcher: &mut Dispatcher,
) {
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
    on_result: ResultDispatch,
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
    dispatcher.dispatch(TcpClientPureAction::Send {
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

    dispatcher.dispatch_back(&request.on_result, result)
}
