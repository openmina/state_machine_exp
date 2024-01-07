use crate::{
    automaton::{
        action::{Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::effectful::mio::action::{PollResult, TcpAcceptResult, TcpReadResult, TcpWriteResult},
};
use std::rc::Rc;

#[derive(Debug)]
pub enum TcpPureAction {
    Init {
        instance: Uid,
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    },
    Listen {
        tcp_listener: Uid,
        address: String,
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    },
    Accept {
        connection: Uid,
        tcp_listener: Uid,
        on_result: ResultDispatch<(Uid, ConnectionResult)>,
    },
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, ConnectionResult)>,
    },
    Close {
        connection: Uid,
        on_result: ResultDispatch<Uid>,
    },
    Poll {
        uid: Uid,
        objects: Vec<Uid>,
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, TcpPollResult)>,
    },
    Send {
        uid: Uid,
        connection: Uid,
        data: Rc<[u8]>,
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, SendResult)>,
    },
    Recv {
        uid: Uid,
        connection: Uid,
        count: usize,
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, RecvResult)>,
    },
}

impl Action for TcpPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
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
    const KIND: ActionKind = ActionKind::Input;
}

#[derive(Clone, Debug)]
pub enum ListenerEvent {
    AcceptPending,
    AllAccepted,
    Closed,
    Error,
}

#[derive(Clone, Debug)]
pub enum ConnectionEvent {
    Ready { can_recv: bool, can_send: bool },
    Closed,
    Error,
}

#[derive(Clone, Debug)]
pub enum Event {
    Listener(ListenerEvent),
    Connection(ConnectionEvent),
}

pub type TcpPollResult = Result<Vec<(Uid, Event)>, String>;

#[derive(Clone, Debug)]
pub enum RecvResult {
    Success(Vec<u8>),
    Timeout(Vec<u8>),
    Error(String),
}

#[derive(Clone, Debug)]
pub enum SendResult {
    Success,
    Timeout,
    Error(String),
}

#[derive(Clone, Debug)]
pub enum ConnectResult {
    Success,
    Timeout,
    Error(String),
}

#[derive(Clone, Debug)]
pub enum AcceptResult {
    Success,
    WouldBlock,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ConnectionResult {
    Incoming(AcceptResult),
    Outgoing(ConnectResult),
}
