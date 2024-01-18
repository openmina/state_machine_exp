use super::{
    action::{TcpClientInputAction, TcpClientPureAction},
    state::{RecvRequest, SendRequest, TcpClientState},
};
use crate::{
    automaton::{
        action::Dispatcher,
        model::{InputModel, PureModel},
        runner::{RegisterModel, RunnerBuilder},
        state::{ModelState, State, Uid},
    },
    callback,
    models::pure::{
        tcp::{
            action::{ConnectionResult, RecvResult, SendResult, TcpPureAction},
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

enum Act {
    In(TcpClientInputAction),
    Pure(TcpClientPureAction),
}

fn process_action<Substate: ModelState>(
    state: &mut State<Substate>,
    action: Act,
    dispatcher: &mut Dispatcher,
) {
    match action {
        Act::Pure(TcpClientPureAction::Poll {
            uid,
            timeout,
            on_result,
        }) => dispatcher.dispatch(TcpPureAction::Poll {
            uid,
            objects: Vec::new(),
            timeout,
            on_result,
        }),
        Act::Pure(TcpClientPureAction::Connect {
            connection,
            address,
            timeout,
            on_close_connection,
            on_result,
        }) => {
            state.substate_mut::<TcpClientState>().new_connection(
                connection,
                on_close_connection,
                on_result,
            );

            dispatcher.dispatch(TcpPureAction::Connect {
                connection,
                address,
                timeout,
                on_result: callback!(|(connection: Uid, result: ConnectionResult)| {
                    TcpClientInputAction::ConnectResult { connection, result }
                }),
            });
        }
        Act::In(TcpClientInputAction::ConnectResult { connection, result }) => {
            let Connection { on_result, .. } = state
                .substate_mut::<TcpClientState>()
                .get_connection(&connection);

            dispatcher.dispatch_back(on_result, (connection, result));
        }
        Act::Pure(TcpClientPureAction::Close { connection }) => {
            dispatcher.dispatch(TcpPureAction::Close {
                connection,
                on_result: callback!(|connection: Uid| {
                    TcpClientInputAction::CloseResult { connection }
                }),
            })
        }
        Act::In(TcpClientInputAction::CloseResult { connection }) => {
            let client_state: &mut TcpClientState = state.substate_mut();
            let Connection {
                on_close_connection,
                ..
            } = client_state.get_connection(&connection);

            dispatcher.dispatch_back(&on_close_connection, connection);
            client_state.remove_connection(&connection);
        }
        Act::Pure(TcpClientPureAction::Send {
            uid,
            connection,
            data,
            timeout,
            on_result,
        }) => {
            state
                .substate_mut::<TcpClientState>()
                .new_send_request(&uid, connection, on_result);

            dispatcher.dispatch(TcpPureAction::Send {
                uid,
                connection,
                data,
                timeout,
                on_result: callback!(|(uid: Uid, result: SendResult)| {
                    TcpClientInputAction::SendResult { uid, result }
                }),
            });
        }
        Act::In(TcpClientInputAction::SendResult { uid, result }) => {
            let SendRequest {
                connection,
                on_result,
            } = state
                .substate_mut::<TcpClientState>()
                .take_send_request(&uid);

            if let SendResult::Error(_) = result {
                dispatcher.dispatch(TcpPureAction::Close {
                    connection,
                    on_result: callback!(|connection: Uid| {
                        TcpClientInputAction::CloseResult { connection }
                    }),
                });
            }

            dispatcher.dispatch_back(&on_result, (uid, result))
        }
        Act::Pure(TcpClientPureAction::Recv {
            uid,
            connection,
            count,
            timeout,
            on_result,
        }) => {
            state
                .substate_mut::<TcpClientState>()
                .new_recv_request(&uid, connection, on_result);

            dispatcher.dispatch(TcpPureAction::Recv {
                uid,
                connection,
                count,
                timeout,
                on_result: callback!(|(uid: Uid, result: RecvResult)| {
                    TcpClientInputAction::RecvResult { uid, result }
                }),
            });
        }
        Act::In(TcpClientInputAction::RecvResult { uid, result }) => {
            let RecvRequest {
                connection,
                on_result,
            } = state
                .substate_mut::<TcpClientState>()
                .take_recv_request(&uid);

            if let RecvResult::Error(_) = result {
                dispatcher.dispatch(TcpPureAction::Close {
                    connection,
                    on_result: callback!(|connection: Uid| {
                        TcpClientInputAction::CloseResult { connection }
                    }),
                });
            }

            dispatcher.dispatch_back(&on_result, (uid, result))
        }
    }
}

impl InputModel for TcpClientState {
    type Action = TcpClientInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::In(action), dispatcher)
    }
}

impl PureModel for TcpClientState {
    type Action = TcpClientPureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        process_action(state, Act::Pure(action), dispatcher)
    }
}
