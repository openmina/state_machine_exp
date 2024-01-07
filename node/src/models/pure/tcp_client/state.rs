use crate::{
    automaton::{
        action::ResultDispatch,
        state::{Objects, Uid},
    },
    models::pure::tcp::action::{ConnectionResult, RecvResult, SendResult},
};

pub struct Connection {
    pub on_close_connection: ResultDispatch<Uid>,
    pub on_result: ResultDispatch<(Uid, ConnectionResult)>,
}

pub struct SendRequest {
    pub connection: Uid,
    pub on_result: ResultDispatch<(Uid, SendResult)>,
}

pub struct RecvRequest {
    pub connection: Uid,
    pub on_result: ResultDispatch<(Uid, RecvResult)>,
}

pub struct TcpClientState {
    pub connections: Objects<Connection>,
    pub send_requests: Objects<SendRequest>,
    pub recv_requests: Objects<RecvRequest>,
}

impl TcpClientState {
    pub fn new() -> Self {
        Self {
            connections: Objects::<Connection>::new(),
            send_requests: Objects::<SendRequest>::new(),
            recv_requests: Objects::<RecvRequest>::new(),
        }
    }
    pub fn get_connection(&self, connection: &Uid) -> &Connection {
        self.connections
            .get(connection)
            .expect(&format!("Connection object {:?} not found", connection))
    }

    pub fn new_connection(
        &mut self,
        connection: Uid,
        on_close_connection: ResultDispatch<Uid>,
        on_result: ResultDispatch<(Uid, ConnectionResult)>,
    ) {
        if self
            .connections
            .insert(
                connection,
                Connection {
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
            panic!("Attempt to re-use existing SendRequest {:?}", uid)
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
}
