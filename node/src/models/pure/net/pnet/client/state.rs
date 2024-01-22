use crate::{
    automaton::{
        action::ResultDispatch,
        state::{Objects, Uid},
    },
    models::pure::net::tcp_client::state::RecvRequest,
};
use std::{
    fmt,
    ops::{Deref, DerefMut},
};

use salsa20::{cipher::generic_array::GenericArray, cipher::KeyIvInit, XSalsa20};

#[derive(Debug)]
pub struct PnetKey(pub [u8; 32]);

impl PnetKey {
    pub fn new(chain_id: &str) -> Self {
        use blake2::{
            digest::{generic_array, Update, VariableOutput},
            Blake2bVar,
        };

        let mut key = generic_array::GenericArray::default();
        Blake2bVar::new(32)
            .expect("valid constant")
            .chain(b"/coda/0.0.1/")
            .chain(chain_id.as_bytes())
            .finalize_variable(&mut key)
            .expect("good buffer size");
        Self(key.into())
    }
}

//#[derive(Clone)]
pub struct XSalsa20Wrapper {
    inner: XSalsa20,
}

impl XSalsa20Wrapper {
    pub fn new(shared_secret: &[u8; 32], nonce: &[u8; 24]) -> Self {
        XSalsa20Wrapper {
            inner: XSalsa20::new(
                GenericArray::from_slice(shared_secret),
                GenericArray::from_slice(nonce),
            ),
        }
    }
}

impl Deref for XSalsa20Wrapper {
    type Target = XSalsa20;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for XSalsa20Wrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl fmt::Debug for XSalsa20Wrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XSalsa20").finish()
    }
}

#[derive(Debug)]
pub enum ConnectionState {
    Init,
    NonceSent {
        send_request: Uid,
        nonce: [u8; 24],
    },
    NonceWait {
        recv_request: Uid,
        nonce_sent: [u8; 24],
    },
    Ready {
        send_cipher: XSalsa20Wrapper,
        recv_cipher: XSalsa20Wrapper,
    },
}

#[derive(Debug)]
pub struct Connection {
    pub state: ConnectionState,
    pub on_close_connection: ResultDispatch,
    pub on_result: ResultDispatch,
}

#[derive(Debug)]
pub struct PnetClientConfig {
    pub pnet_key: PnetKey,
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
                ConnectionState::Init => unreachable!(),
                ConnectionState::NonceSent { send_request, .. } => send_request == uid,
                ConnectionState::NonceWait { recv_request, .. } => recv_request == uid,
                ConnectionState::Ready { .. } => unreachable!(),
            })
            .expect(&format!(
                "No connection object with nonce request {:?}",
                uid
            ))
    }

    pub fn new_connection(
        &mut self,
        connection: Uid,
        on_close_connection: ResultDispatch,
        on_result: ResultDispatch,
    ) {
        if self
            .connections
            .insert(
                connection,
                Connection {
                    state: ConnectionState::Init,
                    on_close_connection,
                    on_result,
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
