use crate::automaton::{action::Timeout, state::Uid};

#[derive(Debug)]
pub struct SimpleClientConfig {
    pub connect_to_address: String,
    pub connect_timeout: Timeout,
    pub poll_timeout: u64,
    pub max_connection_attempts: usize,
    pub retry_interval_ms: u64,
    pub send_data: Vec<u8>,
    pub recv_size: usize,
}


#[derive(Debug)]
pub struct SimpleClientState {
    pub ready: bool,
    pub connection: Option<Uid>,
    pub send_request: Option<Uid>,
    pub recv_request: Option<Uid>,
    pub connection_attempt: usize,
    pub config: SimpleClientConfig,
}

impl SimpleClientState {
    pub fn from_config(config: SimpleClientConfig) -> Self {
        Self {
            ready: false,
            connection: None,
            send_request: None,
            recv_request: None,
            connection_attempt: 0,
            config,
        }
    }
}
