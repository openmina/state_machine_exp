use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::tcp::action::{RecvResult, SendResult},
};
use serde::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "7c21da23-66af-423a-ad67-ad5d02631251"]
pub struct EchoServerTickAction();

//#[typetag::serde]
impl Action for EchoServerTickAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "04f45d4b-7484-4fe5-a6b2-651ef7e58ca9"]
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

//#[typetag::serde]
impl Action for EchoServerInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
