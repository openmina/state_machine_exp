use crate::{
    automaton::{
        action::{AnyAction, CompletionRoutine, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State},
    },
    models::pure::{
        tcp::action::TcpPureAction, tcp_server::action::TcpServerPureAction,
        tests::echo_server::state::ServerStatus, time::action::TimePureAction,
    },
};

use super::{
    action::{EchoServerInputAction, EchoServerPureAction},
    state::EchoServerState,
};

impl InputModel for EchoServerState {
    type Action = EchoServerInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        _dispatcher: &mut Dispatcher,
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
            EchoServerInputAction::NewConnection {
                server_uid,
                connection_uid,
            } => {
                todo!()
            }
            EchoServerInputAction::Closed {
                server_uid,
                connection_uid,
            } => todo!(),
            EchoServerInputAction::Poll { uid, result } => {
                assert!(result.is_ok());
                todo!()
                // for each connection:
                //      recv 1024 bytes w/ 1sec timeout
                //      send data back to client 
                //      if 0 bytes received w/ timeout close the connection
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
            dispatcher.dispatch(TimePureAction::Tick);
            *tock = true;
            // Return so the `TimePureAction::Tick` action can be processed.
            // On the next `EchoServerPureAction::Tick` we will have the updated time.
            return;
        } else {
            *tock = false;
        }

        match status {
            // Init TCP model
            ServerStatus::Uninitialized => dispatcher.dispatch(TcpPureAction::Init {
                init_uid: state.new_uid(),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(EchoServerInputAction::Init { uid, result })
                }),
            }),
            // Init TCP-server model
            ServerStatus::Init => dispatcher.dispatch(TcpServerPureAction::New {
                uid: state.new_uid(),
                address: "127.0.0.1:8888".to_string(),
                max_connections: 2,
                on_new_connection: CompletionRoutine::new(|(server_uid, connection_uid)| {
                    AnyAction::from(EchoServerInputAction::NewConnection {
                        server_uid,
                        connection_uid,
                    })
                }),
                on_close_connection: CompletionRoutine::new(|(server_uid, connection_uid)| {
                    AnyAction::from(EchoServerInputAction::Closed {
                        server_uid,
                        connection_uid,
                    })
                }),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(EchoServerInputAction::InitCompleted { uid, result })
                }),
            }),
            // Poll events
            ServerStatus::Ready => dispatcher.dispatch(TcpServerPureAction::Poll {
                uid: state.new_uid(),
                timeout: Some(250),
                on_completion: CompletionRoutine::new(|(uid, result)| {
                    AnyAction::from(EchoServerInputAction::Poll { uid, result })
                }),
            }),
        }
    }
}
