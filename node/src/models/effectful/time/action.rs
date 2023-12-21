use std::time::Duration;

use crate::automaton::{
    action::{Action, ActionKind, ResultDispatch},
    state::Uid,
};

#[derive(Debug)]
pub enum TimeOutputAction {
    GetSystemTime {
        uid: Uid,
        on_result: ResultDispatch<(Uid, Duration)>,
    },
}

impl Action for TimeOutputAction {
    const KIND: ActionKind = ActionKind::Output;
}
