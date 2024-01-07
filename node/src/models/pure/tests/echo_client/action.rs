use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};

#[derive(Debug)]
pub struct EchoClientTickAction();

impl Action for EchoClientTickAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum EchoClientInputAction {
    InitResult {
        instance: Uid,
        result: Result<(), String>,
    },
    ConnectResult {
        connection: Uid,
        result: ConnectionResult,
    },
    Closed {
        connection: Uid,
    },
    PollResult {
        uid: Uid,
        result: TcpPollResult,
    },
    RecvResult {
        uid: Uid,
        result: RecvResult,
    },
    SendResult {
        uid: Uid,
        result: SendResult,
    },
}

impl Action for EchoClientInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
