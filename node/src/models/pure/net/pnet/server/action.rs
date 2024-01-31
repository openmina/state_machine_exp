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
pub enum PnetServerPureAction {
    New {
        address: String,
        server: Uid,
        max_connections: usize,
        on_new_connection: Redispatch<(Uid, Uid)>,
        on_close_connection: Redispatch<(Uid, Uid)>, // (server_uid, connection_uid)
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    Close {
        connection: Uid,
    },
    Send {
        uid: Uid,
        connection: Uid,
        data: Vec<u8>,
        timeout: Timeout,
        on_result: Redispatch<(Uid, SendResult)>,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize, // number of bytes to read
        timeout: Timeout,
        on_result: Redispatch<(Uid, RecvResult)>,
    },
}

impl Action for PnetServerPureAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "9c55db43-3a8e-4ac0-b52e-221b7b87206b"]
pub enum PnetServerInputAction {
    NewResult { server: Uid, result: OrError<()> },
    NewConnection { server: Uid, connection: Uid },
    SendNonceResult { uid: Uid, result: SendResult },
    RecvNonceResult { uid: Uid, result: RecvResult },
    Closed { connection: Uid },
    RecvResult { uid: Uid, result: RecvResult },
}

impl Action for PnetServerInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
