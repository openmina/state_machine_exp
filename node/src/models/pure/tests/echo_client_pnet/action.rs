use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "60e5b626-c401-48fe-ba69-32c3b4bf50f3"]
pub struct PnetEchoClientTickAction();

impl Action for PnetEchoClientTickAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "0a64ed4a-df98-47aa-b847-97f0e405c686"]
pub enum PnetEchoClientInputAction {
    InitResult {
        instance: Uid,
        result: Result<(), String>,
    },
    ConnectResult {
        connection: Uid,
        result: ConnectResult,
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

impl Action for PnetEchoClientInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
