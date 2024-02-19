use crate::models::pure::tests::echo_client::state::{EchoClientConfig, EchoClientStatus};

#[derive(Debug)]
pub struct PnetEchoClientState {
    pub status: EchoClientStatus,
    pub connection_attempt: usize,
    pub config: EchoClientConfig,
}

impl PnetEchoClientState {
    pub fn from_config(config: EchoClientConfig) -> Self {
        Self {
            status: EchoClientStatus::Init,
            connection_attempt: 0,
            config,
        }
    }
}
