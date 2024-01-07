use std::{
    any::{Any, TypeId},
    collections::VecDeque,
    fmt,
};

#[derive(Clone, Debug)]
pub enum Timeout {
    Millis(u64),
    Never,
}

#[derive(Clone, Debug)]
pub enum TimeoutAbsolute {
    Millis(u128),
    Never,
}

// Actions fall into 3 categories:
//
// 1. `Pure`: these are both dispatched and processed by `PureModel`s.
//    They can change the state-machine state but they don't cause any other
//    side-effects. We don't need to record/replay them since they can be
//    re-generated deterministically.
//
// 2. `Input`: these can change the state-machine state but they don't cause
//    side-effects. They are dispatched by `dispatch_back` and contain the
//    result (`ResultDispatch`) of the processing of an action.
//    If the processed action was an `Output` action, the resulting `Input`
//    action brings information from the "external world" to the state-machine.
//
//    `Input` actions must be recorded: in theory, we only need to record the
//    input actions dispatched by `OutputModel`s, however to deterministically
//    reproduce `Input` actions dispatched from other sources, it should be
//    required that `ResultDispatch` can be serialised and deserialized.
//    `ResultDispatch` provides a mechanism where the callee (the dispatcher
//    of an action) can convert the action's result into an `Input` action
//    that gets dispatched (`dispatch_back`) to the caller. To do so, the
//    caller includes in the action a pointer to a caller-defined function that
//    performs this "result to `Input` action" conversion. To avoid the extra
//    complexity of serializing/deserializing function pointers we skip them,
//    instead we record/replay *all* `Input` actions.
//
// 3. `Output`: these are handled by `OutputModel`s to communicate to the
//    "external world". `OutputModel`s don't have access to the state-machine
//    state, but they have their own (minimal) state.
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
    // action id of caller action
    pub caller: u64,
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
            caller: 0,
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
    // This is a caller-defined function that produces and dispatches an action
    // when the action queue is empty. To the state-mache, the "tick" action is
    // analogous to the clock-cycle of a CPU.
    tick: fn() -> AnyAction,

    // The following fields are for debugging purposes:

    // The nesting level into the action chain: if action `A` has `depth=1` and
    // its handler dispatches `B`,then `B` has `depth=2`.
    pub depth: usize,

    // Every dispatched action has an unique `action_id` for the lifetime of
    // the state-machine. In combination with an action's `caller` we can
    // reconstruct the flow graph of all actions.
    pub action_id: u64,
    // The action's `caller` is the `action_id` of the action that was being
    // handled at the moment the current action was dispatched.
    pub caller: u64,
}

impl Dispatcher {
    pub fn new(tick: fn() -> AnyAction) -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
            tick,
            depth: 0,
            action_id: 0,
            caller: 0,
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

// The following macros are wrappers to the `dispatch` and `dispatch_back`
// methods so we can include into every action the source code file and
// line number where the action was dispatched. Their purpose is just to
// aid with debugging.

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
