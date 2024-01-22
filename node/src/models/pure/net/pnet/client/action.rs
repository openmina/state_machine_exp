use crate::{
    automaton::{
        action::{Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectionResult, RecvResult, SendResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "1a161896-de5f-46b2-8774-e60e8a34ef9f"]
pub enum PnetClientPureAction {
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_close_connection: ResultDispatch,
        on_result: ResultDispatch,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: ResultDispatch,
    },
    Close {
        connection: Uid,
    },
    Send {
        uid: Uid,
        connection: Uid,
        data: Vec<u8>,
        timeout: Timeout,
        on_result: ResultDispatch,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize, // number of bytes to read
        timeout: Timeout,
        on_result: ResultDispatch,
    },
}

impl Action for PnetClientPureAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "f315283b-258d-4b62-8d3a-ecfd2d0f3c9f"]
pub enum PnetClientInputAction {
    ConnectResult {
        connection: Uid,
        result: ConnectionResult,
    },
    SendNonceResult {
        uid: Uid,
        result: SendResult,
    },
    RecvNonceResult {
        uid: Uid,
        result: RecvResult,
    },
    Closed {
        connection: Uid,
    },
    RecvResult {
        uid: Uid,
        result: RecvResult,
    },
}

impl Action for PnetClientInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
