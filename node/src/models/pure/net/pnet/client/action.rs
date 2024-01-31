use crate::{
    automaton::{
        action::{Action, ActionKind, Redispatch, Timeout},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "1a161896-de5f-46b2-8774-e60e8a34ef9f"]
pub enum PnetClientAction {
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_close_connection: Redispatch<Uid>,
        on_result: Redispatch<(Uid, ConnectResult)>,
    },
    ConnectResult {
        connection: Uid,
        result: ConnectResult,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: Redispatch<(Uid, TcpPollResult)>,
    },
    Close {
        connection: Uid,
    },
    Closed {
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

impl Action for PnetClientAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}
