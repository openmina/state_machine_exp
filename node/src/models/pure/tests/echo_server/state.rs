use crate::automaton::state::Uid;

pub enum ServerStatus {
    Uninitialized,
    Init,
    Ready,
}

pub struct EchoServerState {
    pub tock: bool,
    pub status: ServerStatus,
    pub connections: Vec<Uid>,
}

impl EchoServerState {
    pub fn new() -> Self {
        Self {
            tock: false,
            status: ServerStatus::Uninitialized,
            connections: Vec::new(),
        }
    }
}
