use crate::{
    automaton::{
        action::{self, Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::pure::tcp::action::{ConnectionResult, RecvResult, SendResult},
};
use serde_derive::{Deserialize, Serialize};
use std::rc::Rc;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "f15cd869-0966-4ab5-881c-530bc0fe95e6"]
pub enum TcpClientPureAction {
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

impl Action for TcpClientPureAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "830b3ab4-d5c9-44f3-9366-7486bb5b52b2"]
pub enum TcpClientInputAction {
    ConnectResult {
        connection: Uid,
        result: ConnectionResult,
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

impl Action for TcpClientInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
