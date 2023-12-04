use std::rc::Rc;

use crate::automaton::{
    action::{Action, ActionKind, CompletionRoutine},
    state::Uid,
};

#[derive(Clone)]
pub enum TcpListenResult {
    Success,
    InvalidAddress,
    Error(String),
}

#[derive(Clone)]
pub enum TcpAcceptResult {
    Success,
    WouldBlock,
    Error(String),
}

#[derive(Clone)]
pub enum TcpWriteResult {
    WrittenAll,
    WrittenPartial(usize),
    WouldBlock,
    Interrupted,
    Error(String),
}

#[derive(Clone)]
pub enum TcpReadResult {
    ReadAll(Rc<Vec<u8>>),
    ReadPartial {
        bytes_read: Rc<Vec<u8>>,
        remaining: usize,
    },
    ConnectionClosed,
    WouldBlock,
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
        poll: Uid,         // created by PollCreate
        tcp_listener: Uid, // created by TcpListen
        token: Uid,        // unique Token for MIO
        on_completion: CompletionRoutine<(Uid, bool)>,
    },
    PollRegisterTcpConnection {
        poll: Uid,       // created by PollCreate
        connection: Uid, // created by TcpAccept (TODO: outgoing connections)
        token: Uid,      // unique Token for MIO
        on_completion: CompletionRoutine<(Uid, bool)>,
    },
    PollEvents {
        poll: Uid,            // created by PollCreate
        events: Uid,          // created by EventsCreate
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
        on_completion: CompletionRoutine<(Uid, TcpListenResult)>,
    },
    TcpAccept {
        uid: Uid,
        listener: Uid, // created by TcpListen
        on_completion: CompletionRoutine<(Uid, TcpAcceptResult)>,
    },
    TcpWrite {
        // not associated to any resources but passed back to the completion routine
        uid: Uid,
        connection: Uid,
        // Strictly speaking, we should pass a copy here instead of referencing memory,
        // but the Rc guarantees immutability, allowing safe and efficient data sharing.
        data: Rc<[u8]>,
        on_completion: CompletionRoutine<(Uid, TcpWriteResult)>,
    },
    TcpRead {
        // not associated to any resources but passed back to the completion routine
        uid: Uid,
        connection: Uid,
        len_bytes: usize, // max number of bytes to read
        on_completion: CompletionRoutine<(Uid, TcpReadResult)>,
    },
}


impl Action for MioAction {
    const KIND: ActionKind = ActionKind::Output;
}
