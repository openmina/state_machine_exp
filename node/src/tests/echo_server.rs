use std::any::Any;

use crate::{
    automaton::{
        action::{AnyAction, Dispatcher},
        model::Output,
        runner::RunnerBuilder,
        state::{ModelState, State},
    },
    models::{
        effectful::{self, mio::state::MioState},
        pure::{
            tcp::state::TcpState,
            tcp_server::state::TcpServerState,
            tests::echo_server::{action::EchoServerPureAction, state::EchoServerState},
            time::state::TimeState,
        },
    },
};

// Substates of our state-machine state. Includes all pure and input models involved.
pub struct Substates {
    pub time: TimeState,
    pub tcp: TcpState,
    pub tcp_server: TcpServerState,
    pub echo_server: EchoServerState,
}

impl Substates {
    pub fn new() -> Self {
        Self {
            time: TimeState::default(),
            tcp: TcpState::new(),
            tcp_server: TcpServerState::new(),
            echo_server: EchoServerState::new(),
        }
    }
}

impl ModelState for Substates {
    fn state<T: 'static + Any>(&self) -> &T {
        <dyn Any>::downcast_ref::<T>(&self.time).unwrap_or_else(|| {
            <dyn Any>::downcast_ref::<T>(&self.tcp).unwrap_or_else(|| {
                <dyn Any>::downcast_ref::<T>(&self.tcp_server).unwrap_or_else(|| {
                    <dyn Any>::downcast_ref::<T>(&self.echo_server).expect("Unsupported type")
                })
            })
        })
    }

    fn state_mut<T: 'static + Any>(&mut self) -> &mut T {
        <dyn Any>::downcast_mut::<T>(&mut self.time).unwrap_or_else(|| {
            <dyn Any>::downcast_mut::<T>(&mut self.tcp).unwrap_or_else(|| {
                <dyn Any>::downcast_mut::<T>(&mut self.tcp_server).unwrap_or_else(|| {
                    <dyn Any>::downcast_mut::<T>(&mut self.echo_server).expect("Unsupported type")
                })
            })
        })
    }
}

#[test]
fn echo_server() {
    let mut runner = RunnerBuilder::<Substates>::new()
        .model_output(Output::<MioState>(MioState::new()))
        .model_output(Output::<effectful::time::state::TimeState>(
            effectful::time::state::TimeState(),
        ))
        .model_pure_and_input::<TimeState>()
        .model_pure_and_input::<TcpState>()
        .model_pure_and_input::<TcpServerState>()
        .model_pure_and_input::<EchoServerState>()
        .state(State::<Substates>::from_substates(Substates::new()))
        .build();
    let dispatcher = Dispatcher::new(|| AnyAction::from(EchoServerPureAction::Tick));

    runner.run(dispatcher)
}
