use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "57c25007-3c1d-4871-8faf-6d1576c94ec4"]
pub struct EchoClientTickAction();

impl Action for EchoClientTickAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "6f8ab34b-2f20-49ff-a4a5-3573ff86fc61"]
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
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
