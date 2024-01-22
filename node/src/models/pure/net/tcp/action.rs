use crate::{
    automaton::{
        action::{self, Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::effectful::mio::action::{PollResult, TcpAcceptResult, TcpReadResult, TcpWriteResult},
};
use serde_derive::{Deserialize, Serialize};
use std::rc::Rc;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "2fbd467c-1fb0-4190-89e1-7a0e756f63a4"]
pub enum TcpPureAction {
    Init {
        instance: Uid,
        on_result: ResultDispatch,
    },
    Listen {
        tcp_listener: Uid,
        address: String,
        on_result: ResultDispatch,
    },
    Accept {
        connection: Uid,
        tcp_listener: Uid,
        on_result: ResultDispatch,
    },
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_result: ResultDispatch,
    },
    Close {
        connection: Uid,
        on_result: ResultDispatch,
    },
    Poll {
        uid: Uid,
        objects: Vec<Uid>,
        timeout: Timeout,
        on_result: ResultDispatch,
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
        count: usize,
        timeout: Timeout,
        on_result: ResultDispatch,
    },
}

impl Action for TcpPureAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "d37b48ca-42a6-4029-84e8-5ef6486cda6d"]
pub enum TcpInputAction {
    PollCreateResult {
        poll: Uid,
        result: Result<(), String>,
    },
    EventsCreateResult {
        uid: Uid,
    },
    ListenResult {
        tcp_listener: Uid,
        result: Result<(), String>,
    },
    AcceptResult {
        connection: Uid,
        result: TcpAcceptResult,
    },
    ConnectResult {
        connection: Uid,
        result: Result<(), String>,
    },
    CloseResult {
        connection: Uid,
    },
    RegisterConnectionResult {
        connection: Uid,
        result: Result<(), String>,
    },
    DeregisterConnectionResult {
        connection: Uid,
        result: Result<(), String>,
    },
    RegisterListenerResult {
        tcp_listener: Uid,
        result: Result<(), String>,
    },
    PollResult {
        uid: Uid,
        result: PollResult,
    },
    SendResult {
        uid: Uid,
        result: TcpWriteResult,
    },
    RecvResult {
        uid: Uid,
        result: TcpReadResult,
    },
    PeerAddressResult {
        connection: Uid,
        result: Result<String, String>,
    },
}

impl Action for TcpInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
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

pub type TcpPollResult = Result<Vec<(Uid, Event)>, String>;

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