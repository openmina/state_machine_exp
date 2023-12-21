use core::panic;
use std::collections::BTreeMap;

use crate::automaton::state::{Objects, Uid};

pub enum ServerStatus {
    Uninitialized,
    Init,
    Ready,
}

pub struct Connection {
    pub recv_uid: Option<Uid>,
}

pub struct EchoServerState {
    pub tock: bool,
    pub status: ServerStatus,
    pub connections: Objects<Connection>,
}

impl EchoServerState {
    pub fn new() -> Self {
        Self {
            tock: false,
            status: ServerStatus::Uninitialized,
            connections: BTreeMap::new(),
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
        self.connections.remove(uid).unwrap_or_else(|| {
            panic!("Attempt to remove an inexistent Connection {:?}", uid)
        });
    }

    pub fn get_connection_mut(&mut self, connection_uid: &Uid) -> &mut Connection {
        let Some(connection) = self.connections.get_mut(&connection_uid) else {
            panic!("Connection object not found for {:?}", connection_uid)
        };
        connection
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
        let Some(connection) = self
            .connections
            .iter_mut()
            .find(|(_, conn)| match conn.recv_uid {
                Some(uid) => uid == recv_uid,
                None => false,
            })
        else {
            panic!("Connection object not found for recv {:?}", recv_uid)
        };

        connection
    }
}
