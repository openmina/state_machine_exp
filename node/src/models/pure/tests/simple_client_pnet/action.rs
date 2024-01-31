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
#[uuid = "35ffc5fc-4efd-406e-90dd-d09df6372684"]
pub struct SimpleClientTickAction();

impl Action for SimpleClientTickAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "fd4da055-71d2-4484-9009-93d5a7924a23"]
pub enum SimpleClientInputAction {
    InitResult {
        instance: Uid,
        result: OrError<()>,
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

impl Action for SimpleClientInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
