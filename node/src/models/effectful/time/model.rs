use super::{action::TimeOutputAction, state::TimeState};
use crate::automaton::{
    action::Dispatcher,
    model::{Output, OutputModel},
    runner::{RegisterModel, RunnerBuilder},
    state::ModelState,
};
use std::time::{SystemTime, UNIX_EPOCH};

// This is an `OutputModel` responsible for handling time-related actions.
//
// As of now, it supports one action: getting the system time.
//
// The `GetSystemTime` action gets the current system time since the UNIX_EPOCH
// and dispatches the result back as an `InputAction`` defined by the caller.

impl RegisterModel for TimeState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.model_output(Output::<Self>(Self()))
    }
}

impl OutputModel for TimeState {
    type Action = TimeOutputAction;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher) {
        match action {
            TimeOutputAction::GetSystemTime { uid, on_result } => {
                let since_epoch = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System clock set before UNIX_EPOCH");

                dispatcher.dispatch_back(&on_result, (uid, since_epoch));
            }
        }
    }
}
