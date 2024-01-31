use crate::automaton::action::Timeout;
use crate::automaton::runner::{RegisterModel, RunnerBuilder};
use crate::automaton::state::ModelState;
use crate::models::pure::net::pnet::client::state::PnetClientConfig;
use crate::models::pure::net::pnet::common::PnetKey;
use crate::models::pure::prng::state::{PRNGConfig, PRNGState};
use crate::models::pure::tests::simple_client_pnet::action::SimpleClientTickAction;
use crate::models::pure::{
    net::{
        pnet::client::state::PnetClientState, tcp::state::TcpState,
        tcp_client::state::TcpClientState,
    },
    tests::simple_client_pnet::state::{SimpleClientConfig, SimpleClientState},
    time::state::TimeState,
};
use model_state_derive::ModelState;
use std::any::Any;

#[derive(ModelState, Debug)]
pub struct PnetClient {
    pub prng: PRNGState,
    pub time: TimeState,
    pub tcp: TcpState,
    pub tcp_client: TcpClientState,
    pub pnet_client: PnetClientState,
    pub client: SimpleClientState,
}

pub struct ClientConfig {
    client: SimpleClientConfig,
    pnet: PnetClientConfig,
}

impl PnetClient {
    pub fn from_config(config: ClientConfig) -> Self {
        Self {
            prng: PRNGState::from_config(PRNGConfig { seed: 31337 }),
            time: TimeState::default(),
            tcp: TcpState::new(),
            tcp_client: TcpClientState::new(),
            pnet_client: PnetClientState::from_config(config.pnet),
            client: SimpleClientState::from_config(config.client),
        }
    }
}

impl RegisterModel for PnetClient {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.register::<SimpleClientState>()
    }
}

#[test]
fn connect() {
    RunnerBuilder::<PnetClient>::new()
        .register::<PnetClient>()
        .instance(
            PnetClient::from_config(ClientConfig {
                client: SimpleClientConfig {
                    connect_to_address: "65.109.110.75:18302".to_string(),
                    connect_timeout: Timeout::Millis(2000),
                    poll_timeout: 1000,
                    max_connection_attempts: 10,
                    retry_interval_ms: 500,
                    recv_size: 20,
                    send_data: b"\x13/multistream/1.0.0\n".to_vec(),
                },
                pnet: PnetClientConfig {
                    pnet_key: PnetKey::new(
                        "3c41383994b87449625df91769dff7b507825c064287d30fada9286f3f1cb15e",
                    ),
                    send_nonce_timeout: Timeout::Millis(2000),
                    recv_nonce_timeout: Timeout::Millis(2000),
                },
            }),
            || SimpleClientTickAction().into(),
        )
        .build()
        .run()
}
