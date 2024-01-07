use std::time::Duration;

use crate::{
    automaton::{
        runner::{RegisterModel, RunnerBuilder},
        state::ModelState,
    },
    models::effectful,
};

#[derive(Default)]
pub struct TimeState {
    now: Duration,
    tick: bool,
}

impl TimeState {
    pub fn now(&self) -> &Duration {
        &self.now
    }

    pub fn set_time(&mut self, time: Duration) {
        self.now = time;
    }

    pub fn tick(&mut self) -> bool {
        self.tick = !self.tick;
        self.tick
    }
}

impl RegisterModel for TimeState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<effectful::time::state::TimeState>()
            .model_pure_and_input::<Self>()
    }
}
