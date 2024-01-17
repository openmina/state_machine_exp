use std::time::Duration;

use super::{action::TimePureAction, state::TimeState};
use crate::{
    automaton::{
        action::{Dispatcher, Timeout, TimeoutAbsolute},
        model::{InputModel, PureModel},
        state::{ModelState, State, Uid},
    },
    callback,
    models::{effectful::time::action::TimeOutputAction, pure::time::action::TimeInputAction},
};

pub fn update_time<Substate: ModelState>(
    state: &mut State<Substate>,
    dispatcher: &mut Dispatcher,
) -> bool {
    let tick = state.substate_mut::<TimeState>().tick();

    if tick {
        dispatcher.dispatch(TimePureAction::UpdateCurrentTime);
    }

    return tick;
}

pub fn get_current_time<Substate: ModelState>(state: &State<Substate>) -> u128 {
    state.substate::<TimeState>().now().as_millis()
}

pub fn get_timeout_absolute<Substate: ModelState>(
    state: &State<Substate>,
    timeout: Timeout,
) -> TimeoutAbsolute {
    // Convert relative the timeout we passed to absolute timeout by adding the current time
    match timeout {
        Timeout::Millis(ms) => {
            TimeoutAbsolute::Millis(get_current_time(state).saturating_add(ms.into()))
        }
        Timeout::Never => TimeoutAbsolute::Never,
    }
}

impl InputModel for TimeState {
    type Action = TimeInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        _dispatcher: &mut Dispatcher,
    ) {
        let TimeInputAction::GetSystemTimeResult { result, .. } = action;
        let time_state: &mut TimeState = state.substate_mut();

        time_state.set_time(result);
    }
}

impl PureModel for TimeState {
    type Action = TimePureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        assert!(matches!(action, TimePureAction::UpdateCurrentTime));
        dispatcher.dispatch(TimeOutputAction::GetSystemTime {
            uid: state.new_uid(),
            on_result: callback!(|(uid: Uid, result: Duration)| {
                TimeInputAction::GetSystemTimeResult { uid, result }
            }),
        })
    }
}
