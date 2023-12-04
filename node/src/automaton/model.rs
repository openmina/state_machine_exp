use std::any::Any;

use super::{
    action::{AnyAction, Dispatcher},
    state::{ModelState, State},
};

pub struct AnyModel<Substates: ModelState> {
    model: Box<dyn Any>,
    vtable: ModelVTable<Substates>,
}

impl<Substates: ModelState> AnyModel<Substates> {
    pub fn process_pure(
        &mut self,
        state: &mut State<Substates>,
        action: AnyAction,
        dispatcher: &mut Dispatcher,
    ) {
        (self.vtable.process_pure)(state, action, dispatcher)
    }

    pub fn process_input(
        &mut self,
        state: &mut State<Substates>,
        action: AnyAction,
        dispatcher: &mut Dispatcher,
    ) {
        (self.vtable.process_input)(state, action, dispatcher)
    }

    pub fn process_output(&mut self, action: AnyAction, dispatcher: &mut Dispatcher) {
        (self.vtable.process_output)(&mut self.model, action, dispatcher)
    }
}

struct ModelVTable<Substates: ModelState> {
    // `Pure` actions can access the state-machine state only
    process_pure: fn(state: &mut State<Substates>, action: AnyAction, dispatcher: &mut Dispatcher),

    // `Input` actions can access the state-machine state only
    process_input: fn(state: &mut State<Substates>, action: AnyAction, dispatcher: &mut Dispatcher),

    // `Output` actions access the state of the `OutputModel` (external) state
    // but they can't access the state-machine state
    process_output: fn(state: &mut Box<dyn Any>, action: AnyAction, dispatcher: &mut Dispatcher),
}

pub trait PrivateModel
where
    Self: Sized + 'static,
{
    fn into_vtable<Substates: ModelState>(self: Box<Self>) -> AnyModel<Substates> {
        let model = self;
        let vtable = ModelVTable {
            process_pure: Self::process_pure,
            process_input: Self::process_input,
            process_output: Self::process_output,
        };
        AnyModel { model, vtable }
    }

    fn process_pure<Substates: ModelState>(
        _state: &mut State<Substates>,
        _action: AnyAction,
        _dispatcher: &mut Dispatcher,
    ) {
        unreachable!()
    }

    fn process_input<Substates: ModelState>(
        _state: &mut State<Substates>,
        _action: AnyAction,
        _dispatcher: &mut Dispatcher,
    ) {
        unreachable!()
    }

    fn process_output(_state: &mut Box<dyn Any>, _action: AnyAction, _dispatcher: &mut Dispatcher) {
        unreachable!()
    }
}

pub trait PureModel
where
    Self: Sized + 'static,
{
    type Action: Sized + 'static;

    fn process_pure<Substates: ModelState>(
        state: &mut State<Substates>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    );
}

pub struct Pure<T: PureModel>(T);

impl<T: PureModel> PrivateModel for Pure<T> {
    fn process_pure<Substates: ModelState>(
        state: &mut State<Substates>,
        action: AnyAction,
        dispatcher: &mut Dispatcher,
    ) {
        let Ok(action) = action.ptr.downcast::<T::Action>() else {
            panic!("action not found")
        };
        T::process_pure(state, *action, dispatcher)
    }
}

pub trait InputModel
where
    Self: Sized + 'static,
{
    type Action: Sized + 'static;

    fn process_input<Substates: ModelState>(
        state: &mut State<Substates>,
        action: Self::Action,
        dispatcher: &mut Dispatcher,
    );
}

pub struct Input<T: InputModel>(T);

impl<T: InputModel> PrivateModel for Input<T> {
    fn process_input<Substates: ModelState>(
        state: &mut State<Substates>,
        action: AnyAction,
        dispatcher: &mut Dispatcher,
    ) {
        let Ok(action) = action.ptr.downcast::<T::Action>() else {
            panic!("action not found")
        };

        // TODO: add record logic
        T::process_input(state, *action, dispatcher)
    }
}

pub trait OutputModel
where
    Self: Sized + 'static,
{
    type Action: Sized + 'static;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher);
}

pub struct Output<T: OutputModel>(T);

impl<T: OutputModel> PrivateModel for Output<T> {
    fn process_output(state: &mut Box<dyn Any>, action: AnyAction, dispatcher: &mut Dispatcher) {
        let Some(state) = state.downcast_mut::<Self>() else {
            panic!("model's state not found");
        };

        let Ok(action) = action.ptr.downcast::<T::Action>() else {
            panic!("action not found")
        };

        state.0.process_output(*action, dispatcher)
    }
}
