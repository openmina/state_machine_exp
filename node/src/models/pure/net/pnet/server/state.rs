use crate::{
    automaton::{
        action::{Redispatch, Timeout},
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
pub struct Listener {
    pub on_success: Redispatch<Uid>,
    pub on_error: Redispatch<(Uid, String)>,
    pub on_new_connection: Redispatch<(Uid, Uid)>,
    pub on_new_connection_error: Redispatch<(Uid, Uid, String)>,
    pub on_connection_closed: Redispatch<(Uid, Uid)>,
    pub on_listener_closed: Redispatch<Uid>,
    pub connections: Objects<Connection>,
}

impl Listener {
    pub fn new(
        on_success: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
        on_new_connection: Redispatch<(Uid, Uid)>,
        on_new_connection_error: Redispatch<(Uid, Uid, String)>,
        on_connection_closed: Redispatch<(Uid, Uid)>,
        on_listener_closed: Redispatch<Uid>,
    ) -> Self {
        Self {
            on_success,
            on_error,
            on_new_connection,
            on_new_connection_error,
            on_connection_closed,
            on_listener_closed,
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
    pub recv_nonce_timeout: Timeout,
}

#[derive(Debug)]
pub struct PnetServerState {
    pub listeners: Objects<Listener>,
    pub recv_requests: Objects<RecvRequest>,
    pub config: PnetServerConfig,
}

impl PnetServerState {
    pub fn from_config(config: PnetServerConfig) -> Self {
        Self {
            listeners: Objects::<Listener>::new(),
            recv_requests: Objects::<RecvRequest>::new(),
            config,
        }
    }

    pub fn new_listener(
        &mut self,
        listener: Uid,
        on_success: Redispatch<Uid>,
        on_error: Redispatch<(Uid, String)>,
        on_new_connection: Redispatch<(Uid, Uid)>,
        on_new_connection_error: Redispatch<(Uid, Uid, String)>,
        on_connection_closed: Redispatch<(Uid, Uid)>,
        on_listener_closed: Redispatch<Uid>,
    ) {
        if self
            .listeners
            .insert(
                listener,
                Listener::new(
                    on_success,
                    on_error,
                    on_new_connection,
                    on_new_connection_error,
                    on_connection_closed,
                    on_listener_closed,
                ),
            )
            .is_some()
        {
            panic!("Attempt to re-use existing {:?}", listener)
        }
    }

    pub fn get_listener(&self, listener: &Uid) -> &Listener {
        self.listeners
            .get(listener)
            .expect(&format!("Listener {:?} not found", listener))
    }

    pub fn get_listener_mut(&mut self, listener: &Uid) -> &mut Listener {
        self.listeners
            .get_mut(listener)
            .expect(&format!("Listener {:?} not found", listener))
    }

    pub fn remove_listener(&mut self, listener: &Uid) {
        self.listeners.remove(listener).expect(&format!(
            "Attempt to remove an inexistent Listener {:?}",
            listener
        ));
    }

    pub fn new_connection(&mut self, listener: Uid, connection: Uid) {
        self.get_listener_mut(&listener).connections.insert(
            connection,
            Connection {
                state: ConnectionState::Init,
            },
        );
    }

    pub fn find_listener_by_connection(&self, connection: &Uid) -> &Uid {
        let (listener, _) = self
            .listeners
            .iter()
            .find(|(_, Listener { connections, .. })| connections.contains_key(connection))
            .expect(&format!(
                "No Listener containing Connection {:?}",
                connection
            ));

        listener
    }

    pub fn get_connection(&self, connection: &Uid) -> &Connection {
        let listener = self.find_listener_by_connection(connection);

        self.listeners
            .get(listener)
            .expect(&format!("Listener {:?} not found", listener))
            .connections
            .get(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn get_connection_mut(&mut self, connection: &Uid) -> &mut Connection {
        let listener = *self.find_listener_by_connection(connection);

        self.listeners
            .get_mut(&listener)
            .expect(&format!("Listener {:?} not found", listener))
            .connections
            .get_mut(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn find_connection_uid_by_nonce_request(&self, uid: &Uid) -> Uid {
        for (_, Listener { connections, .. }) in self.listeners.iter() {
            if let Some((connection, _)) =
                connections
                    .iter()
                    .find(|(_connection, Connection { state, .. })| match state {
                        ConnectionState::Init => false,
                        ConnectionState::NonceSent { send_request, .. } => send_request == uid,
                        ConnectionState::NonceWait { recv_request, .. } => recv_request == uid,
                        ConnectionState::Ready { .. } => false,
                    })
            {
                return *connection;
            }
        }

        panic!("No connection object with nonce request {:?}", uid)
    }

    pub fn find_connection_mut_by_nonce_request(&mut self, uid: &Uid) -> (&Uid, &mut Connection) {
        for (_, Listener { connections, .. }) in self.listeners.iter_mut() {
            if let Some(result) =
                connections
                    .iter_mut()
                    .find(|(_connection, Connection { state, .. })| match state {
                        ConnectionState::Init => false,
                        ConnectionState::NonceSent { send_request, .. } => send_request == uid,
                        ConnectionState::NonceWait { recv_request, .. } => recv_request == uid,
                        ConnectionState::Ready { .. } => false,
                    })
            {
                return result;
            }
        }

        panic!("No connection object with nonce request {:?}", uid)
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
