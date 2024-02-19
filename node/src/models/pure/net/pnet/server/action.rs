use crate::automaton::{
    action::{Action, ActionKind, Redispatch, Timeout},
    state::Uid,
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "7f93cd46-0dd7-4849-a823-c1231ea51f60"]
pub enum PnetServerAction {
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_success: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
    },
    New {
        address: String,
        listener: Uid,
        max_connections: usize,
        on_success: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
        on_new_connection: Redispatch<(Uid, Uid)>,
        on_new_connection_error: Redispatch<(Uid, Uid, String)>,
        on_connection_closed: Redispatch<(Uid, Uid)>,
        on_listener_closed: Redispatch<Uid>,
    },
    NewSuccess {
        listener: Uid,
    },
    NewError {
        listener: Uid,
        error: String,
    },
    ConnectionEvent {
        listener: Uid,
        connection: Uid,
    },
    ListenerCloseEvent {
        listener: Uid,
    },
    Close {
        connection: Uid,
    },
    CloseEvent {
        listener: Uid,
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

impl Action for PnetServerAction {
    const KIND: ActionKind = ActionKind::Pure;
}
