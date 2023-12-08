use std::rc::Rc;

use crate::{
    automaton::{
        action::{Action, ActionKind, CompletionRoutine},
        state::Uid,
    },
    models::effectful::mio::action::{PollEventsResult, TcpWriteResult},
};


#[derive(PartialEq, Clone, Debug)]
pub enum InitResult {
    Success,
    Error(String),
}

#[derive(Clone, Debug)]
pub enum ListenerEvent {
    AcceptPending(usize),
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

#[derive(Clone, Debug)]
pub enum PollResult {
    Events(Vec<(Uid, Event)>),
    Error(String),
}

#[derive(Clone)]
pub enum RecvResult {
    Success(Rc<Vec<u8>>),
    Error(String),
}

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
}

impl Action for TcpCallbackAction {
    const KIND: ActionKind = ActionKind::Input;
}
