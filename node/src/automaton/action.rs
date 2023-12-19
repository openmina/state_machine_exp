use std::{
    any::{Any, TypeId},
    collections::VecDeque, fmt,
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
}

impl AnyAction {
    pub fn from<T: Action>(v: T) -> Self {
        // Panic when calling `AnyAction::from(AnyAction { .. })`
        assert_ne!(TypeId::of::<T>(), TypeId::of::<AnyAction>());
        Self {
            id: TypeId::of::<T>(),
            ptr: Box::new(v),
            kind: T::KIND,
            type_name: std::any::type_name::<T>(),
        }
    }
}

#[derive(PartialEq, Clone)]
pub struct CompletionRoutine<R: Clone>(fn(R) -> AnyAction);

impl<R: Clone> CompletionRoutine<R> {
    pub fn new(ptr: fn(R) -> AnyAction) -> Self {
        Self(ptr)
    }

    pub fn make(&self, value: R) -> AnyAction {
        self.0(value)
    }
}

impl<R: Clone> fmt::Debug for CompletionRoutine<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "...")
    }
}

pub struct Dispatcher {
    queue: VecDeque<AnyAction>,
    tick: fn() -> AnyAction,
    pub depth: usize // for debug logs
}

impl Dispatcher {
    pub fn new(tick: fn() -> AnyAction) -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
            tick,
            depth: 0
        }
    }

    pub fn next_action(&mut self) -> AnyAction {
        self.queue.pop_front().unwrap_or_else(|| {
            debug!("|DISPATCHER| {}", "TICK callback".yellow());
            self.depth = 0;
            (self.tick)()
        }
        )
    }

    pub fn dispatch<A: Action>(&mut self, action: A)
    where
        A: Sized + 'static,
    {
        assert_ne!(TypeId::of::<A>(), TypeId::of::<AnyAction>());
        self.queue.push_back(AnyAction::from(action));
    }

    pub fn completion_dispatch<R: Clone>(&mut self, on_completion: &CompletionRoutine<R>, result: R)
    where
        R: Sized + 'static,
    {
        let action = on_completion.make(result);
        assert_ne!(action.id, TypeId::of::<AnyAction>());
        assert!(matches!(action.kind, ActionKind::Input));
        self.queue.push_back(action);
    }
}
