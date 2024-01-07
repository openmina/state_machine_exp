use std::time::Duration;

use crate::automaton::{
    action::{Action, ActionKind},
    state::Uid,
};

#[derive(Debug)]
pub enum TimePureAction {
    UpdateCurrentTime,
}

impl Action for TimePureAction {
    const KIND: ActionKind = ActionKind::Pure;
}

#[derive(Debug)]
pub enum TimeInputAction {
    GetSystemTimeResult { uid: Uid, result: Duration },
}

impl Action for TimeInputAction {
    const KIND: ActionKind = ActionKind::Input;
}
