use std::time::Duration;

use crate::automaton::{
    action::{Action, ActionKind, CompletionRoutine},
    state::Uid,
};

pub enum TimeOutputAction {
    GetSystemTime {
        uid: Uid,
        on_completion: CompletionRoutine<(Uid, Duration)>,
    },
}

impl Action for TimeOutputAction {
    const KIND: ActionKind = ActionKind::Output;
}
