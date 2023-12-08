use std::rc::Rc;

use crate::{
    automaton::{
        action::{Action, ActionKind, CompletionRoutine},
        state::Uid,
    },
    models::effectful::mio::action::{PollEventsResult, TcpReadResult, TcpWriteResult},
};

#[derive(PartialEq, Clone, Debug)]
pub enum InitResult {
    Success,
    Error(String),
}

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
pub type RecvResult = Result<Vec<u8>, String>;

pub enum TcpAction {
    Init {
        init_uid: Uid, // TCP model instance
        on_completion: CompletionRoutine<(Uid, InitResult)>,
    },
    Listen {
        uid: Uid,
        address: String,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Accept {
        uid: Uid,
        listener_uid: Uid,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Poll {
        uid: Uid,
        objects: Vec<Uid>,    // TCP objects we are intereted in
        timeout: Option<u64>, // timeout in milliseconds
        on_completion: CompletionRoutine<(Uid, PollResult)>,
    },
    Send {
        uid: Uid, // server instance
        connection_uid: Uid,
        data: Rc<[u8]>,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Recv {
        uid: Uid, // server instance
        connection_uid: Uid,
        count: usize, // number of bytes to read
        on_completion: CompletionRoutine<(Uid, RecvResult)>,
    },
}

impl Action for TcpAction {
    const KIND: ActionKind = ActionKind::Pure;
}

pub enum TcpCallbackAction {
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
}

impl Action for TcpCallbackAction {
    const KIND: ActionKind = ActionKind::Input;
}
