use crate::{
    automaton::{
        action::{self, Action, ActionKind, OrError, Redispatch, Timeout},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use std::rc::Rc;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "9bb1c88e-71c8-4a55-8074-cd3dd939a1fb"]
pub enum TcpServerAction {
    New {
        address: String,
        server: Uid,
        max_connections: usize,
        on_new_connection: Redispatch<(Uid, Uid)>, // (server_uid, new_connection_uid)
        on_close_connection: Redispatch<(Uid, Uid)>, // (server_uid, connection_uid)
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    NewResult {
        server: Uid,
        result: OrError<()>,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    PollResult {
        uid: Uid,
        result: TcpPollResult,
    },
    AcceptResult {
        connection: Uid,
        result: ConnectionResult,
    },
    Close {
        connection: Uid,
    },
    CloseResult {
        connection: Uid,
        notify: bool,
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
        on_result: Redispatch<(Uid, SendResult)>,
    },
    SendResult {
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
}

impl Action for TcpServerAction {
    const KIND: ActionKind = ActionKind::Pure;
}
