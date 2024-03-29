use crate::{
    automaton::{
        action::{Redispatch, Timeout},
        state::{Objects, Uid},
    },
    models::pure::net::{
        pnet::common::{ConnectionState, PnetKey},
        tcp_client::state::RecvRequest,
    },
};

#[derive(Debug)]
pub struct Connection {
    pub state: ConnectionState,
    pub on_success: Redispatch<Uid>,
    pub on_timeout: Redispatch<Uid>,
    pub on_error: Redispatch<(Uid, String)>,
    pub on_close: Redispatch<Uid>,
}

#[derive(Debug)]
pub struct PnetClientConfig {
    pub pnet_key: PnetKey,
    pub send_nonce_timeout: Timeout,
    pub recv_nonce_timeout: Timeout,
}

#[derive(Debug)]
pub struct PnetClientState {
    pub connections: Objects<Connection>,
    pub recv_requests: Objects<RecvRequest>,
    pub config: PnetClientConfig,
}

impl PnetClientState {
    pub fn from_config(config: PnetClientConfig) -> Self {
        Self {
            connections: Objects::<Connection>::new(),
            recv_requests: Objects::<RecvRequest>::new(),
            config,
        }
    }

    pub fn get_connection(&self, connection: &Uid) -> &Connection {
        self.connections
            .get(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn get_connection_mut(&mut self, connection: &Uid) -> &mut Connection {
        self.connections
            .get_mut(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn find_connection_by_nonce_request(&self, uid: &Uid) -> (&Uid, &Connection) {
        self.connections
            .iter()
            .find(|(_connection, Connection { state, .. })| match state {
                ConnectionState::Init => false,
                ConnectionState::NonceSent { send_request, .. } => send_request == uid,
                ConnectionState::NonceWait { recv_request, .. } => recv_request == uid,
                ConnectionState::Ready { .. } => false,
            })
            .expect(&format!(
                "No connection object with nonce request {:?}",
                uid
            ))
    }

    pub fn find_connection_mut_by_nonce_request(&mut self, uid: &Uid) -> (&Uid, &mut Connection) {
        self.connections
            .iter_mut()
            .find(|(_connection, Connection { state, .. })| match state {
                ConnectionState::Init => false,
                ConnectionState::NonceSent { send_request, .. } => send_request == uid,
                ConnectionState::NonceWait { recv_request, .. } => recv_request == uid,
                ConnectionState::Ready { .. } => false,
            })
            .expect(&format!(
                "No connection object with nonce request {:?}",
                uid
            ))
    }

    pub fn new_connection(
        &mut self,
        connection: Uid,
        on_success: Redispatch<Uid>,
        on_timeout: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
        on_close: Redispatch<Uid>,
    ) {
        if self
            .connections
            .insert(
                connection,
                Connection {
                    state: ConnectionState::Init,
                    on_success,
                    on_timeout,
                    on_error,
                    on_close,
                },
            )
            .is_some()
        {
            panic!("Attempt to re-use existing connection {:?}", connection)
        }
    }

    pub fn remove_connection(&mut self, connection: &Uid) {
        self.connections.remove(connection).expect(&format!(
            "Attempt to remove an inexistent connection {:?}",
            connection
        ));
    }

    pub fn new_recv_request(
        &mut self,
        uid: &Uid,
        connection: Uid,
        on_success: Redispatch<(Uid, Vec<u8>)>,
        on_timeout: Redispatch<(Uid, Vec<u8>)>,
        on_error: Redispatch<(Uid, String)>,
    ) {
        if self
            .recv_requests
            .insert(
                *uid,
                RecvRequest {
                    connection,
                    on_success,
                    on_timeout,
                    on_error,
                },
            )
            .is_some()
        {
            panic!("Attempt to re-use existing RecvRequest {:?}", uid)
        }
    }

    pub fn take_recv_request(&mut self, uid: &Uid) -> RecvRequest {
        self.recv_requests
            .remove(uid)
            .expect(&format!("Take attempt on inexistent RecvRequest {:?}", uid))
    }
}
