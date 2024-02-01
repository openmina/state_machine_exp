use crate::{
    automaton::{
        action::{Action, ActionKind, OrError},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "6f8ab34b-2f20-49ff-a4a5-3573ff86fc61"]
pub enum EchoClientAction {
    Tick,
    InitResult {
        instance: Uid,
        result: OrError<()>,
    },
    ConnectResult {
        connection: Uid,
        result: ConnectionResult,
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

impl Action for EchoClientAction {
    const KIND: ActionKind = ActionKind::Pure;
}
