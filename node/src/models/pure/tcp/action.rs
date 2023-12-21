use std::rc::Rc;

use crate::{
    automaton::{
        action::{Action, ActionKind, ResultDispatch},
        state::Uid,
    },
    models::effectful::mio::action::{PollEventsResult, TcpReadResult, TcpWriteResult},
};

use super::state::SendResult;

#[derive(Clone, Debug)]
pub enum ListenerEvent {
    AcceptPending,
    ConnectionAccepted, // set by us when handling Accept action
    Closed,
    Error,
}

#[derive(Clone, Debug)]
pub enum ConnectionEvent {
    Ready { recv: bool, send: bool },
    Closed,
    Error,
}

#[derive(Clone, Debug)]
pub enum Event {
    Listener(ListenerEvent),
    Connection(ConnectionEvent),
}

pub type PollResult = Result<Vec<(Uid, Event)>, String>;

#[derive(Clone, Debug)]
pub enum RecvResult {
    Success(Vec<u8>),
    Timeout(Vec<u8>),
    Error(String),
}

#[derive(Clone, Debug)]
pub enum ConnectResult {
    Success,
    Timeout,
    Error(String),
}

#[derive(Debug)]
pub enum TcpPureAction {
    Init {
        init_uid: Uid, // TCP model instance
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    },
    Listen {
        uid: Uid,
        address: String,
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    },
    Accept {
        uid: Uid,
        listener_uid: Uid,
        on_result: ResultDispatch<(Uid, ConnectResult)>,
    },
    Connect {
        uid: Uid,
        address: String,
        timeout: Option<u64>, // timeout in milliseconds
        on_result: ResultDispatch<(Uid, ConnectResult)>,
    },
    Close {
        connection_uid: Uid,
        on_result: ResultDispatch<Uid>,
    },
    Poll {
        uid: Uid,
        objects: Vec<Uid>,    // TCP objects we are intereted in
        timeout: Option<u64>, // timeout in milliseconds
        on_result: ResultDispatch<(Uid, PollResult)>,
    },
    Send {
        uid: Uid,
        connection_uid: Uid,
        data: Rc<[u8]>,
        timeout: Option<u64>, // timeout in milliseconds
        on_result: ResultDispatch<(Uid, SendResult)>,
    },
    Recv {
        uid: Uid,
        connection_uid: Uid,
        count: usize,         // number of bytes to read
        timeout: Option<u64>, // timeout in milliseconds
        on_result: ResultDispatch<(Uid, RecvResult)>,
    },
}

impl Action for TcpPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum TcpInputAction {
    PollCreate {
        uid: Uid,
        success: bool,
    },
    EventsCreate(Uid),
    Listen {
        uid: Uid,
        result: Result<(), String>,
    },
    Accept {
        uid: Uid,
        result: Result<(), String>,
    },
    Connect {
        uid: Uid,
        result: Result<(), String>,
    },
    CloseConnection {
        uid: Uid,
    },
    RegisterConnection {
        uid: Uid,
        result: bool,
    },
    DeregisterConnection {
        uid: Uid,
        result: bool,
    },
    RegisterListener {
        uid: Uid,
        result: bool,
    },
    Poll {
        uid: Uid,
        result: PollEventsResult,
    },
    Send {
        uid: Uid,
        result: TcpWriteResult,
    },
    Recv {
        uid: Uid,
        result: TcpReadResult,
    },
    PeerAddress {
        uid: Uid,
        result: Result<String, String>,
    },
}

impl Action for TcpInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
