use crate::{
    automaton::{
        action::{self, Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::pure::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use std::rc::Rc;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "9bb1c88e-71c8-4a55-8074-cd3dd939a1fb"]
pub enum TcpServerPureAction {
    New {
        address: String,
        server: Uid,
        max_connections: usize,
        on_new_connection: ResultDispatch, // (server_uid, new_connection_uid)
        on_close_connection: ResultDispatch, // (server_uid, connection_uid)
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
        #[serde(
            serialize_with = "action::serialize_rc_bytes",
            deserialize_with = "action::deserialize_rc_bytes"
        )]
        data: Rc<[u8]>,
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

//#[typetag::serde]
impl Action for TcpServerPureAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "577ab1fe-220a-489b-a8c3-c63b5b7bbf9a"]
pub enum TcpServerInputAction {
    NewResult {
        server: Uid,
        result: Result<(), String>,
    },
    PollResult {
        uid: Uid,
        result: TcpPollResult,
    },
    AcceptResult {
        connection: Uid,
        result: ConnectionResult,
    },
    CloseInternalResult {
        connection: Uid,
    },
    CloseResult {
        connection: Uid,
    },
    SendResult {
        uid: Uid,
        result: SendResult,
    },
    RecvResult {
        uid: Uid,
        result: RecvResult,
    },
}

impl Action for TcpServerInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
