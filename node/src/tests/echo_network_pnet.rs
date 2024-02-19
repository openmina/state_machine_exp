use crate::{
    automaton::{
        action::Timeout,
        runner::{RegisterModel, RunnerBuilder},
        state::ModelState,
    },
    models::pure::{
        net::{
            pnet::{
                client::state::{PnetClientConfig, PnetClientState},
                common::PnetKey,
                server::state::{PnetServerConfig, PnetServerState},
            },
            tcp::state::TcpState,
            tcp_client::state::TcpClientState,
            tcp_server::state::TcpServerState,
        },
        prng::state::{PRNGConfig, PRNGState},
        tests::{
            echo_client::state::EchoClientConfig, echo_client_pnet::{action::PnetEchoClientAction, state::PnetEchoClientState}, echo_server::state::EchoServerConfig, echo_server_pnet::{action::PnetEchoServerAction, state::PnetEchoServerState}
        },
        time::state::TimeState,
    },
};
use model_state_derive::ModelState;
use std::any::Any;

#[derive(ModelState, Debug)]
pub struct PnetEchoServer {
    pub prng: PRNGState,
    pub time: TimeState,
    pub tcp: TcpState,
    pub tcp_server: TcpServerState,
    pub pnet_server: PnetServerState,
    pub echo_server: PnetEchoServerState,
}
pub struct PnetEchoServerConfig {
    echo_server: EchoServerConfig,
    pnet: PnetServerConfig,
}

impl PnetEchoServer {
    pub fn from_config(config: PnetEchoServerConfig) -> Self {
        Self {
            prng: PRNGState::from_config(PRNGConfig { seed: 31337 }),
            time: TimeState::default(),
            tcp: TcpState::new(),
            tcp_server: TcpServerState::new(),
            pnet_server: PnetServerState::from_config(config.pnet),
            echo_server: PnetEchoServerState::from_config(config.echo_server),
        }
    }
}

#[derive(ModelState, Debug)]
pub struct PnetEchoClient {
    pub prng: PRNGState,
    pub time: TimeState,
    pub tcp: TcpState,
    pub tcp_client: TcpClientState,
    pub pnet_client: PnetClientState,
    pub echo_client: PnetEchoClientState,
}

pub struct PnetEchoClientConfig {
    echo_client: EchoClientConfig,
    pnet: PnetClientConfig,
}

impl PnetEchoClient {
    pub fn from_config(config: PnetEchoClientConfig) -> Self {
        Self {
            prng: PRNGState::from_config(PRNGConfig { seed: 1337 }),
            time: TimeState::default(),
            tcp: TcpState::new(),
            tcp_client: TcpClientState::new(),
            pnet_client: PnetClientState::from_config(config.pnet),
            echo_client: PnetEchoClientState::from_config(config.echo_client),
        }
    }
}

#[derive(ModelState, Debug)]
pub enum EchoNetwork {
    PnetEchoServer(PnetEchoServer),
    PnetEchoClient(PnetEchoClient),
}

impl RegisterModel for EchoNetwork {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder
            .register::<PnetEchoClientState>()
            .register::<PnetEchoServerState>()
    }
}

#[test]
fn echo_server_1_client() {
    RunnerBuilder::<EchoNetwork>::new()
        .register::<EchoNetwork>()
        .instance(
            EchoNetwork::PnetEchoServer(PnetEchoServer::from_config(PnetEchoServerConfig {
                echo_server: EchoServerConfig {
                    address: "127.0.0.1:8888".to_string(),
                    max_connections: 1,
                    poll_timeout: 100,
                    recv_timeout: 500,
                },
                pnet: PnetServerConfig {
                    pnet_key: PnetKey::new("test"),
                    send_nonce_timeout: Timeout::Millis(500),
                    recv_nonce_timeout: Timeout::Millis(500),
                },
            })),
            || PnetEchoServerAction::Tick.into(),
        )
        .instance(
            EchoNetwork::PnetEchoClient(PnetEchoClient::from_config(PnetEchoClientConfig {
                echo_client: EchoClientConfig {
                    connect_to_address: "127.0.0.1:8888".to_string(),
                    connect_timeout: Timeout::Millis(1000),
                    poll_timeout: 100,
                    max_connection_attempts: 10,
                    retry_interval_ms: 500,
                    max_send_size: 10240,
                    min_rnd_timeout: 1000,
                    max_rnd_timeout: 10000,
                },
                pnet: PnetClientConfig {
                    pnet_key: PnetKey::new("test"),
                    send_nonce_timeout: Timeout::Millis(500),
                    recv_nonce_timeout: Timeout::Millis(500),
                },
            })),
            || PnetEchoClientAction::Tick.into(),
        )
        .build()
        .run()
}

fn echo_server_n_clients(n_clients: u64) {
    let mut builder = RunnerBuilder::<EchoNetwork>::new()
        .register::<EchoNetwork>()
        .instance(
            EchoNetwork::PnetEchoServer(PnetEchoServer::from_config(PnetEchoServerConfig {
                echo_server: EchoServerConfig {
                    address: "127.0.0.1:8888".to_string(),
                    max_connections: n_clients as usize,
                    poll_timeout: 100 / n_clients,
                    recv_timeout: 500 * n_clients,
                },
                pnet: PnetServerConfig {
                    pnet_key: PnetKey::new("test"),
                    send_nonce_timeout: Timeout::Millis(500 * n_clients),
                    recv_nonce_timeout: Timeout::Millis(500 * n_clients),
                },
            })),
            || PnetEchoServerAction::Tick.into(),
        );

    for _ in 0..n_clients {
        builder = builder.instance(
            EchoNetwork::PnetEchoClient(PnetEchoClient::from_config(PnetEchoClientConfig {
                echo_client: EchoClientConfig {
                    connect_to_address: "127.0.0.1:8888".to_string(),
                    connect_timeout: Timeout::Millis(1000 * n_clients),
                    poll_timeout: 100 / n_clients,
                    max_connection_attempts: 10,
                    retry_interval_ms: 5000,
                    max_send_size: 1024 / n_clients,
                    min_rnd_timeout: 1000,
                    max_rnd_timeout: 1000 * n_clients,
                },
                pnet: PnetClientConfig {
                    pnet_key: PnetKey::new("test"),
                    send_nonce_timeout: Timeout::Millis(500 * n_clients),
                    recv_nonce_timeout: Timeout::Millis(500 * n_clients),
                },
            })),
            || PnetEchoClientAction::Tick.into(),
        );
    }

    builder.build().run()
}

#[test]
fn echo_server_5_clients() {
    echo_server_n_clients(5)
}

#[test]
fn echo_server_50_clients() {
    // WARNING: this test probably needs an increase in the fd limit (ulimit -n 10000)
    echo_server_n_clients(50)
}
