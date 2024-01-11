use std::any::Any;

use bincode::{deserialize_from, serialize_into};
use colored::Colorize;
use log::debug;
use serde::{Deserialize, Serialize};

use crate::automaton::action::{ActionDebugInfo, SerializableAction};

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
    type Action: Clone
        + Eq
        + Serialize
        + for<'a> Deserialize<'a>
        + type_uuid::TypeUuid
        + std::fmt::Debug
        + Sized
        + 'static;

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
        let downcasted_action = action
            .ptr
            .downcast::<T::Action>()
            .expect("action not found");

        // Action replay: we don't need to replay pure actions, but we compare
        // them to the pure actions that were deterministically dispatched by
        // the state-machine to ensure they always match.
        if let Some(reader) = &mut dispatcher.replay_file {
            let uuid: type_uuid::Bytes =
                deserialize_from(reader).expect("UUID deserialization failed");

            println!("deserializing {:?}", uuid);
            assert_eq!(uuid, action.uuid);

            let deserialized_action: SerializableAction<T::Action> =
                deserialize_from(dispatcher.replay_file.as_mut().unwrap())
                    .expect("Action deserialization failed");

            println!("{:?}", deserialized_action);

            if action.dbginfo != deserialized_action.dbginfo {
                panic!(
                    "Deserialized debug info mismatch:\naction:{:?}\ndeserialized:{:?}",
                    action.dbginfo, deserialized_action.dbginfo
                )
            }

            if *downcasted_action.clone() != deserialized_action.action {
                panic!(
                    "Deserialized action mismatch:\naction:{:?}\ndeserialized:{:?}",
                    *downcasted_action, deserialized_action
                )
            }
        }

        // Action recording: we shouldn't need to record pure actions but we
        // record them to ensure the correct functioning of the state-machine.
        if let Some(writer) = &mut dispatcher.record_file {
            serialize_into(writer, &action.uuid).expect("UUID serialization failed");
            serialize_into(
                dispatcher.record_file.as_mut().unwrap(),
                &SerializableAction {
                    action: *downcasted_action.clone(),
                    dbginfo: action.dbginfo.clone(),
                },
            )
            .expect("Action serialization failed");
        }

        let ActionDebugInfo {
            location_file,
            location_line,
            depth,
            action_id,
            caller,
        } = action.dbginfo;

        let from = if depth == 0 {
            format!("from DISPATCHER TICK ({}→{})", caller, action_id)
        } else {
            format!(
                "from {}:{} ({}→{})",
                location_file, location_line, caller, action_id
            )
        };

        let pad = "  ".repeat(depth);
        debug!(
            "{}: →{} {}::{:?}\n\t\t {} {}",
            state.get_current_instance(),
            pad,
            action.type_name,
            downcasted_action,
            pad,
            from.bright_black()
        );

        dispatcher.depth = depth;
        dispatcher.caller = action_id;

        T::process_pure(state, *downcasted_action, dispatcher)
    }
}

pub trait InputModel
where
    Self: Sized + 'static,
{
    type Action: Clone
        + Eq
        + Serialize
        + for<'a> Deserialize<'a>
        + type_uuid::TypeUuid
        + std::fmt::Debug
        + Into<AnyAction>
        + Sized
        + 'static;

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
        // Action replay: we must rebuild the current action with the
        // information form the replay file. We can't generate input
        // actions deterministicaly because the ResultDispatch's function
        // pointer is lost during serialization.
        let action = dispatcher.replay_file.as_mut().map_or(action, |reader| {
            let deserialized_action: SerializableAction<T::Action> =
                deserialize_from(reader).expect("Action deserialization failed");

            println!("{:?}", deserialized_action);

            let mut action: AnyAction = deserialized_action.action.into();
            action.dbginfo = deserialized_action.dbginfo;
            action
        });

        let downcasted_action = action
            .ptr
            .downcast::<T::Action>()
            .expect("action not found");

        if let Some(writer) = &mut dispatcher.record_file {
            serialize_into(writer, &action.uuid).expect("UUID serialization failed");
            serialize_into(
                dispatcher.record_file.as_mut().unwrap(),
                &SerializableAction {
                    action: *downcasted_action.clone(),
                    dbginfo: action.dbginfo.clone(),
                },
            )
            .expect("Action serialization failed");
        }

        let ActionDebugInfo {
            location_file,
            location_line,
            depth,
            action_id,
            caller,
        } = action.dbginfo;

        let pad = "  ".repeat(depth);
        debug!(
            "{}: ←{} {}::{}\n\t\t {} {}",
            state.get_current_instance(),
            pad,
            action.type_name.bright_cyan(),
            format!("{:?}", downcasted_action).bright_cyan(),
            pad,
            format!(
                "from {}:{} ({}→{})",
                location_file, location_line, caller, action_id
            )
            .bright_black()
        );

        dispatcher.depth = depth;
        dispatcher.caller = action_id;
        T::process_input(state, *downcasted_action, dispatcher)
    }
}

pub trait OutputModel
where
    Self: Sized + 'static,
{
    type Action: Clone
        + Eq
        + Serialize
        + for<'a> Deserialize<'a>
        + type_uuid::TypeUuid
        + std::fmt::Debug
        + Sized
        + 'static;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher);
}

pub struct Output<T: OutputModel>(pub T);

impl<T: OutputModel> PrivateModel for Output<T> {
    fn process_output(state: &mut Box<dyn Any>, action: AnyAction, dispatcher: &mut Dispatcher) {
        let state = state
            .downcast_mut::<Self>()
            .expect("model's state not found");

        let downcasted_action = action
            .ptr
            .downcast::<T::Action>()
            .expect("action not found");

        // Action replay: we don't need to replay output actions, but we compare
        // them to the pure actions that were deterministically dispatched by
        // the state-machine to ensure they always match.
        if let Some(reader) = &mut dispatcher.replay_file {
            let uuid: type_uuid::Bytes =
                deserialize_from(reader).expect("UUID deserialization failed");

            println!("deserializing {:?}", uuid);
            assert_eq!(uuid, action.uuid);

            let deserialized_action: SerializableAction<T::Action> =
                deserialize_from(dispatcher.replay_file.as_mut().unwrap())
                    .expect("Action deserialization failed");

            println!("{:?}", deserialized_action);

            if action.dbginfo != deserialized_action.dbginfo {
                panic!(
                    "Deserialized debug info mismatch:\naction:{:?}\ndeserialized:{:?}",
                    action.dbginfo, deserialized_action.dbginfo
                )
            }

            if *downcasted_action.clone() != deserialized_action.action {
                panic!(
                    "Deserialized action mismatch:\naction:{:?}\ndeserialized:{:?}",
                    *downcasted_action, deserialized_action
                )
            }
        }

        // Action recording: we shouldn't need to record Output actions but we
        // record them to ensure the correct functioning of the state-machine.
        if let Some(writer) = &mut dispatcher.record_file {
            serialize_into(writer, &action.uuid).expect("UUID serialization failed");
            serialize_into(
                dispatcher.record_file.as_mut().unwrap(),
                &SerializableAction {
                    action: *downcasted_action.clone(),
                    dbginfo: action.dbginfo.clone(),
                },
            )
            .expect("Action serialization failed");
        }

        let ActionDebugInfo {
            location_file,
            location_line,
            depth,
            action_id,
            caller,
        } = action.dbginfo;

        let pad = "  ".repeat(depth);
        debug!(
            "→{} {}::{}\n\t\t {} {}",
            pad,
            action.type_name.bright_yellow(),
            format!("{:?}", downcasted_action).bright_yellow(),
            pad,
            format!(
                "from {}:{} ({}→{})",
                location_file, location_line, caller, action_id
            )
            .bright_black()
        );
        dispatcher.depth = depth;
        dispatcher.caller = action_id;
        state.0.process_output(*downcasted_action, dispatcher)
    }
}
