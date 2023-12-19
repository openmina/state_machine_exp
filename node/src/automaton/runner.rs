use log::LevelFilter;
use std::io::Write;

use std::{any::TypeId, collections::BTreeMap};

use super::{
    action::{ActionKind, AnyAction, Dispatcher},
    model::{AnyModel, Input, InputModel, Output, OutputModel, PrivateModel, Pure, PureModel},
    state::{ModelState, State},
};

pub struct Runner<Substate: ModelState> {
    models: BTreeMap<TypeId, AnyModel<Substate>>,
    state: State<Substate>,
}

pub struct RunnerBuilder<Substate: ModelState> {
    models: BTreeMap<TypeId, AnyModel<Substate>>,
    state: Option<State<Substate>>,
}

impl<Substate: ModelState> RunnerBuilder<Substate> {
    pub fn new() -> Self {
        Self {
            models: BTreeMap::default(),
            state: None,
        }
    }

    pub fn state(mut self, state: State<Substate>) -> Self {
        self.state = Some(state);
        self
    }

    pub fn model_pure<M: PureModel>(mut self) -> Self {
        self.models
            .insert(TypeId::of::<M::Action>(), Pure::<M>::into_vtable2());
        self
    }

    pub fn model_input<M: InputModel>(mut self) -> Self {
        self.models
            .insert(TypeId::of::<M::Action>(), Input::<M>::into_vtable2());
        self
    }

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

    pub fn build(self) -> Runner<Substate> {
        let Some(state) = self.state else {
            panic!("Runner state missing")
        };
        Runner::new(state, self.models)
    }
}

impl<Substate: ModelState> Runner<Substate> {
    pub fn new(state: State<Substate>, models: BTreeMap<TypeId, AnyModel<Substate>>) -> Self {
        Self { models, state }
    }

    pub fn run(&mut self, mut dispatcher: Dispatcher) {
        env_logger::Builder::new()
            .format(|buf, record| writeln!(buf, "[{}] {}", record.level(), record.args()))
            .filter(None, LevelFilter::Debug)
            .init();

        loop {
            let action = dispatcher.next_action();
            self.process_action(action, &mut dispatcher)
        }
    }

    fn process_action(&mut self, action: AnyAction, dispatcher: &mut Dispatcher) {
        assert_ne!(action.id, TypeId::of::<AnyAction>());

        let Some(model) = self.models.get_mut(&action.id) else {
            panic!("action not found1 {}", action.type_name);
        };

        match action.kind {
            ActionKind::Pure => model.process_pure(&mut self.state, action, dispatcher),
            ActionKind::Input => model.process_input(&mut self.state, action, dispatcher),
            ActionKind::Output => model.process_output(action, dispatcher),
        }
    }
}
