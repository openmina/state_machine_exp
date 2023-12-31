use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::tcp::action::{RecvResult, SendResult},
};

#[derive(Debug)]
pub struct EchoServerTickAction();

impl Action for EchoServerTickAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum EchoServerInputAction {
    InitResult {
        instance: Uid,
        result: Result<(), String>,
    },
    NewServerResult {
        server: Uid,
        result: Result<(), String>,
    },
    NewConnection {
        connection: Uid,
    },
    Closed {
        connection: Uid,
    },
    PollResult {
        uid: Uid,
        result: Result<(), String>,
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

impl Action for EchoServerInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
