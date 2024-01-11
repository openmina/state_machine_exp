use crate::automaton::{action::Timeout, state::Uid};
use std::rc::Rc;

#[derive(Debug)]
pub struct EchoClientConfig {
    pub connect_to_address: String,
    pub connect_timeout: Timeout,
    pub poll_timeout: u64,
    pub max_connection_attempts: usize,
    pub retry_interval_ms: u64,
    pub max_send_size: u64,
    pub min_rnd_timeout: u64,
    pub max_rnd_timeout: u64,
}

#[derive(Debug)]
pub struct SendRequest {
    pub uid: Uid,
    pub data: Rc<[u8]>,
}

#[derive(Debug)]
pub struct RecvRequest {
    pub uid: Uid,
    // this contains the data of a previous SendRequest,
    // when we receive data it should match the contents of `data`.
    pub data: Rc<[u8]>,
}

#[derive(Debug)]
pub struct EchoClientState {
    pub ready: bool,
    pub connection: Option<Uid>,
    pub send_request: Option<SendRequest>,
    pub recv_request: Option<RecvRequest>,
    pub connection_attempt: usize,
    pub config: EchoClientConfig,
}

impl EchoClientState {
    pub fn from_config(config: EchoClientConfig) -> Self {
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
