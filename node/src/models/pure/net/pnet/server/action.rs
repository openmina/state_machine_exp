use crate::{
    automaton::{
        action::{Action, ActionKind, OrError, Redispatch, Timeout},
        state::Uid,
    },
    models::pure::net::tcp::action::{RecvResult, SendResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "7f93cd46-0dd7-4849-a823-c1231ea51f60"]
pub enum PnetServerAction {
    New {
        address: String,
        server: Uid,
        max_connections: usize,
        on_new_connection: Redispatch<(Uid, Uid)>,
        on_close_connection: Redispatch<(Uid, Uid)>, // (server_uid, connection_uid)
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    NewResult {
        server: Uid,
        result: OrError<()>,
    },
    NewConnectionEvent {
        server: Uid,
        connection: Uid,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    Close {
        connection: Uid,
    },
    CloseEvent {
        connection: Uid,
    },
    Send {
        uid: Uid,
        connection: Uid,
        data: Vec<u8>,
        timeout: Timeout,
        on_result: Redispatch<(Uid, SendResult)>,
    },
    SendNonceResult {
        uid: Uid,
        result: SendResult,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize, // number of bytes to read
        timeout: Timeout,
        on_result: Redispatch<(Uid, RecvResult)>,
    },
    RecvResult {
        uid: Uid,
        result: RecvResult,
    },
    RecvNonceResult {
        uid: Uid,
        result: RecvResult,
    },
}

impl Action for PnetServerAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}
