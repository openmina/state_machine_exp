use std::io::Write;

use std::{any::TypeId, collections::BTreeMap};

use super::{
    action::{ActionKind, AnyAction, Dispatcher},
    model::{AnyModel, Input, InputModel, Output, OutputModel, PrivateModel, Pure, PureModel},
    state::{ModelState, State},
};

// This struct holds the registered models, the state-machine state, and one
// or more dispatchers. Usually, we need only one `Dispatcher`, except for
// testing scenarios where we want to run several "instances". For example,
// if our state-machine implements a node, we might want to simulate a network
// running multiple nodes interacting with each other, all this inside the same
// state-machine.
pub struct Runner<Substate: ModelState> {
    models: BTreeMap<TypeId, AnyModel<Substate>>,
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
    models: BTreeMap<TypeId, AnyModel<Substate>>,
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
            .insert(TypeId::of::<M::Action>(), Pure::<M>::into_vtable2());
        self
    }

    // pub fn model_input<M: InputModel>(mut self) -> Self {
    //     self.models
    //         .insert(TypeId::of::<M::Action>(), Input::<M>::into_vtable2());
    //     self
    // }

    pub fn model_pure_and_input<M: PureModel + InputModel>(mut self) -> Self {
        self.models.insert(
            TypeId::of::<<M as PureModel>::Action>(),
            Pure::<M>::into_vtable2(),
        );
        self.models.insert(
            TypeId::of::<<M as InputModel>::Action>(),
            Input::<M>::into_vtable2(),
        );
        self
    }

    pub fn model_output<M: OutputModel>(mut self, model: Output<M>) -> Self {
        self.models
            .insert(TypeId::of::<M::Action>(), Box::new(model).into_vtable());
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
        models: BTreeMap<TypeId, AnyModel<Substate>>,
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

    fn process_action(&mut self, action: AnyAction, instance: usize) {
        assert_ne!(action.id, TypeId::of::<AnyAction>());

        let Some(model) = self.models.get_mut(&action.id) else {
            panic!("action not found1 {}", action.type_name);
        };

        let dispatcher = &mut self.dispatchers[instance];

        match action.kind {
            ActionKind::Pure => model.process_pure(&mut self.state, action, dispatcher),
            ActionKind::Input => model.process_input(&mut self.state, action, dispatcher),
            ActionKind::Output => model.process_output(action, dispatcher),
        }
    }
}
