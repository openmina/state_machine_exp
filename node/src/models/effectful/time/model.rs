use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    automaton::{action::Dispatcher, model::OutputModel},
    dispatch_back,
};

use super::{action::TimeOutputAction, state::TimeState};

impl OutputModel for TimeState {
    type Action = TimeOutputAction;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher) {
        match action {
            TimeOutputAction::GetSystemTime { uid, on_result } => {
                let since_epoch = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System clock set before UNIX_EPOCH");

                dispatch_back!(dispatcher, &on_result, (uid, since_epoch));
            }
        }
    }
}
