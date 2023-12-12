use std::rc::Rc;

use crate::{
    automaton::{
        action::{Action, ActionKind, CompletionRoutine},
        state::Uid,
    },
    models::pure::tcp::action::{PollResult, RecvResult},
};

pub enum TcpServerAction {
    New {
        uid: Uid,
        address: String,
        max_connections: usize,
        on_new_connection: CompletionRoutine<(Uid, Uid)>, // (server_uid, new_connection_uid)
        on_close_connection: CompletionRoutine<(Uid, Uid)>, // (server_uid, connection_uid)
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Poll {
        uid: Uid,
        timeout: Option<u64>,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Send {
        uid: Uid,
        connection_uid: Uid,
        data: Rc<[u8]>,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Recv {
        uid: Uid,
        connection_uid: Uid,
        count: usize, // number of bytes to read
        on_completion: CompletionRoutine<(Uid, RecvResult)>,
    },
}

impl Action for TcpServerAction {
    const KIND: ActionKind = ActionKind::Pure;
}

pub enum TcpServerCallbackAction {
    New {
        uid: Uid,
        result: Result<(), String>,
    },
    Poll {
        uid: Uid,
        result: PollResult,
    },
    Accept {
        uid: Uid,
        result: Result<(), String>,
    },
    Send {
        uid: Uid,
        result: Result<(), String>,
    },
    Recv {
        uid: Uid,
        result: Result<Vec<u8>, String>,
    },
}

impl Action for TcpServerCallbackAction {
    const KIND: ActionKind = ActionKind::Input;
}
