use crate::{
    automaton::{
        action::{Action, ActionKind},
        state::Uid,
    },
    models::pure::net::tcp::action::TcpPollEvents,
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "0a64ed4a-df98-47aa-b847-97f0e405c686"]
pub enum PnetEchoClientAction {
    Tick,
    PollSuccess { uid: Uid, events: TcpPollEvents },
    PollError { uid: Uid, error: String },
    InitSuccess { instance: Uid },
    InitError { instance: Uid, error: String },
    ConnectSuccess { connection: Uid },
    ConnectTimeout { connection: Uid },
    ConnectError { connection: Uid, error: String },
    CloseEvent { connection: Uid },
    SendSuccess { uid: Uid },
    SendTimeout { uid: Uid },
    SendError { uid: Uid, error: String },
    RecvSuccess { uid: Uid, data: Vec<u8> },
    RecvTimeout { uid: Uid, partial_data: Vec<u8> },
    RecvError { uid: Uid, error: String },
}

impl Action for PnetEchoClientAction {
    const KIND: ActionKind = ActionKind::Pure;
}
