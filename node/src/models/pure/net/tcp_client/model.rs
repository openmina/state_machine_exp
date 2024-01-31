use super::{
    action::TcpClientAction,
    state::{RecvRequest, SendRequest, TcpClientState},
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
        net::tcp::{
            action::{ConnectionResult, RecvResult, SendResult, TcpAction},
            state::TcpState,
        },
        net::tcp_client::state::Connection,
    },
};

// The `TcpClientState` model is an abstraction layer over the `TcpState` model
// providing a simpler interface for working with TCP client operations.

// This model depends on the `TcpState` model.
impl RegisterModel for TcpClientState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.register::<TcpState>().model_pure::<Self>()
    }
}

impl PureModel for TcpClientState {
    type Action = TcpClientAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        match action {
            TcpClientAction::Poll {
                uid,
                timeout,
                on_result,
            } => dispatcher.dispatch(TcpAction::Poll {
                uid,
                objects: Vec::new(),
                timeout,
                on_result,
            }),
            TcpClientAction::Connect {
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

                dispatcher.dispatch(TcpAction::Connect {
                    connection,
                    address,
                    timeout,
                    on_result: callback!(|(connection: Uid, result: ConnectionResult)| {
                        TcpClientAction::ConnectResult { connection, result }
                    }),
                });
            }
            TcpClientAction::ConnectResult { connection, result } => {
                let Connection { on_result, .. } = state
                    .substate_mut::<TcpClientState>()
                    .get_connection(&connection);

                dispatcher.dispatch_back(on_result, (connection, result));
            }
            TcpClientAction::Close { connection } => dispatcher.dispatch(TcpAction::Close {
                connection,
                on_result: callback!(|connection: Uid| {
                    TcpClientAction::CloseResult {
                        connection,
                        notify: true,
                    }
                }),
            }),
            TcpClientAction::CloseResult { connection, notify } => {
                let client_state: &mut TcpClientState = state.substate_mut();
                let Connection {
                    on_close_connection,
                    ..
                } = client_state.get_connection(&connection);

                if notify {
                    dispatcher.dispatch_back(&on_close_connection, connection);
                }

                client_state.remove_connection(&connection);
            }
            TcpClientAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<TcpClientState>()
                    .new_send_request(&uid, connection, on_result);

                dispatcher.dispatch(TcpAction::Send {
                    uid,
                    connection,
                    data,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: SendResult)| {
                        TcpClientAction::SendResult { uid, result }
                    }),
                });
            }
            TcpClientAction::SendResult { uid, result } => {
                let SendRequest {
                    connection,
                    on_result,
                } = state
                    .substate_mut::<TcpClientState>()
                    .take_send_request(&uid);

                if let SendResult::Error(_) = result {
                    dispatcher.dispatch(TcpAction::Close {
                        connection,
                        on_result: callback!(|connection: Uid| {
                            TcpClientAction::CloseResult {
                                connection,
                                notify: true,
                            }
                        }),
                    });
                }

                dispatcher.dispatch_back(&on_result, (uid, result))
            }
            TcpClientAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result,
            } => {
                state
                    .substate_mut::<TcpClientState>()
                    .new_recv_request(&uid, connection, on_result);

                dispatcher.dispatch(TcpAction::Recv {
                    uid,
                    connection,
                    count,
                    timeout,
                    on_result: callback!(|(uid: Uid, result: RecvResult)| {
                        TcpClientAction::RecvResult { uid, result }
                    }),
                });
            }
            TcpClientAction::RecvResult { uid, result } => {
                let RecvRequest {
                    connection,
                    on_result,
                } = state
                    .substate_mut::<TcpClientState>()
                    .take_recv_request(&uid);

                if let RecvResult::Error(_) = result {
                    dispatcher.dispatch(TcpAction::Close {
                        connection,
                        on_result: callback!(|connection: Uid| {
                            TcpClientAction::CloseResult {
                                connection,
                                notify: true,
                            }
                        }),
                    });
                }

                dispatcher.dispatch_back(&on_result, (uid, result))
            }
        }
    }
}
