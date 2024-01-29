use crate::automaton::state::{Objects, Uid};
use core::panic;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Connection {
    pub recv_uid: Option<Uid>,
}

#[derive(Debug)]
pub struct EchoServerConfig {
    pub address: String,
    pub max_connections: usize,
    pub poll_timeout: u64,
    pub recv_timeout: u64,
}

#[derive(Debug)]
pub struct PnetEchoServerState {
    pub ready: bool,
    pub connections: Objects<Connection>,
    pub config: EchoServerConfig,
}

impl PnetEchoServerState {
    pub fn from_config(config: EchoServerConfig) -> Self {
        Self {
            ready: false,
            connections: BTreeMap::new(),
            config,
        }
    }

    pub fn new_connection(&mut self, connection_uid: Uid) {
        if self
            .connections
            .insert(connection_uid, Connection { recv_uid: None })
            .is_some()
        {
            panic!("Attempt to re-use existing {:?}", connection_uid)
        }
    }

    pub fn remove_connection(&mut self, uid: &Uid) {
        self.connections.remove(uid).expect(&format!(
            "Attempt to remove an inexistent Connection {:?}",
            uid
        ));
    }

    pub fn get_connection_mut(&mut self, connection_uid: &Uid) -> &mut Connection {
        self.connections.get_mut(&connection_uid).expect(&format!(
            "Connection object not found for {:?}",
            connection_uid
        ))
    }

    pub fn connections_to_recv(&self) -> Vec<Uid> {
        self.connections
            .iter()
            .filter_map(|(&uid, conn)| match conn.recv_uid {
                Some(_) => None,
                None => Some(uid),
            })
            .collect()
    }

    pub fn find_connection_by_recv_uid(&mut self, recv_uid: Uid) -> (&Uid, &mut Connection) {
        self.connections
            .iter_mut()
            .find(|(_, conn)| match conn.recv_uid {
                Some(uid) => uid == recv_uid,
                None => false,
            })
            .expect(&format!(
                "Connection object not found for recv {:?}",
                recv_uid
            ))
    }
}
