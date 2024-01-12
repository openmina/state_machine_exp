use super::{
    action::{ActionKind, AnyAction, Dispatcher, SerializedResultDispatch},
    model::{AnyModel, Input, InputModel, Output, OutputModel, PrivateModel, Pure, PureModel},
    state::{ModelState, State},
};
use bincode::deserialize_from;
use std::collections::BTreeMap;
use std::{env, io::Write};
use type_uuid::TypeUuid;

// This struct holds the registered models, the state-machine state, and one
// or more dispatchers. Usually, we need only one `Dispatcher`, except for
// testing scenarios where we want to run several "instances". For example,
// if our state-machine implements a node, we might want to simulate a network
// running multiple nodes interacting with each other, all this inside the same
// state-machine.
pub struct Runner<Substate: ModelState> {
    models: BTreeMap<type_uuid::Bytes, AnyModel<Substate>>,
    state: State<Substate>,
    dispatchers: Vec<Dispatcher>,
}

// Models should implement their own `register` function to register themselves
// along with their dependencies (other models).
pub trait RegisterModel {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate>;
}

// We use the builder pattern to register the state-machine models and to
// establish one or more state/dispatcher instances.
// This allows us to dynamically construct state-machine configurations at the
// time of creating the Runner instance. Models remain immutable thereafter.
pub struct RunnerBuilder<Substate: ModelState> {
    models: BTreeMap<type_uuid::Bytes, AnyModel<Substate>>,
    state: State<Substate>,
    dispatchers: Vec<Dispatcher>,
}

impl<Substate: ModelState> RunnerBuilder<Substate> {
    pub fn new() -> Self {
        Self {
            models: BTreeMap::default(),
            state: State::<Substate>::new(),
            dispatchers: Vec::new(),
        }
    }

    // Usually called once, except for testing scenarios describied earlier.
    pub fn instance(mut self, substate: Substate, tick: fn() -> AnyAction) -> Self {
        self.state.substates.push(substate);
        self.dispatchers.push(Dispatcher::new(tick));
        self
    }

    // Should be called once with the top-most model. The top-most model's
    // `RegisterModel` trait should handle dependencies.
    pub fn register<T: RegisterModel>(self) -> Self {
        T::register(self)
    }

    // The following methods should be called by `RegisterModel`
    // implementations only.

    pub fn model_pure<M: PureModel>(mut self) -> Self {
        self.models
            .insert(M::Action::UUID, Pure::<M>::into_vtable2());
        self
    }

    pub fn model_pure_and_input<M: PureModel + InputModel>(mut self) -> Self {
        self.models
            .insert(<M as PureModel>::Action::UUID, Pure::<M>::into_vtable2());
        self.models
            .insert(<M as InputModel>::Action::UUID, Input::<M>::into_vtable2());
        self
    }

    pub fn model_output<M: OutputModel>(mut self, model: Output<M>) -> Self {
        self.models
            .insert(M::Action::UUID, Box::new(model).into_vtable());
        self
    }

    // Called once to construct the `Runner`.
    pub fn build(self) -> Runner<Substate> {
        Runner::new(self.state, self.models, self.dispatchers)
    }
}

impl<Substate: ModelState> Runner<Substate> {
    pub fn new(
        state: State<Substate>,
        models: BTreeMap<type_uuid::Bytes, AnyModel<Substate>>,
        dispatchers: Vec<Dispatcher>,
    ) -> Self {
        Self {
            models,
            state,
            dispatchers,
        }
    }

    // State-machine main loop. If the runner contains more than one instance,
    // it interleaves the processing of actions fairly for each instance.
    pub fn run(&mut self) {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
            .init();

        loop {
            for instance in 0..self.dispatchers.len() {
                self.state.set_current_instance(instance);

                let action = self.dispatchers[instance].next_action();
                self.process_action(action, instance)
            }
        }
    }

    fn process_action(&mut self, mut action: AnyAction, instance: usize) {
        let dispatcher = &mut self.dispatchers[instance];

        // Replayer: this is a special case where we handle actions coming from
        // calls to a dummy function used in ResultDispatch. In this case we
        // get the actual action's UUID from the replay file to find the right
        // model.
        if action.uuid == SerializedResultDispatch::UUID {
            let reader = dispatcher
                .replay_file
                .as_mut()
                .expect("SerializedResultDispatch UUID but not in replay mode");

            let uuid: type_uuid::Bytes =
                deserialize_from(reader).expect("UUID deserialization failed");

            println!("deserializing callback {:?}", uuid);
            action.uuid = uuid;
        }

        let model = self
            .models
            .get_mut(&action.uuid)
            .expect(&format!("action not found {}", action.type_name));

        // Replayer
        if let Some(reader) = &mut dispatcher.replay_file {
            let deserialized_action = model.deserialize_from(reader);

            match action.kind {
                ActionKind::Input => {
                    // We replay *all* Input actions because we can't generate
                    // any input actions deterministicaly. The reason is that
                    // the function pointer in ResultDispatch fields is lost
                    // during serialization.
                    action = deserialized_action;
                }
                ActionKind::Pure | ActionKind::Output => {
                    // For debugging purposes we check that the deserialized
                    // action debugging information matches the one that was
                    // generated deterministically.
                    if action.dbginfo != deserialized_action.dbginfo {
                        panic!(
                            "Deserialized debug info mismatch:\naction:{:?}\ndeserialized:{:?}",
                            action.dbginfo, deserialized_action.dbginfo
                        )
                    }
                }
            }
        }

        // Recorder: no need to record Pure/Output actions, but for the moment
        // we record them to ensure that the state-machine works properly.
        if let Some(writer) = &mut dispatcher.record_file {
            model.serialize_into(writer, &action)
        }

        match action.kind {
            ActionKind::Pure => model.process_pure(&mut self.state, action, dispatcher),
            ActionKind::Input => model.process_input(&mut self.state, action, dispatcher),
            ActionKind::Output => model.process_output(action, dispatcher),
        }
    }

    // Run the state-machine main loop and record actions
    pub fn record(&mut self, session_name: &str) {
        let path = env::current_dir().expect("Failed to retrieve current directory");

        for (instance, dispatcher) in self.dispatchers.iter_mut().enumerate() {
            dispatcher.record(&format!(
                "{}/{}_{}.rec",
                path.to_str().unwrap(),
                session_name,
                instance
            ))
        }

        self.run()
    }

    // Replay deterministically from a session's recording files
    pub fn replay(&mut self, session_name: &str) {
        let path = env::current_dir().expect("Failed to retrieve current directory");

        for (instance, dispatcher) in self.dispatchers.iter_mut().enumerate() {
            dispatcher.open_recording(&format!(
                "{}/{}_{}.rec",
                path.to_str().unwrap(),
                session_name,
                instance
            ))
        }

        self.run()
    }
}
