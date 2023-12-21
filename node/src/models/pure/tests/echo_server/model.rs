use log::info;

use crate::{
    automaton::{
        action::{Dispatcher, ResultDispatch},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    dispatch,
    models::pure::{
        tcp::{
            action::{RecvResult, TcpPureAction},
            state::SendResult,
        },
        tcp_server::action::TcpServerPureAction,
        tests::echo_server::state::ServerStatus,
        time::action::TimePureAction,
    },
};

use super::{
    action::{EchoServerInputAction, EchoServerPureAction},
    state::EchoServerState,
};

fn dispatch_recv_to_connections<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
) {
    let server_state: &EchoServerState = state.models.state();
    assert!(matches!(server_state.status, ServerStatus::Ready));

    for connection_uid in server_state.connections_to_recv() {
        let uid = state.new_uid();

        info!("Dispatch recv {:?} to connection {:?}", uid, connection_uid);

        dispatch!(
            dispatcher,
            TcpServerPureAction::Recv {
                uid,
                connection_uid,
                count: 1024,
                timeout: Some(1000),
                on_result: ResultDispatch::new(|(uid, result)| {
                    (EchoServerInputAction::Recv { uid, result }).into()
                }),
            }
        );

        let connection = state
            .models
            .state_mut::<EchoServerState>()
            .get_connection_mut(&connection_uid);

        connection.recv_uid = Some(uid);
    }
}

fn echo_received_data_to_client(
    server_state: &mut EchoServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: RecvResult,
) {
    assert!(matches!(server_state.status, ServerStatus::Ready));
    let (&connection_uid, connection) = server_state.find_connection_by_recv_uid(uid);

    let fail_reason = match result {
        RecvResult::Success(data) | RecvResult::Timeout(data) => {
            // It is OK to get a timeout as long as it contains partial data (< 1024 bytes)
            if data.len() != 0 {
                dispatch!(
                    dispatcher,
                    TcpServerPureAction::Send {
                        uid,
                        connection_uid,
                        data: data.into(),
                        timeout: Some(1024),
                        on_result: ResultDispatch::new(|(uid, result)| {
                            (EchoServerInputAction::Send { uid, result }).into()
                        }),
                    }
                );

                None
            } else {
                Some("Timeout".to_string())
            }
        }
        RecvResult::Error(err) => Some(err),
    };

    if let Some(reason) = fail_reason {
        info!(
            "Echo server: error receiving data from client {:?}, reason: {:?}",
            connection_uid, reason
        );
        dispatch!(dispatcher, TcpServerPureAction::Close { connection_uid });
    }
}

fn handle_send_result(
    server_state: &mut EchoServerState,
    dispatcher: &mut Dispatcher,
    uid: Uid,
    result: SendResult,
) {
    assert!(matches!(server_state.status, ServerStatus::Ready));
    let (&connection_uid, connection) = server_state.find_connection_by_recv_uid(uid);

    let fail_reason = match result {
        SendResult::Success => None,
        SendResult::Timeout => Some("Timeout".to_string()),
        SendResult::Error(error) => Some(error.to_string()),
    };

    connection.recv_uid = None;

    if let Some(reason) = fail_reason {
        info!(
            "Echo server: error sending data to client {:?}, reason: {:?}",
            connection_uid, reason
        );
        dispatch!(dispatcher, TcpServerPureAction::Close { connection_uid });
    }
}

impl InputModel for EchoServerState {
    type Action = EchoServerInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            EchoServerInputAction::Init { result, .. } => {
                assert!(result.is_ok());
                let EchoServerState { status, .. } = state.models.state_mut();
                *status = ServerStatus::Init;
            }
            EchoServerInputAction::InitCompleted { result, .. } => {
                assert!(result.is_ok());
                let EchoServerState { status, .. } = state.models.state_mut();
                *status = ServerStatus::Ready;
            }
            EchoServerInputAction::NewConnection { connection_uid } => {
                let server_state: &mut EchoServerState = state.models.state_mut();
                assert!(matches!(server_state.status, ServerStatus::Ready));

                info!("New connection {:?}", connection_uid);
                server_state.new_connection(connection_uid)
            }
            EchoServerInputAction::Closed { connection_uid } => {
                let server_state: &mut EchoServerState = state.models.state_mut();
                assert!(matches!(server_state.status, ServerStatus::Ready));

                info!("Connection {:?} closed", connection_uid);
                server_state.remove_connection(&connection_uid);
            }
            EchoServerInputAction::Poll { uid: _, result } => {
                assert!(result.is_ok());
                //info!("Poll result {:?}", result);
                dispatch_recv_to_connections(state, dispatcher)
            }
            EchoServerInputAction::Recv { uid, result } => {
                info!("Recv {:?} result {:?}", uid, result);
                echo_received_data_to_client(state.models.state_mut(), dispatcher, uid, result)
            }
            EchoServerInputAction::Send { uid, result } => {
                info!("Send {:?} result {:?}", uid, result);
                handle_send_result(state.models.state_mut(), dispatcher, uid, result)
            }
        }
    }
}

impl PureModel for EchoServerState {
    type Action = EchoServerPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        assert!(matches!(action, EchoServerPureAction::Tick));

        let EchoServerState { tock, status, .. } = state.models.state_mut();

        if *tock == false {
            // Update time information on each tick
            dispatch!(dispatcher, TimePureAction::Tick);
            *tock = true;
            // Return so the `TimePureAction::Tick` action can be processed.
            // On the next `EchoServerPureAction::Tick` we will have the updated time.
            return;
        } else {
            *tock = false;
        }

        match status {
            // Init TCP model
            ServerStatus::Uninitialized => dispatch!(
                dispatcher,
                TcpPureAction::Init {
                    init_uid: state.new_uid(),
                    on_result: ResultDispatch::new(|(uid, result)| {
                        (EchoServerInputAction::Init { uid, result }).into()
                    }),
                }
            ),
            // Init TCP-server model
            ServerStatus::Init => dispatch!(
                dispatcher,
                TcpServerPureAction::New {
                    uid: state.new_uid(),
                    address: "127.0.0.1:8888".to_string(),
                    max_connections: 2,
                    on_new_connection: ResultDispatch::new(|(_server_uid, connection_uid)| {
                        (EchoServerInputAction::NewConnection { connection_uid }).into()
                    }),
                    on_close_connection: ResultDispatch::new(|(_server_uid, connection_uid)| {
                        (EchoServerInputAction::Closed { connection_uid }).into()
                    }),
                    on_result: ResultDispatch::new(|(uid, result)| {
                        (EchoServerInputAction::InitCompleted { uid, result }).into()
                    }),
                }
            ),
            // Poll events
            ServerStatus::Ready => dispatch!(
                dispatcher,
                TcpServerPureAction::Poll {
                    uid: state.new_uid(),
                    timeout: Some(250),
                    on_result: ResultDispatch::new(|(uid, result)| {
                        (EchoServerInputAction::Poll { uid, result }).into()
                    }),
                }
            ),
        }
    }
}
