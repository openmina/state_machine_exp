use crate::automaton::{
    action::{Action, ActionKind},
    state::Uid,
};
use serde_derive::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "ab8ace39-22cd-4717-a446-e20442f7f0f1"]
pub enum PnetEchoServerAction {
    Tick,
    PollSuccess {
        uid: Uid,
    },
    PollError {
        uid: Uid,
        error: String,
    },
    InitSuccess {
        instance: Uid,
    },
    InitError {
        instance: Uid,
        error: String,
    },
    InitListenerSuccess {
        listener: Uid,
    },
    InitListenerError {
        listener: Uid,
        error: String,
    },
    ListenerCloseEvent {
        listener: Uid,
    },
    ConnectionEvent {
        listener: Uid,
        connection: Uid,
    },
    ConnectionErrorEvent {
        listener: Uid,
        connection: Uid,
        error: String,
    },
    CloseEvent {
        listener: Uid,
        connection: Uid,
    },
    SendSuccess {
        uid: Uid,
    },
    SendTimeout {
        uid: Uid,
    },
    SendError {
        uid: Uid,
        error: String,
    },
    RecvSuccess {
        uid: Uid,
        data: Vec<u8>,
    },
    RecvTimeout {
        uid: Uid,
        partial_data: Vec<u8>,
    },
    RecvError {
        uid: Uid,
        error: String,
    },
}

impl Action for PnetEchoServerAction {
    const KIND: ActionKind = ActionKind::Pure;
}
