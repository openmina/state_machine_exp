use crate::{
    automaton::{
        action::{ResultDispatch, Dispatcher},
        model::{InputModel, PureModel},
        state::{ModelState, State},
    },
    models::{effectful::time::action::TimeOutputAction, pure::time::action::TimeInputAction}, dispatch,
};

use super::{action::TimePureAction, state::TimeState};

impl InputModel for TimeState {
    type Action = TimeInputAction;

    fn process_input<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        _dispatcher: &mut Dispatcher,
    ) {
        let TimeInputAction::TimeUpdate { result, .. } = action;
        let time_state: &mut TimeState = state.models.state_mut();

        time_state.now = result;
    }
}

impl PureModel for TimeState {
    type Action = TimePureAction;

    fn process_pure<Substate: ModelState>(
        state: &mut State<Substate>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    ) {
        assert!(matches!(action, TimePureAction::Tick));
        dispatch!(dispatcher, TimeOutputAction::GetSystemTime {
            uid: state.new_uid(),
            on_result: ResultDispatch::new(|(uid, result)| {
                (TimeInputAction::TimeUpdate { uid, result }).into()
            }),
        })
    }
}
