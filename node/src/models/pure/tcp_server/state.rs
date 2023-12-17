use std::{collections::BTreeSet, mem};

use crate::{automaton::{
    action::CompletionRoutine,
    state::{Objects, Uid},
}, models::pure::tcp::{action::RecvResult, state::SendResult}};

pub struct Server {
    pub address: String,
    pub max_connections: usize,
    pub on_new_connection: CompletionRoutine<(Uid, Uid)>,
    pub on_close_connection: CompletionRoutine<(Uid, Uid)>,
    pub on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    pub connections: BTreeSet<Uid>,
}

impl Server {
    pub fn new(
        address: String,
        max_connections: usize,
        on_new_connection: CompletionRoutine<(Uid, Uid)>,
        on_close_connection: CompletionRoutine<(Uid, Uid)>,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) -> Self {
        Self {
            address,
            max_connections,
            on_new_connection,
            on_close_connection,
            on_completion,
            connections: BTreeSet::new(),
        }
    }

    pub fn remove_connection(&mut self, uid: &Uid) {
        self.connections.remove(uid);
    }
}

pub struct SendRequest {
    pub connection_uid: Uid,
    pub on_completion: CompletionRoutine<(Uid, SendResult)>,
}

pub struct RecvRequest {
    pub connection_uid: Uid,
    pub on_completion: CompletionRoutine<(Uid, RecvResult)>,
}

pub struct PollRequest {
    pub timeout: Option<u64>,
    pub on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
}

pub struct TcpServerState {
    pub server_objects: Objects<Server>,
    pub send_requests: Objects<SendRequest>,
    pub recv_requests: Objects<RecvRequest>,
    pub poll_request: Option<PollRequest>,
}

impl TcpServerState {
    pub fn new() -> Self {
        Self {
            server_objects: Objects::<Server>::new(),
            send_requests: Objects::<SendRequest>::new(),
            recv_requests: Objects::<RecvRequest>::new(),
            poll_request: None,
        }
    }

    pub fn set_poll_request(&mut self, request: PollRequest) {
        assert!(self.poll_request.is_none());
        self.poll_request = Some(request);
    }

    pub fn take_poll_request(&mut self) -> PollRequest {
        mem::take(&mut self.poll_request).expect("Take attempt on inexisting PollRequest")
    }

    pub fn new_send_request(
        &mut self,
        uid: &Uid,
        connection_uid: Uid,
        on_completion: CompletionRoutine<(Uid, SendResult)>,
    ) {
        if self
            .send_requests
            .insert(
                *uid,
                SendRequest {
                    connection_uid,
                    on_completion,
                },
            )
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn take_send_request(&mut self, uid: &Uid) -> SendRequest {
        self.send_requests
            .remove(uid)
            .expect("Take attempt on inexisting SendRequest")
    }

    pub fn new_recv_request(
        &mut self,
        uid: &Uid,
        connection_uid: Uid,
        on_completion: CompletionRoutine<(Uid, RecvResult)>,
    ) {
        if self
            .recv_requests
            .insert(
                *uid,
                RecvRequest {
                    connection_uid,
                    on_completion,
                },
            )
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn take_recv_request(&mut self, uid: &Uid) -> RecvRequest {
        self.recv_requests
            .remove(uid)
            .expect("Take attempt on inexisting SendRequest")
    }

    pub fn new_connection(&mut self, connection_uid: Uid, listener_uid: Uid) {
        self.get_server_mut(&listener_uid)
            .connections
            .insert(connection_uid);
    }

    // pub fn get_connection_server(&self, connection_uid: &Uid) -> (&Uid, &Server) {
    //     self.server_objects
    //         .iter()
    //         .find(|(_, server)| server.connections.contains(connection_uid))
    //         .expect("Server not found for connection")
    // }

    pub fn get_connection_server_mut(&mut self, connection_uid: &Uid) -> (&Uid, &mut Server) {
        self.server_objects
            .iter_mut()
            .find(|(_, server)| server.connections.contains(connection_uid))
            .expect("Server not found for connection")
    }

    pub fn new_server(
        &mut self,
        uid: Uid,
        address: String,
        max_connections: usize,
        on_new_connection: CompletionRoutine<(Uid, Uid)>,
        on_close_connection: CompletionRoutine<(Uid, Uid)>,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        if self
            .server_objects
            .insert(
                uid,
                Server::new(
                    address,
                    max_connections,
                    on_new_connection,
                    on_close_connection,
                    on_completion,
                ),
            )
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn get_server(&self, uid: &Uid) -> &Server {
        self.server_objects
            .get(uid)
            .unwrap_or_else(|| panic!("Server object (uid {:?}) not found", uid))
    }

    pub fn get_server_mut(&mut self, uid: &Uid) -> &mut Server {
        self.server_objects
            .get_mut(uid)
            .unwrap_or_else(|| panic!("Server object (uid {:?}) not found", uid))
    }

    pub fn remove_server(&mut self, uid: &Uid) {
        self.server_objects
            .remove(uid)
            .unwrap_or_else(|| panic!("Attempt to remove an inexistent Server (uid {:?})", uid));
    }
}
