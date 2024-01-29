use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::net::tcp::action::{RecvResult, SendResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "4430484f-487b-4c03-9964-faf00bbab2fe"]
pub struct PnetEchoServerTickAction();

impl Action for PnetEchoServerTickAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "ab8ace39-22cd-4717-a446-e20442f7f0f1"]
pub enum PnetEchoServerInputAction {
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

impl Action for PnetEchoServerInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
