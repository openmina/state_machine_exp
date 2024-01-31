use crate::{
    automaton::{
        action::{self, Action, ActionKind, OrError, Redispatch, Timeout},
        state::Uid,
    },
    models::effectful::mio::action::{PollResult, TcpAcceptResult, TcpReadResult, TcpWriteResult},
};
use serde_derive::{Deserialize, Serialize};
use std::rc::Rc;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "2fbd467c-1fb0-4190-89e1-7a0e756f63a4"]
pub enum TcpAction {
    Init {
        instance: Uid,
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    PollCreateResult {
        poll: Uid,
        result: OrError<()>,
    },
    EventsCreateResult {
        uid: Uid,
    },
    Listen {
        tcp_listener: Uid,
        address: String,
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    ListenResult {
        tcp_listener: Uid,
        result: OrError<()>,
    },
    RegisterListenerResult {
        tcp_listener: Uid,
        result: OrError<()>,
    },
    Accept {
        connection: Uid,
        tcp_listener: Uid,
        on_result: Redispatch<(Uid, ConnectionResult)>,
    },
    AcceptResult {
        connection: Uid,
        result: TcpAcceptResult,
    },
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_result: Redispatch<(Uid, ConnectionResult)>,
    },
    ConnectResult {
        connection: Uid,
        result: OrError<()>,
    },
    PeerAddressResult {
        connection: Uid,
        result: OrError<String>,
    },
    RegisterConnectionResult {
        connection: Uid,
        result: OrError<()>,
    },
    DeregisterConnectionResult {
        connection: Uid,
        result: OrError<()>,
    },
    Close {
        connection: Uid,
        on_result: Redispatch<Uid>,
    },
    CloseResult {
        connection: Uid,
    },
    Poll {
        uid: Uid,
        objects: Vec<Uid>,
        timeout: Timeout,
        on_result: Redispatch<(Uid, TcpPollResult)>,
    },
    PollResult {
        uid: Uid,
        result: PollResult,
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
        result: TcpWriteResult,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize,
        timeout: Timeout,
        on_result: Redispatch<(Uid, RecvResult)>,
    },
    RecvResult {
        uid: Uid,
        result: TcpReadResult,
    },
}

impl Action for TcpAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ListenerEvent {
    AcceptPending,
    AllAccepted,
    Closed,
    Error,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConnectionEvent {
    Ready { can_recv: bool, can_send: bool },
    Closed,
    Error,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Event {
    Listener(ListenerEvent),
    Connection(ConnectionEvent),
}

pub type TcpPollResult = OrError<Vec<(Uid, Event)>>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RecvResult {
    Success(Vec<u8>),
    Timeout(Vec<u8>),
    Error(String),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum SendResult {
    Success,
    Timeout,
    Error(String),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConnectResult {
    Success,
    Timeout,
    Error(String),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum AcceptResult {
    Success,
    WouldBlock,
    Error(String),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConnectionResult {
    Incoming(AcceptResult),
    Outgoing(ConnectResult),
}
