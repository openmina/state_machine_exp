use crate::{
    automaton::{
        action::{ResultDispatch, Timeout},
        state::{Objects, Uid},
    },
    models::pure::net::{
        pnet::common::{ConnectionState, PnetKey},
        tcp_server::state::RecvRequest,
    },
};

#[derive(Debug)]
pub struct Connection {
    pub state: ConnectionState,
}

#[derive(Debug)]
pub struct Server {
    pub on_new_connection: ResultDispatch,
    pub on_close_connection: ResultDispatch,
    pub on_result: ResultDispatch,
    pub connections: Objects<Connection>,
}

impl Server {
    pub fn new(
        on_new_connection: ResultDispatch,
        on_close_connection: ResultDispatch,
        on_result: ResultDispatch,
    ) -> Self {
        Self {
            on_new_connection,
            on_close_connection,
            on_result,
            connections: Objects::new(),
        }
    }

    pub fn remove_connection(&mut self, uid: &Uid) {
        self.connections.remove(uid);
    }
}

#[derive(Debug)]
pub struct PnetServerConfig {
    pub pnet_key: PnetKey,
    pub send_nonce_timeout: Timeout,
    pub recv_nonce_timeout: Timeout
}

#[derive(Debug)]
pub struct PnetServerState {
    pub server_objects: Objects<Server>,
    pub recv_requests: Objects<RecvRequest>,
    pub config: PnetServerConfig,
}

impl PnetServerState {
    pub fn from_config(config: PnetServerConfig) -> Self {
        Self {
            server_objects: Objects::<Server>::new(),
            recv_requests: Objects::<RecvRequest>::new(),
            config,
        }
    }

    pub fn new_server(
        &mut self,
        server: Uid,
        on_new_connection: ResultDispatch,
        on_close_connection: ResultDispatch,
        on_result: ResultDispatch,
    ) {
        if self
            .server_objects
            .insert(
                server,
                Server::new(on_new_connection, on_close_connection, on_result),
            )
            .is_some()
        {
            panic!("Attempt to re-use existing {:?}", server)
        }
    }

    pub fn get_server(&self, server: &Uid) -> &Server {
        self.server_objects
            .get(server)
            .expect(&format!("Server object {:?} not found", server))
    }

    pub fn get_server_mut(&mut self, server: &Uid) -> &mut Server {
        self.server_objects
            .get_mut(server)
            .expect(&format!("Server object {:?} not found", server))
    }

    pub fn remove_server(&mut self, server: &Uid) {
        self.server_objects.remove(server).expect(&format!(
            "Attempt to remove an inexistent Server {:?}",
            server
        ));
    }

    pub fn new_connection(&mut self, server: Uid, connection: Uid) {
        self.get_server_mut(&server).connections.insert(
            connection,
            Connection {
                state: ConnectionState::Init,
            },
        );
    }

    pub fn get_connection(&self, server: &Uid, connection: &Uid) -> &Connection {
        let server = self
            .server_objects
            .get(server)
            .expect(&format!("Server object {:?} not found", server));

        server
            .connections
            .get(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn get_connection_mut(&mut self, server: &Uid, connection: &Uid) -> &mut Connection {
        let server = self
            .server_objects
            .get_mut(server)
            .expect(&format!("Server object {:?} not found", server));

        server
            .connections
            .get_mut(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn find_connection_by_nonce_request(&self, uid: &Uid) -> (&Uid, &Uid, &Connection) {
        for (server_uid, server) in self.server_objects.iter() {
            let maybe_conn = server.connections.iter().find(
                |(_connection, Connection { state, .. })| match state {
                    ConnectionState::Init => false,
                    ConnectionState::NonceSent { send_request, .. } => send_request == uid,
                    ConnectionState::NonceWait { recv_request, .. } => recv_request == uid,
                    ConnectionState::Ready { .. } => false
                },
            );

            if let Some((connection, conn)) = maybe_conn {
                return (server_uid, connection, conn);
            }
        }

        panic!("No connection object with nonce request {:?}", uid)
    }

    pub fn find_server_by_connection(&self, connection: &Uid) -> &Uid {
        let (server, _) = self
            .server_objects
            .iter()
            .find(|(_uid, server)| server.connections.contains_key(connection))
            .expect(&format!("No server containing Connection {:?}", connection));

        server
    }

    pub fn new_recv_request(&mut self, uid: &Uid, connection: Uid, on_result: ResultDispatch) {
        if self
            .recv_requests
            .insert(
                *uid,
                RecvRequest {
                    connection,
                    on_result,
                },
            )
            .is_some()
        {
            panic!("Attempt to re-use existing {:?}", uid)
        }
    }

    pub fn take_recv_request(&mut self, uid: &Uid) -> RecvRequest {
        self.recv_requests
            .remove(uid)
            .expect(&format!("Take attempt on inexistent RecvRequest {:?}", uid))
    }
}
