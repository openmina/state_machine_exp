use crate::{
    automaton::{
        action::ResultDispatch,
        state::{Objects, Uid},
    },
    models::pure::net::tcp::action::{RecvResult, SendResult},
};
use std::{collections::BTreeSet, mem};

#[derive(Debug)]
pub struct Server {
    pub max_connections: usize,
    pub on_new_connection: ResultDispatch<(Uid, Uid)>,
    pub on_close_connection: ResultDispatch<(Uid, Uid)>,
    pub on_result: ResultDispatch<(Uid, Result<(), String>)>,
    pub connections: BTreeSet<Uid>,
}

impl Server {
    pub fn new(
        max_connections: usize,
        on_new_connection: ResultDispatch<(Uid, Uid)>,
        on_close_connection: ResultDispatch<(Uid, Uid)>,
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    ) -> Self {
        Self {
            max_connections,
            on_new_connection,
            on_close_connection,
            on_result,
            connections: BTreeSet::new(),
        }
    }

    pub fn remove_connection(&mut self, uid: &Uid) {
        self.connections.remove(uid);
    }
}

#[derive(Debug)]
pub struct SendRequest {
    pub connection: Uid,
    pub on_result: ResultDispatch<(Uid, SendResult)>,
}

#[derive(Debug)]
pub struct RecvRequest {
    pub connection: Uid,
    pub on_result: ResultDispatch<(Uid, RecvResult)>,
}

#[derive(Debug)]
pub struct PollRequest {
    pub on_result: ResultDispatch<(Uid, Result<(), String>)>,
}

#[derive(Debug)]
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
        mem::take(&mut self.poll_request).expect("Take attempt on inexistent PollRequest")
    }

    pub fn new_send_request(
        &mut self,
        uid: &Uid,
        connection: Uid,
        on_result: ResultDispatch<(Uid, SendResult)>,
    ) {
        if self
            .send_requests
            .insert(
                *uid,
                SendRequest {
                    connection,
                    on_result,
                },
            )
            .is_some()
        {
            panic!("Attempt to re-use existing {:?}", uid)
        }
    }

    pub fn take_send_request(&mut self, uid: &Uid) -> SendRequest {
        self.send_requests
            .remove(uid)
            .expect(&format!("Take attempt on inexistent SendRequest {:?}", uid))
    }

    pub fn new_recv_request(
        &mut self,
        uid: &Uid,
        connection: Uid,
        on_result: ResultDispatch<(Uid, RecvResult)>,
    ) {
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
            .expect(&format!("Take attempt on inexistent SendRequest {:?}", uid))
    }

    pub fn new_connection(&mut self, connection: Uid, listener: Uid) {
        self.get_server_mut(&listener)
            .connections
            .insert(connection);
    }

    pub fn get_connection_server_mut(&mut self, connection: &Uid) -> (&Uid, &mut Server) {
        self.server_objects
            .iter_mut()
            .find(|(_, server)| server.connections.contains(connection))
            .expect(&format!("Server not found for connection {:?}", connection))
    }

    pub fn new_server(
        &mut self,
        server: Uid,
        max_connections: usize,
        on_new_connection: ResultDispatch<(Uid, Uid)>,
        on_close_connection: ResultDispatch<(Uid, Uid)>,
        on_result: ResultDispatch<(Uid, Result<(), String>)>,
    ) {
        if self
            .server_objects
            .insert(
                server,
                Server::new(
                    max_connections,
                    on_new_connection,
                    on_close_connection,
                    on_result,
                ),
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
}
