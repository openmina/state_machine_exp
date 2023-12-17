use crate::automaton::{
    action::{Action, ActionKind},
    state::Uid,
};

pub enum EchoServerPureAction {
    Tick,
}

impl Action for EchoServerPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

pub enum EchoServerInputAction {
    Init {
        uid: Uid,
        result: Result<(), String>,
    },
    InitCompleted {
        uid: Uid,
        result: Result<(), String>,
    },
    NewConnection {
        server_uid: Uid,
        connection_uid: Uid,
    },
    Closed {
        server_uid: Uid,
        connection_uid: Uid,
    },
    Poll {
        uid: Uid,
        result: Result<(), String>,
    },
}

impl Action for EchoServerInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
