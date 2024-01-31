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
#[uuid = "fd4da055-71d2-4484-9009-93d5a7924a23"]
pub enum SimpleClientAction {
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

impl Action for SimpleClientAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}
