use crate::{
    automaton::{
        action::{Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::pure::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};
use std::rc::Rc;

#[derive(Debug)]
pub enum TcpServerPureAction {
    New {
        address: String,
        server: Uid,
        max_connections: usize,
        on_new_connection: ResultDispatch<(Uid, Uid)>, // (server_uid, new_connection_uid)
        on_close_connection: ResultDispatch<(Uid, Uid)>, // (server_uid, connection_uid)
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    },
    Close {
        connection: Uid,
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
        count: usize, // number of bytes to read
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, RecvResult)>,
    },
}

impl Action for TcpServerPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum TcpServerInputAction {
    NewResult {
        server: Uid,
        result: Result<(), String>,
    },
    PollResult {
        uid: Uid,
        result: TcpPollResult,
    },
    AcceptResult {
        connection: Uid,
        result: ConnectionResult,
    },
    CloseInternalResult {
        connection: Uid,
    },
    CloseResult {
        connection: Uid,
    },
    SendResult {
        uid: Uid,
        result: SendResult,
    },
    RecvResult {
        uid: Uid,
        result: RecvResult,
    },
}

impl Action for TcpServerInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
