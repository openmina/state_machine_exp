use crate::{
    automaton::{
        action::{self, Action, ActionKind, OrError, Redispatch, Timeout},
        state::Uid,
    },
    models::effectful::mio::action::MioEvent,
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
    PollCreateSuccess {
        poll: Uid,
    },
    PollCreateError {
        poll: Uid,
        error: String,
    },
    EventsCreate {
        uid: Uid,
    },
    Listen {
        tcp_listener: Uid,
        address: String,
        on_result: Redispatch<(Uid, OrError<()>)>,
    },
    ListenSuccess {
        listener: Uid,
    },
    ListenError {
        listener: Uid,
        error: String,
    },
    RegisterListenerSuccess {
        listener: Uid,
    },
    RegisterListenerError {
        listener: Uid,
        error: String,
    },
    Accept {
        connection: Uid,
        listener: Uid,
        on_result: Redispatch<(Uid, ConnectionResult)>,
    },
    AcceptSuccess {
        connection: Uid,
    },
    AcceptTryAgain {
        connection: Uid,
    },
    AcceptError {
        connection: Uid,
        error: String,
    },
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_result: Redispatch<(Uid, ConnectionResult)>,
    },
    ConnectSuccess {
        connection: Uid,
    },
    ConnectError {
        connection: Uid,
        error: String,
    },
    GetPeerAddressSuccess {
        connection: Uid,
        address: String,
    },
    GetPeerAddressError {
        connection: Uid,
        error: String,
    },
    RegisterConnectionSuccess {
        connection: Uid,
    },
    RegisterConnectionError {
        connection: Uid,
        error: String,
    },
    DeregisterConnectionSuccess {
        connection: Uid,
    },
    DeregisterConnectionError {
        connection: Uid,
        error: String,
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
    PollSuccess {
        uid: Uid,
        events: Vec<MioEvent>,
    },
    PollInterrupted {
        uid: Uid,
    },
    PollError {
        uid: Uid,
        error: String,
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
    SendSuccess {
        uid: Uid,
    },
    SendSuccessPartial {
        uid: Uid,
        written: usize,
    },
    SendErrorInterrupted {
        uid: Uid,
    },
    SendErrorTryAgain {
        uid: Uid,
    },
    SendError {
        uid: Uid,
        error: String,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize,
        timeout: Timeout,
        on_result: Redispatch<(Uid, RecvResult)>,
    },
    RecvSuccess {
        uid: Uid,
        data: Vec<u8>,
    },
    RecvSuccessPartial {
        uid: Uid,
        data: Vec<u8>,
    },
    RecvErrorInterrupted {
        uid: Uid,
    },
    RecvErrorTryAgain {
        uid: Uid,
    },
    RecvError {
        uid: Uid,
        error: String,
    },
}

impl Action for TcpAction {
    const KIND: ActionKind = ActionKind::Pure;
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
