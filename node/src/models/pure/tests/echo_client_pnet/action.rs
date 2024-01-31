use crate::{
    automaton::{
        action::{Action, ActionKind, OrError},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "0a64ed4a-df98-47aa-b847-97f0e405c686"]
pub enum PnetEchoClientAction {
    Tick,
    InitResult {
        instance: Uid,
        result: OrError<()>,
    },
    ConnectResult {
        connection: Uid,
        result: ConnectResult,
    },
    CloseEvent {
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

impl Action for PnetEchoClientAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}
