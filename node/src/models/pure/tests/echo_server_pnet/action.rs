use crate::{
    automaton::{
        action::{Action, ActionKind, OrError},
        state::Uid,
    },
    models::pure::net::tcp::action::{RecvResult, SendResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "ab8ace39-22cd-4717-a446-e20442f7f0f1"]
pub enum PnetEchoServerAction {
    Tick,
    InitResult { instance: Uid, result: OrError<()> },
    NewServerResult { server: Uid, result: OrError<()> },
    NewConnectionEvent { connection: Uid },
    CloseEvent { connection: Uid },
    PollResult { uid: Uid, result: OrError<()> },
    RecvResult { uid: Uid, result: RecvResult },
    SendResult { uid: Uid, result: SendResult },
}

impl Action for PnetEchoServerAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}
