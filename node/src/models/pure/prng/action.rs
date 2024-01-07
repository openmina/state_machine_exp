use crate::automaton::action::{Action, ActionKind};

#[allow(dead_code)]
#[derive(Debug)]
pub enum PRNGPureAction {
    Reseed { seed: u64 },
}

impl Action for PRNGPureAction {
    const KIND: ActionKind = ActionKind::Pure;
}
