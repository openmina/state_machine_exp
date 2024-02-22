use crate::automaton::{action::Timeout, state::Uid};

#[derive(Debug)]
pub struct PnetSimpleClientConfig {
    pub connect_to_address: String,
    pub connect_timeout: Timeout,
    pub poll_timeout: u64,
    pub max_connection_attempts: usize,
    pub retry_interval_ms: u64,
    pub send_data: Vec<u8>,
    pub recv_data: Vec<u8>,
    pub recv_timeout: Timeout,
}

#[derive(Debug)]
pub enum ClientStatus {
    Init,
    Connecting,
    Connected { connection: Uid },
    Sending { connection: Uid, request: Uid },
    Receiving { connection: Uid, request: Uid },
    TestCompleted
}

#[derive(Debug)]
pub struct PnetSimpleClientState {
    pub status: ClientStatus,
    pub connection_attempt: usize,
    pub config: PnetSimpleClientConfig,
}

impl PnetSimpleClientState {
    pub fn from_config(config: PnetSimpleClientConfig) -> Self {
        Self {
            status: ClientStatus::Init,
            connection_attempt: 0,
            config,
        }
    }
}
