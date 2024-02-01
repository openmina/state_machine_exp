use crate::{
    automaton::{
        action::{self, Action, ActionKind, Redispatch, Timeout},
        state::Uid,
    },
    models::pure::net::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};
use serde_derive::{Deserialize, Serialize};
use std::rc::Rc;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "f15cd869-0966-4ab5-881c-530bc0fe95e6"]
pub enum TcpClientAction {
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_close_connection: Redispatch<Uid>,
        on_result: Redispatch<(Uid, ConnectionResult)>,
    },
    ConnectResult {
        connection: Uid,
        result: ConnectionResult,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: Redispatch<(Uid, TcpPollResult)>,
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

impl Action for TcpClientAction {
    const KIND: ActionKind = ActionKind::Pure;
}
