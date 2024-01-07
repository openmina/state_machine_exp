use std::rc::Rc;

use crate::{
    automaton::{
        action::{Action, ActionKind, ResultDispatch, Timeout},
        state::Uid,
    },
    models::pure::tcp::action::{ConnectionResult, RecvResult, SendResult, TcpPollResult},
};

#[derive(Debug)]
pub enum TcpClientPureAction {
    Connect {
        connection: Uid,
        address: String,
        timeout: Timeout,
        on_close_connection: ResultDispatch<Uid>,
        on_result: ResultDispatch<(Uid, ConnectionResult)>,
    },
    Poll {
        uid: Uid,
        timeout: Timeout,
        on_result: ResultDispatch<(Uid, TcpPollResult)>,
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

impl Action for TcpClientPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum TcpClientInputAction {
    ConnectResult {
        connection: Uid,
        result: ConnectionResult,
    },
    // PollResult {
    //     uid: Uid,
    //     result: TcpPollResult,
    // },
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

impl Action for TcpClientInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
