use std::rc::Rc;

use crate::{
    automaton::{
        action::{Action, ActionKind, CompletionRoutine},
        state::Uid,
    },
    models::pure::tcp::{
        action::{ConnectResult, PollResult, RecvResult},
        state::SendResult,
    },
};

#[derive(Debug)]
pub enum TcpServerPureAction {
    New {
        uid: Uid,
        address: String,
        max_connections: usize,
        on_new_connection: CompletionRoutine<(Uid, Uid)>, // (server_uid, new_connection_uid)
        on_close_connection: CompletionRoutine<(Uid, Uid)>, // (server_uid, connection_uid)
        on_result: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Poll {
        uid: Uid,
        timeout: Option<u64>,
        on_result: CompletionRoutine<(Uid, Result<(), String>)>,
    },
    Close {
        connection_uid: Uid,
    },
    Send {
        uid: Uid,
        connection_uid: Uid,
        data: Rc<[u8]>,
        timeout: Option<u64>,
        on_result: CompletionRoutine<(Uid, SendResult)>,
    },
    Recv {
        uid: Uid,
        connection_uid: Uid,
        count: usize, // number of bytes to read
        timeout: Option<u64>,
        on_result: CompletionRoutine<(Uid, RecvResult)>,
    },
}

impl Action for TcpServerPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum TcpServerInputAction {
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
        result: ConnectResult,
    },
    Close {
        uid: Uid,
    },
    Send {
        uid: Uid,
        result: SendResult,
    },
    Recv {
        uid: Uid,
        result: RecvResult,
    },
}

impl Action for TcpServerInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
