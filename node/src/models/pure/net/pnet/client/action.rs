use crate::{
    automaton::{
        action::{Action, ActionKind, Redispatch, Timeout},
        state::Uid,
    },
    models::pure::net::tcp::action::TcpPollEvents,
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "1a161896-de5f-46b2-8774-e60e8a34ef9f"]
pub enum PnetClientAction {
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_success: Redispatch<(Uid, TcpPollEvents)>,
        on_error: Redispatch<(Uid, String)>,
    },
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_success: Redispatch<Uid>,
        on_timeout: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
        on_close: Redispatch<Uid>,
    },
    ConnectSuccess {
        connection: Uid,
    },
    ConnectTimeout {
        connection: Uid,
    },
    ConnectError {
        connection: Uid,
        error: String,
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
        on_success: Redispatch<Uid>,
        on_timeout: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
    },
    // No need for SendSuccess, SendTimeout, or SendError actions because we forward the on_* callbacks
    SendNonceSuccess {
        uid: Uid,
    },
    SendNonceTimeout {
        uid: Uid,
    },
    SendNonceError {
        uid: Uid,
        error: String,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize, // number of bytes to read
        timeout: Timeout,
        on_success: Redispatch<(Uid, Vec<u8>)>,
        on_timeout: Redispatch<(Uid, Vec<u8>)>,
        on_error: Redispatch<(Uid, String)>,
    },
    RecvSuccess {
        uid: Uid,
        data: Vec<u8>,
    },
    RecvTimeout {
        uid: Uid,
        partial_data: Vec<u8>,
    },
    RecvError {
        uid: Uid,
        error: String,
    },
    RecvNonceSuccess {
        uid: Uid,
        nonce: Vec<u8>,
    },
    RecvNonceTimeout {
        uid: Uid,
        partial_data: Vec<u8>,
    },
    RecvNonceError {
        uid: Uid,
        error: String,
    },
}

impl Action for PnetClientAction {
    const KIND: ActionKind = ActionKind::Pure;
}
