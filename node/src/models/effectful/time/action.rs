use crate::automaton::{
    action::{Action, ActionKind, Redispatch},
    state::Uid,
};
use serde_derive::{Deserialize, Serialize};
use std::time::Duration;
use type_uuid::TypeUuid;

#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "3221c0d5-02f5-4ed6-bf79-29f40c5619f0"]
pub enum TimeOutputAction {
    GetSystemTime {
        uid: Uid,
        on_result: Redispatch<(Uid, Duration)>,
    },
}

impl Action for TimeOutputAction {
    fn kind(&self) -> ActionKind {
        ActionKind::Output
    }
}
