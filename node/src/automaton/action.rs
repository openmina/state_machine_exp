use std::{
    any::{Any, TypeId},
    collections::VecDeque,
    fmt,
};

use colored::Colorize;
use log::debug;

// Actions can fall into 3 categories:
//
// 1. `Pure`: these are both dispatched and processed by `PureModel`s.
//    They can change the state-machine state but they don't cause any other side effects.
//    We don't record/replay them since they can be re-generated deterministically.
//
// 2. `Input`: similarly to `Pure` actions, they can change the state-machine state but
//    they don't cause any other side effects.
//    They are dispatched with `completion_dispatch` either from:
//
//      - An `OutputModel`: to bring information from the "external world" to the
//        state-machine (in response to some IO operation).
//
//      - An `InputModel`: to further propagate the input to the `CompletionRoutine` of
//        a caller model.
//
//    These must be recorded:
//      Note that in theory, we only need to record the first input action (dispatched by
//      the `OutputModel`) of a chain of `Input` actions. However, to reproduce the rest
//      of them (the ones dispatched by `InputModel`s) deterministically, it is required
//      that the `CompletionRoutine` type can be serialised/deserialized.
//
//      The purpose of the `CompletionRoutine` is to provide a mechanism so the callee
//      can convert the result of an operation into an action that gets dispatched to the
//      caller. To do so, the caller assigns the `CompletionRoutine` to pointer to a
//      caller-defined function that performs this result->action conversion.
//
//      To avoid the extra complexity of serializing/deserializing caller-defined function
//      pointers, we just skip them and instead we record/replay *all* `Input` actions.
//
// 3. `Output`: these are processed by `OutputModel`s performing IO to the "external world".
//    They don't have access to the state-machine state but they access state that is
//    specific to the `OutputModel`.
//
#[derive(Debug)]
pub enum ActionKind {
    Pure,
    Input,
    Output,
}

pub trait Action
where
    Self: 'static,
{
    const KIND: ActionKind;
}

#[derive(Debug)]
pub struct AnyAction {
    pub id: TypeId,
    pub ptr: Box<dyn Any>,
    pub kind: ActionKind,
    /// For printing/debug purpose only
    pub type_name: &'static str,
    pub dispatched_from_file: &'static str,
    pub dispatched_from_line: u32,
    pub depth: usize,
    pub action_id: u64,
    pub caller: u64, // action id of caller action
}

impl<T: Action> From<T> for AnyAction {
    fn from(v: T) -> Self {
        // Panic when calling `AnyAction::from(AnyAction { .. })`
        assert_ne!(TypeId::of::<T>(), TypeId::of::<AnyAction>());
        Self {
            id: TypeId::of::<T>(),
            ptr: Box::new(v),
            kind: T::KIND,
            type_name: std::any::type_name::<T>(),
            dispatched_from_file: "",
            dispatched_from_line: 0,
            depth: 0,
            action_id: 0,
            caller: 0
        }
    }
}

#[derive(PartialEq, Clone)]
pub struct ResultDispatch<R: Clone>(fn(R) -> AnyAction);

impl<R: Clone> ResultDispatch<R> {
    pub fn new(ptr: fn(R) -> AnyAction) -> Self {
        Self(ptr)
    }

    pub fn make(&self, value: R) -> AnyAction {
        self.0(value)
    }
}

impl<R: Clone> fmt::Debug for ResultDispatch<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "...")
    }
}

pub struct Dispatcher {
    queue: VecDeque<AnyAction>,
    tick: fn() -> AnyAction,
    // for debugging purposes
    pub depth: usize,
    pub action_id: u64,
    pub caller: u64, // action id of the action being processed when the new action was dispatched
}

impl Dispatcher {
    pub fn new(tick: fn() -> AnyAction) -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
            tick,
            depth: 0,
            action_id: 0,
            caller: 0
        }
    }

    pub fn next_action(&mut self) -> AnyAction {
        self.queue.pop_front().unwrap_or_else(|| {
            let mut any_action = (self.tick)();

            any_action.action_id = self.action_id;
            any_action.caller = 0;
            self.depth = 0;
            self.action_id += 1;
            self.caller = 0;
            any_action
        })
    }

    pub fn dispatch<A: Action>(&mut self, action: A, file: &'static str, line: u32)
    where
        A: Sized + 'static,
    {
        assert_ne!(TypeId::of::<A>(), TypeId::of::<AnyAction>());
        let mut any_action: AnyAction = action.into();

        any_action.dispatched_from_file = file;
        any_action.dispatched_from_line = line;
        any_action.depth = self.depth + 1;
        any_action.action_id = self.action_id;
        any_action.caller = self.caller;
        self.action_id += 1;
        self.queue.push_back(any_action);
    }

    pub fn dispatch_back<R: Clone>(
        &mut self,
        on_result: &ResultDispatch<R>,
        result: R,
        file: &'static str,
        line: u32,
    ) where
        R: Sized + 'static,
    {
        let mut any_action = on_result.make(result);
        assert_ne!(any_action.id, TypeId::of::<AnyAction>());
        assert!(matches!(any_action.kind, ActionKind::Input));

        any_action.dispatched_from_file = file;
        any_action.dispatched_from_line = line;
        any_action.depth = self.depth.saturating_sub(1);
        any_action.action_id = self.action_id;
        any_action.caller = self.caller;
        self.action_id += 1;
        self.queue.push_back(any_action);
    }
}

#[macro_export]
macro_rules! dispatch {
    ($dispatcher:expr, $action:expr) => {
        $dispatcher.dispatch($action, file!(), line!())
    };
}

#[macro_export]
macro_rules! dispatch_back {
    ($dispatcher:expr, $on_result:expr, $result:expr) => {
        $dispatcher.dispatch_back($on_result, $result, file!(), line!())
    };
}
