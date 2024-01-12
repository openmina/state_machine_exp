use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};
use std::{
    any::{Any, TypeId},
    collections::VecDeque,
    fmt,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    ops::Deref,
    panic::Location,
    rc::Rc,
};
use type_uuid::{TypeUuid, TypeUuidDynamic};

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone, Debug)]
pub enum Timeout {
    Millis(u64),
    Never,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum TimeoutAbsolute {
    Millis(u128),
    Never,
}

pub fn serialize_rc_bytes<S>(data: &Rc<[u8]>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let vec: Vec<u8> = data.deref().to_vec();
    vec.serialize(serializer)
}

pub fn deserialize_rc_bytes<'de, D>(deserializer: D) -> Result<Rc<[u8]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let vec: Vec<u8> = Deserialize::deserialize(deserializer)?;
    Ok(Rc::from(vec.into_boxed_slice()))
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
#[derive(Serialize, Deserialize, Debug)]
pub enum ActionKind {
    Pure,
    Input,
    Output,
}

pub trait Action
where
    Self: TypeUuidDynamic + fmt::Debug + 'static,
{
    fn kind(&self) -> ActionKind;
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct ActionDebugInfo {
    pub location_file: String,
    pub location_line: u32,
    pub depth: usize,
    pub action_id: u64,
    // action id of caller action
    pub caller: u64,
}

pub struct AnyAction {
    pub uuid: type_uuid::Bytes,
    pub kind: ActionKind,
    pub ptr: Box<dyn Any>,
    // For printing/debug purpose only
    pub type_name: &'static str,
    pub dbginfo: ActionDebugInfo,
}

impl<T: Action> From<T> for AnyAction {
    fn from(v: T) -> Self {
        Self {
            uuid: v.uuid(),
            kind: v.kind(),
            ptr: Box::new(v),
            type_name: std::any::type_name::<T>(),
            dbginfo: ActionDebugInfo {
                location_file: String::new(),
                location_line: 0,
                depth: 0,
                action_id: 0,
                caller: 0,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SerializableAction<T: Clone + type_uuid::TypeUuid + std::fmt::Debug + Sized + 'static> {
    pub action: T,
    pub dbginfo: ActionDebugInfo,
}

// This is a special action used for replaying purposes.
// When serializing a ResultDispatch field we lose the function pointer used to
// produce the resulting InputAction, so when deserializing we will build a
// dummy function that returns `SerializedResultDispatch` action.
// When the callback is dispatched the process function will deserialize the
// correct action from the replay file.
#[derive(Clone, PartialEq, Eq, TypeUuid, Serialize, Deserialize, Debug)]
#[uuid = "47a5f477-8715-4968-8823-4334114cd252"]
pub struct SerializedResultDispatch();

impl Action for SerializedResultDispatch {
    fn kind(&self) -> ActionKind {
        ActionKind::Input
    }
}

#[derive(PartialEq, Eq, Clone)]
pub struct ResultDispatch<R: Clone>(fn(R) -> AnyAction);

impl<'de, R: Clone + Deserialize<'de>> Deserialize<'de> for ResultDispatch<R> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Deserialize::deserialize(deserializer)?; // deserialize ()
        Ok(Self(|_| SerializedResultDispatch().into()))
    }
}

impl<R: Clone + Serialize> Serialize for ResultDispatch<R> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_unit()
    }
}

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

    // Record/Replay
    pub record_file: Option<BufWriter<File>>,
    pub replay_file: Option<BufReader<File>>,
}

impl Dispatcher {
    pub fn new(tick: fn() -> AnyAction) -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
            tick,
            depth: 0,
            action_id: 0,
            caller: 0,
            record_file: None,
            replay_file: None,
        }
    }

    pub fn next_action(&mut self) -> AnyAction {
        self.queue.pop_front().unwrap_or_else(|| {
            let mut any_action = (self.tick)();

            any_action.dbginfo.action_id = self.action_id;
            any_action.dbginfo.caller = 0;
            self.depth = 0;
            self.action_id += 1;
            self.caller = 0;
            any_action
        })
    }

    pub fn record(&mut self, filename: &str) {
        assert!(self.record_file.is_none());
        self.record_file = Some(BufWriter::new(
            OpenOptions::new()
                .create(true)
                .write(true)
                .append(false)
                .open(filename)
                .expect(&format!("Recorder: failed to open file: {}", filename)),
        ));
    }

    pub fn open_recording(&mut self, filename: &str) {
        assert!(self.replay_file.is_none());
        self.replay_file = Some(BufReader::new(
            File::open(filename).expect(&format!("Replayer: failed to open file: {}", filename)),
        ));
    }

    pub fn is_replayer(&self) -> bool {
        self.replay_file.is_some()
    }

    #[track_caller]
    pub fn dispatch<A: Action>(&mut self, action: A)
    where
        A: Sized + 'static,
    {
        let location = Location::caller();
        assert_ne!(TypeId::of::<A>(), TypeId::of::<AnyAction>());
        let mut any_action: AnyAction = action.into();
        assert!(matches!(
            any_action.kind,
            ActionKind::Pure | ActionKind::Output
        ));

        any_action.dbginfo = ActionDebugInfo {
            location_file: location.file().to_string(),
            location_line: location.line(),
            depth: self.depth + 1,
            action_id: self.action_id,
            caller: self.caller,
        };
        self.action_id += 1;
        self.queue.push_back(any_action);
    }

    #[track_caller]
    pub fn dispatch_back<R: Clone>(&mut self, on_result: &ResultDispatch<R>, result: R)
    where
        R: Sized + 'static,
    {
        let location = Location::caller();
        let mut any_action = if self.is_replayer() {
            SerializedResultDispatch().into()
        } else {
            on_result.make(result)
        };

        assert!(matches!(any_action.kind, ActionKind::Input));

        any_action.dbginfo = ActionDebugInfo {
            location_file: location.file().to_string(),
            location_line: location.line(),
            depth: self.depth.saturating_sub(1),
            action_id: self.action_id,
            caller: self.caller,
        };
        self.action_id += 1;
        self.queue.push_back(any_action);
    }
}
