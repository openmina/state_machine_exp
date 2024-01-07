use super::{
    action::{TcpClientInputAction, TcpClientPureAction},
    state::{RecvRequest, SendRequest, TcpClientState},
};
use crate::{
    automaton::{
        action::{Dispatcher, ResultDispatch},
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State},
    },
    dispatch, dispatch_back,
    models::pure::{
        tcp::{
            action::{RecvResult, SendResult, TcpPureAction},
            state::TcpState,
        },
        tcp_client::state::Connection,
    },
};

// The `TcpClientState` model is an abstraction layer over the `TcpState` model
// providing a simpler interface for working with TCP client operations.

// This model depends on the `TcpState` model.
impl RegisterModel for TcpClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<TcpState>()
            .model_pure_and_input::<Self>()
    }
}

impl InputModel for TcpClientState {
    type Action = TcpClientInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpClientInputAction::ConnectResult { connection, result } => {
                let Connection { on_result, .. } = state
                    .substate_mut::<TcpClientState>()
                    .get_connection(&connection);

                dispatch_back!(dispatcher, on_result, (connection, result));
            }
            TcpClientInputAction::CloseResult { connection } => {
                let client_state: &mut TcpClientState = state.substate_mut();
                let Connection {
                    on_close_connection,
                    ..
                } = client_state.get_connection(&connection);

                dispatch_back!(dispatcher, &on_close_connection, connection);
                client_state.remove_connection(&connection);
            }
            TcpClientInputAction::SendResult { uid, result } => {
                let SendRequest {
                    connection,
                    on_result,
                } = state
                    .substate_mut::<TcpClientState>()
                    .take_send_request(&uid);

                if let SendResult::Error(_) = result {
                    dispatch!(
                        dispatcher,
                        TcpPureAction::Close {
                            connection,
                            on_result: ResultDispatch::new(|connection| {
                                TcpClientInputAction::CloseResult { connection }.into()
                            }),
                        }
                    );
                }

                dispatch_back!(dispatcher, &on_result, (uid, result))
            }
            TcpClientInputAction::RecvResult { uid, result } => {
                let RecvRequest {
                    connection,
                    on_result,
                } = state
                    .substate_mut::<TcpClientState>()
                    .take_recv_request(&uid);

                if let RecvResult::Error(_) = result {
                    dispatch!(
                        dispatcher,
                        TcpPureAction::Close {
                            connection,
                            on_result: ResultDispatch::new(|connection| {
                                TcpClientInputAction::CloseResult { connection }.into()
                            }),
                        }
                    );
                }

                dispatch_back!(dispatcher, &on_result, (uid, result))
            }
        }
    }
}

impl PureModel for TcpClientState {
    type Action = TcpClientPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpClientPureAction::Connect {
                connection,
                address,
                timeout,
                on_close_connection,
                on_result,
            } => {
                state.substate_mut::<TcpClientState>().new_connection(
                    connection,
                    on_close_connection,
                    on_result,
                );

                dispatch!(
                    dispatcher,
                    TcpPureAction::Connect {
                        connection,
                        address,
                        timeout,
                        on_result: ResultDispatch::new(|(connection, result)| {
                            TcpClientInputAction::ConnectResult { connection, result }.into()
                        }),
                    }
                );
            }
            TcpClientPureAction::Poll {
                uid,
                timeout,
                on_result,
            } => {
                dispatch!(
                    dispatcher,
                    TcpPureAction::Poll {
                        uid,
                        objects: Vec::new(),
                        timeout,
                        on_result
                    }
                )
            }
            TcpClientPureAction::Close { connection } => {
                dispatch!(
                    dispatcher,
                    TcpPureAction::Close {
                        connection,
                        on_result: ResultDispatch::new(|connection| {
                            TcpClientInputAction::CloseResult { connection }.into()
                        }),
                    }
                )
            }
            TcpClientPureAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<TcpClientState>()
                    .new_send_request(&uid, connection, on_result);

                dispatch!(
                    dispatcher,
                    TcpPureAction::Send {
                        uid,
                        connection,
                        data,
                        timeout,
                        on_result: ResultDispatch::new(|(uid, result)| {
                            TcpClientInputAction::SendResult { uid, result }.into()
                        }),
                    }
                );
            }
            TcpClientPureAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<TcpClientState>()
                    .new_recv_request(&uid, connection, on_result);

                dispatch!(
                    dispatcher,
                    TcpPureAction::Recv {
                        uid,
                        connection,
                        count,
                        timeout,
                        on_result: ResultDispatch::new(|(uid, result)| {
                            TcpClientInputAction::RecvResult { uid, result }.into()
                        }),
                    }
                );
            }
        }
    }
}
