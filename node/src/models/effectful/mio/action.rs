use std::rc::Rc;

use crate::automaton::{
    action::{Action, ActionKind, CompletionRoutine},
    state::Uid,
};

#[derive(Clone)]
pub enum TcpWriteResult {
    WrittenAll,
    WrittenPartial(usize),
    Interrupted,
    Error(String),
}

#[derive(Clone)]
pub enum TcpReadResult {
    ReadAll(Vec<u8>),
    ReadPartial(Vec<u8>),
    Interrupted,
    Error(String),
}

#[derive(Clone)]
pub struct MioEvent {
    pub token: Uid,
    pub readable: bool,
    pub writable: bool,
    pub error: bool,
    pub read_closed: bool,
    pub write_closed: bool,
    pub priority: bool,
    pub aio: bool,
    pub lio: bool,
}

#[derive(Clone)]
pub enum PollEventsResult {
    Events(Vec<MioEvent>),
    Interrupted,
    Error(String),
}

pub enum MioAction {
    PollCreate {
        uid: Uid,
        on_completion: CompletionRoutine<(Uid, bool)>,
    },
    PollRegisterTcpServer {
        poll_uid: Uid,         // created by PollCreate
        tcp_listener_uid: Uid, // created by TcpListen
        on_completion: CompletionRoutine<(Uid, bool)>,
    },
    PollRegisterTcpConnection {
        poll_uid: Uid,       // created by PollCreate
        connection_uid: Uid, // created by TcpAccept (TODO: outgoing connections)
        on_completion: CompletionRoutine<(Uid, bool)>,
    },
    PollDeregisterTcpConnection {
        poll_uid: Uid,       // created by PollCreate
        connection_uid: Uid, // created by TcpAccept (TODO: outgoing connections)
        on_completion: CompletionRoutine<(Uid, bool)>,
    },
    PollEvents {
        uid: Uid,             // request uid (passed to the completion routine)
        poll_uid: Uid,        // created by PollCreate
        events_uid: Uid,      // created by EventsCreate
        timeout: Option<u64>, // timeout in milliseconds
        on_completion: CompletionRoutine<(Uid, PollEventsResult)>,
    },
    EventsCreate {
        uid: Uid,
        capacity: usize,
        on_completion: CompletionRoutine<Uid>,
    },
    TcpListen {
        uid: Uid,
        address: String,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    TcpAccept {
        uid: Uid,
        listener_uid: Uid, // created by TcpListen
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    TcpWrite {
        uid: Uid, // request uid (passed to the completion routine)
        connection_uid: Uid,
        // Strictly speaking, we should pass a copy here instead of referencing memory,
        // but the Rc guarantees immutability, allowing safe and efficient data sharing.
        data: Rc<[u8]>,
        on_completion: CompletionRoutine<(Uid, TcpWriteResult)>,
    },
    TcpRead {
        // not associated to any resources but passed back to the completion routine
        uid: Uid,
        connection_uid: Uid,
        len: usize, // max number of bytes to read
        on_completion: CompletionRoutine<(Uid, TcpReadResult)>,
    },
}

impl Action for MioAction {
    const KIND: ActionKind = ActionKind::Output;
}
