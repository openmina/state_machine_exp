use std::any::Any;

use colored::Colorize;
use log::debug;

use super::{
    action::{AnyAction, Dispatcher},
    state::{ModelState, State},
};

// The following code enables polymorphic handling of different *model* types
// and their actions in a state-machine setup. A *model* is defined in terms of
// the type of actions it can process, and uses dynamic dispatch to call the
// correct processing method.
//
// A model implements one of the `PureModel`, `InputModel`, and `OutputModel`
// traits. Each trait defines a specific `Action` associated type, and a
// corresponding processing method (`process_pure`, `process_input`, or
// `process_output`).
//
// Models implementing the `PureModel`, `InputModel`, or `OutputModel` traits
// are wrapped by the `Pure`, `Input`, and `Output` structs. These are used to
// implement the `PrivateModel` trait that provides `into_vtable*` methods so
// models can be converted into the `AnyModel` type.
//
// Finally, the `AnyModel` type is a central struct that can hold any model and
// handles actions via its virtual method table. It provides the methods
// (`process_pure`, `process_input`, `process_output`) to process different
// kinds of actions.

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

    fn into_vtable2<Substates: ModelState>() -> AnyModel<Substates> {
        let model = Box::new(()); // placeholder
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
    type Action: std::fmt::Debug + Sized + 'static;

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
        let Ok(unboxed_action) = action.ptr.downcast::<T::Action>() else {
            panic!("action not found")
        };

        let from = if action.depth == 0 {
            format!(
                "from DISPATCHER TICK ({}→{})",
                action.caller, action.action_id
            )
        } else {
            format!(
                "from {}:{} ({}→{})",
                action.dispatched_from_file,
                action.dispatched_from_line,
                action.caller,
                action.action_id
            )
        };

        let pad = "  ".repeat(action.depth);
        debug!(
            "{}: →{} {}::{:?}\n\t\t {} {}",
            state.get_current_instance(),
            pad,
            action.type_name,
            unboxed_action,
            pad,
            from.bright_black()
        );

        dispatcher.depth = action.depth;
        dispatcher.caller = action.action_id;
        T::process_pure(state, *unboxed_action, dispatcher)
    }
}

pub trait InputModel
where
    Self: Sized + 'static,
{
    type Action: std::fmt::Debug + Sized + 'static;

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
        let Ok(unboxed_action) = action.ptr.downcast::<T::Action>() else {
            panic!("action not found")
        };

        let pad = "  ".repeat(action.depth);
        debug!(
            "{}: ←{} {}::{}\n\t\t {} {}",
            state.get_current_instance(),
            pad,
            action.type_name.bright_cyan(),
            format!("{:?}", unboxed_action).bright_cyan(),
            pad,
            format!(
                "from {}:{} ({}→{})",
                action.dispatched_from_file,
                action.dispatched_from_line,
                action.caller,
                action.action_id
            )
            .bright_black()
        );

        dispatcher.depth = action.depth;
        dispatcher.caller = action.action_id;
        // TODO: add record logic
        T::process_input(state, *unboxed_action, dispatcher)
    }
}

pub trait OutputModel
where
    Self: Sized + 'static,
{
    type Action: std::fmt::Debug + Sized + 'static;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher);
}

pub struct Output<T: OutputModel>(pub T);

impl<T: OutputModel> PrivateModel for Output<T> {
    fn process_output(state: &mut Box<dyn Any>, action: AnyAction, dispatcher: &mut Dispatcher) {
        let Some(state) = state.downcast_mut::<Self>() else {
            panic!("model's state not found");
        };

        let Ok(unboxed_action) = action.ptr.downcast::<T::Action>() else {
            panic!("action not found")
        };

        let pad = "  ".repeat(action.depth);
        debug!(
            "→{} {}::{}\n\t\t {} {}",
            pad,
            action.type_name.bright_yellow(),
            format!("{:?}", unboxed_action).bright_yellow(),
            pad,
            format!(
                "from {}:{} ({}→{})",
                action.dispatched_from_file,
                action.dispatched_from_line,
                action.caller,
                action.action_id
            )
            .bright_black()
        );
        dispatcher.depth = action.depth;
        dispatcher.caller = action.action_id;
        state.0.process_output(*unboxed_action, dispatcher)
    }
}
