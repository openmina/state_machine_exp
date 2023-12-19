use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::tcp::{action::RecvResult, state::SendResult},
};

#[derive(Debug)]
pub enum EchoServerPureAction {
    Tick,
}

impl Action for EchoServerPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum EchoServerInputAction {
    Init {
        uid: Uid,
        result: Result<(), String>,
    },
    InitCompleted {
        uid: Uid,
        result: Result<(), String>,
    },
    NewConnection {
        connection_uid: Uid,
    },
    Closed {
        connection_uid: Uid,
    },
    Poll {
        uid: Uid,
        result: Result<(), String>,
    },
    Recv {
        uid: Uid,
        result: RecvResult,
    },
    Send {
        uid: Uid,
        result: SendResult,
    },
}

impl Action for EchoServerInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
