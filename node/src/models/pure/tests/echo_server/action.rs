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
#[uuid = "04f45d4b-7484-4fe5-a6b2-651ef7e58ca9"]
pub enum EchoServerAction {
    Tick,
    InitResult { instance: Uid, result: OrError<()> },
    NewServerResult { server: Uid, result: OrError<()> },
    NewConnection { connection: Uid },
    Closed { connection: Uid },
    PollResult { uid: Uid, result: OrError<()> },
    RecvResult { uid: Uid, result: RecvResult },
    SendResult { uid: Uid, result: SendResult },
}

impl Action for EchoServerAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}
