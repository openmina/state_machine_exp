use core::panic;
use std::rc::Rc;

use crate::{
    automaton::{
        action::CompletionRoutine,
        state::{Objects, Uid},
    },
    models::effectful::mio::action::MioEvent,
};

use super::action::{ConnectionEvent, Event, InitResult, ListenerEvent, PollResult, RecvResult};

#[derive(Debug)]
pub struct Listener {
    pub address: String,
    pub on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    pub events: Option<ListenerEvent>,
}

impl Listener {
    pub fn new(
        address: String,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) -> Self {
        Self {
            address,
            on_completion,
            events: None,
        }
    }

    pub fn update_events(&mut self, event: &MioEvent) {
        let new_event = match event {
            MioEvent { error: true, .. } => ListenerEvent::Error,
            MioEvent {
                read_closed,
                write_closed,
                ..
            } if *read_closed || *write_closed => ListenerEvent::Closed,
            _ => ListenerEvent::AcceptPending,
        };

        if let Some(curr_event) = &mut self.events {
            match curr_event {
                ListenerEvent::Closed | ListenerEvent::Error => (), // TODO: log message saying we keep this event
                ListenerEvent::AcceptPending | ListenerEvent::ConnectionAccepted => {
                    *curr_event = new_event
                }
            }
        } else {
            self.events = Some(new_event);
        }
    }

    pub fn events(&self) -> &ListenerEvent {
        if let Some(event) = self.events.as_ref() {
            event
        } else {
            panic!("Attempt to fetch events but not initialized yet")
        }
    }

    pub fn events_mut(&mut self) -> &mut ListenerEvent {
        if let Some(event) = self.events.as_mut() {
            event
        } else {
            panic!("Attempt to fetch events but not initialized yet")
        }
    }
}

#[derive(Clone, Debug)]
pub struct PollRequest {
    pub objects: Vec<Uid>,
    pub timeout: Option<u64>,
    pub on_completion: CompletionRoutine<(Uid, PollResult)>,
}

impl PollRequest {
    pub fn new(
        objects: Vec<Uid>,
        timeout: Option<u64>,
        on_completion: CompletionRoutine<(Uid, PollResult)>,
    ) -> Self {
        Self {
            objects,
            timeout,
            on_completion,
        }
    }
}

#[derive(Debug)]
pub enum ConnectionType {
    Incoming(Uid), // Listener Uid
    Outgoing,
}

#[derive(Debug)]
pub struct Connection {
    pub conn_type: ConnectionType,
    pub on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    pub events: Option<ConnectionEvent>,
}

impl Connection {
    pub fn new(
        conn_type: ConnectionType,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) -> Self {
        Self {
            conn_type,
            on_completion,
            events: None,
        }
    }

    pub fn update_events(&mut self, event: &MioEvent) {
        let new_event = match event {
            MioEvent { error: true, .. } => ConnectionEvent::Error,
            MioEvent {
                read_closed,
                write_closed,
                ..
            } if *read_closed || *write_closed => ConnectionEvent::Closed,
            MioEvent {
                readable, writable, ..
            } => ConnectionEvent::Ready {
                recv: *readable,
                send: *writable,
            },
        };

        if let Some(curr_event) = &mut self.events {
            match curr_event {
                ConnectionEvent::Closed | ConnectionEvent::Error => (), // TODO: log message saying we keep this event
                ConnectionEvent::Ready { .. } => *curr_event = new_event,
            }
        } else {
            self.events = Some(new_event);
        }
    }

    pub fn events(&self) -> &ConnectionEvent {
        if let Some(event) = self.events.as_ref() {
            event
        } else {
            panic!("Attempt to fetch events but not initialized yet")
        }
    }

    pub fn events_mut(&mut self) -> &mut ConnectionEvent {
        if let Some(event) = self.events.as_mut() {
            event
        } else {
            panic!("Attempt to fetch events but not initialized yet")
        }
    }
}

#[derive(Debug)]
pub struct SendRequest {
    pub connection_uid: Uid,
    pub data: Rc<[u8]>,
    pub bytes_sent: usize,
    pub send_on_poll: bool,
    pub on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
}

impl SendRequest {
    pub fn new(
        connection_uid: Uid,
        data: Rc<[u8]>,
        send_on_poll: bool,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) -> Self {
        Self {
            connection_uid,
            data,
            bytes_sent: 0,
            send_on_poll,
            on_completion,
        }
    }
}

#[derive(Debug)]
pub struct RecvRequest {
    pub connection_uid: Uid,
    pub data: Vec<u8>,
    pub bytes_received: usize,
    pub recv_on_poll: bool,
    pub on_completion: CompletionRoutine<(Uid, RecvResult)>,
}

impl RecvRequest {
    pub fn new(
        connection_uid: Uid,
        count: usize,
        recv_on_poll: bool,
        on_completion: CompletionRoutine<(Uid, RecvResult)>,
    ) -> Self {
        Self {
            connection_uid,
            data: vec![0; count],
            bytes_received: 0,
            recv_on_poll,
            on_completion,
        }
    }
}

#[derive(Debug)]
pub enum Status {
    New,
    InitError {
        init_uid: Uid,
    },
    InitPollCreate {
        init_uid: Uid,
        poll_uid: Uid,
        on_completion: CompletionRoutine<(Uid, InitResult)>,
    },
    InitEventsCreate {
        init_uid: Uid,
        poll_uid: Uid,
        events_uid: Uid,
        on_completion: CompletionRoutine<(Uid, InitResult)>,
    },
    Ready {
        init_uid: Uid,
        poll_uid: Uid,
        events_uid: Uid,
    },
}

pub struct TcpState {
    pub status: Status,
    listener_objects: Objects<Listener>,
    connection_objects: Objects<Connection>,
    poll_request_objects: Objects<PollRequest>,
    send_request_objects: Objects<SendRequest>,
    recv_request_objects: Objects<RecvRequest>,
}

impl TcpState {
    pub fn new() -> Self {
        Self {
            status: Status::New,
            listener_objects: Objects::<Listener>::new(),
            connection_objects: Objects::<Connection>::new(),
            poll_request_objects: Objects::<PollRequest>::new(),
            send_request_objects: Objects::<SendRequest>::new(),
            recv_request_objects: Objects::<RecvRequest>::new(),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.status, Status::Ready { .. })
    }

    pub fn new_listener(
        &mut self,
        uid: Uid,
        address: String,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        if self
            .listener_objects
            .insert(uid, Listener::new(address, on_completion))
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn new_poll(
        &mut self,
        uid: Uid,
        objects: Vec<Uid>,
        timeout: Option<u64>,
        on_completion: CompletionRoutine<(Uid, PollResult)>,
    ) {
        assert!(objects
            .iter()
            .all(|uid| self.listener_objects.contains_key(uid)
                || self.connection_objects.contains_key(uid)));

        if self
            .poll_request_objects
            .insert(uid, PollRequest::new(objects, timeout, on_completion))
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn new_connection(
        &mut self,
        uid: Uid,
        conn_type: ConnectionType,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        if self
            .connection_objects
            .insert(uid, Connection::new(conn_type, on_completion))
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn new_send_request(
        &mut self,
        uid: Uid,
        connection_uid: Uid,
        data: Rc<[u8]>,
        send_on_poll: bool,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        if self
            .send_request_objects
            .insert(
                uid,
                SendRequest::new(connection_uid, data, send_on_poll, on_completion),
            )
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn new_recv_request(
        &mut self,
        uid: Uid,
        connection_uid: Uid,
        count: usize,
        recv_on_poll: bool,
        on_completion: CompletionRoutine<(Uid, RecvResult)>,
    ) {
        if self
            .recv_request_objects
            .insert(
                uid,
                RecvRequest::new(connection_uid, count, recv_on_poll, on_completion),
            )
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn get_listener(&self, uid: &Uid) -> &Listener {
        self.listener_objects
            .get(uid)
            .unwrap_or_else(|| panic!("Listener object (uid {:?}) not found", uid))
    }

    pub fn get_listener_mut(&mut self, uid: &Uid) -> &mut Listener {
        self.listener_objects
            .get_mut(uid)
            .unwrap_or_else(|| panic!("Listener object (uid {:?}) not found", uid))
    }

    pub fn remove_listener(&mut self, uid: &Uid) {
        self.listener_objects
            .remove(uid)
            .unwrap_or_else(|| panic!("Attempt to remove an inexistent Listener (uid {:?})", uid));
    }

    pub fn get_connection(&self, uid: &Uid) -> &Connection {
        self.connection_objects
            .get(uid)
            .unwrap_or_else(|| panic!("Connection object (uid {:?}) not found", uid))
    }

    pub fn get_connection_mut(&mut self, uid: &Uid) -> &mut Connection {
        self.connection_objects
            .get_mut(uid)
            .unwrap_or_else(|| panic!("Connection object (uid {:?}) not found", uid))
    }

    pub fn remove_connection(&mut self, uid: &Uid) {
        self.connection_objects.remove(uid).unwrap_or_else(|| {
            panic!("Attempt to remove an inexistent Connection (uid {:?})", uid)
        });
    }

    pub fn get_poll_request(&self, uid: &Uid) -> &PollRequest {
        self.poll_request_objects
            .get(uid)
            .unwrap_or_else(|| panic!("PollRequest object (uid {:?}) not found", uid))
    }

    pub fn remove_poll_request(&mut self, uid: &Uid) {
        self.poll_request_objects.remove(uid).unwrap_or_else(|| {
            panic!(
                "Attempt to remove an inexistent PollRequest (uid {:?})",
                uid
            )
        });
    }

    pub fn get_send_request(&self, uid: &Uid) -> &SendRequest {
        self.send_request_objects
            .get(uid)
            .unwrap_or_else(|| panic!("SendRequest object (uid {:?}) not found", uid))
    }

    pub fn get_send_request_mut(&mut self, uid: &Uid) -> &mut SendRequest {
        self.send_request_objects
            .get_mut(uid)
            .unwrap_or_else(|| panic!("SendRequest object (uid {:?}) not found", uid))
    }

    pub fn pending_send_requests(&self) -> Vec<(&Uid, &SendRequest)> {
        self.send_request_objects
            .iter()
            .filter(|(_, request)| request.send_on_poll)
            .collect()
    }

    pub fn remove_send_request(&mut self, uid: &Uid) {
        self.send_request_objects.remove(uid).unwrap_or_else(|| {
            panic!(
                "Attempt to remove an inexistent SendRequest (uid {:?})",
                uid
            )
        });
    }

    pub fn get_recv_request(&self, uid: &Uid) -> &RecvRequest {
        self.recv_request_objects
            .get(uid)
            .unwrap_or_else(|| panic!("RecvRequest object (uid {:?}) not found", uid))
    }

    pub fn get_recv_request_mut(&mut self, uid: &Uid) -> &mut RecvRequest {
        self.recv_request_objects
            .get_mut(uid)
            .unwrap_or_else(|| panic!("RecvRequest object (uid {:?}) not found", uid))
    }

    pub fn pending_recv_requests(&self) -> Vec<(&Uid, &RecvRequest)> {
        self.recv_request_objects
            .iter()
            .filter(|(_, request)| request.recv_on_poll)
            .collect()
    }

    pub fn remove_recv_request(&mut self, uid: &Uid) {
        self.recv_request_objects.remove(uid).unwrap_or_else(|| {
            panic!(
                "Attempt to remove an inexistent RecvRequest (uid {:?})",
                uid
            )
        });
    }

    pub fn get_events(&self, uid: &Uid) -> Option<(Uid, Event)> {
        if let Some(listener) = self.listener_objects.get(&uid) {
            listener
                .events
                .as_ref()
                .and_then(|event| Some((*uid, Event::Listener(event.clone()))))
        } else if let Some(connection) = self.connection_objects.get(&uid) {
            connection
                .events
                .as_ref()
                .and_then(|event| Some((*uid, Event::Connection(event.clone()))))
        } else {
            panic!("Received event for unknown object (uid: {:?}", uid)
        }
    }

    pub fn update_events(&mut self, event: &MioEvent) {
        let uid = event.token;

        if let Some(listener) = self.listener_objects.get_mut(&uid) {
            listener.update_events(event)
        } else if let Some(connection) = self.connection_objects.get_mut(&uid) {
            connection.update_events(event)
        } else {
            panic!("Received event for unknown object (uid: {:?}", uid)
        }
    }
}
