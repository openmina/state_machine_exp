use crate::automaton::{
    action::{Action, ActionKind},
    state::Uid,
};
use serde_derive::{Deserialize, Serialize};
use std::time::Duration;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "1911e66d-e0e3-4efc-8952-c62f583059f6"]
pub enum TimePureAction {
    UpdateCurrentTime,
}

impl Action for TimePureAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Pure
    }
}

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "2bb94ee6-ac54-44d3-91fe-0332408848f5"]
pub enum TimeInputAction {
    GetSystemTimeResult { uid: Uid, result: Duration },
}

impl Action for TimeInputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}
