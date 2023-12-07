use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};


use crate::{
    automaton::{
        action::CompletionRoutine,
        state::{Objects, Uid},
    },
    models::effectful::mio::action::MioEvent,
};

use super::action::{Event, InitResult, ListenerEvent, PollResult};

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

impl Status {
    pub fn set_init_error(&mut self, init_uid: Uid) {
        *self = Status::InitError { init_uid };
    }

    pub fn set_init_poll(
        &mut self,
        init_uid: Uid,
        poll_uid: Uid,
        on_completion: CompletionRoutine<(Uid, InitResult)>,
    ) {
        assert!(matches!(*self, Status::New));
        *self = Status::InitPollCreate {
            init_uid,
            poll_uid,
            on_completion,
        };
    }

    pub fn init_uid(&self) -> Uid {
        match *self {
            Status::New => unreachable!(),
            Status::InitError { init_uid } => init_uid,
            Status::InitPollCreate { init_uid, .. } => init_uid,
            Status::InitEventsCreate { init_uid, .. } => init_uid,
            Status::Ready { init_uid, .. } => init_uid,
        }
    }

    pub fn poll_uid(&self) -> Uid {
        match *self {
            Status::New => unreachable!(),
            Status::InitError { .. } => unreachable!(),
            Status::InitPollCreate { poll_uid, .. } => poll_uid,
            Status::InitEventsCreate { poll_uid, .. } => poll_uid,
            Status::Ready { poll_uid, .. } => poll_uid,
        }
    }

    pub fn events_uid(&self) -> Uid {
        match *self {
            Status::New => unreachable!(),
            Status::InitError { .. } => unreachable!(),
            Status::InitPollCreate { .. } => unreachable!(),
            Status::InitEventsCreate { events_uid, .. } => events_uid,
            Status::Ready { events_uid, .. } => events_uid,
        }
    }

    pub fn init_completion_routine(&self) -> CompletionRoutine<(Uid, InitResult)> {
        match self {
            Status::New => unreachable!(),
            Status::InitError { .. } => unreachable!(),
            Status::InitPollCreate { on_completion, .. } => on_completion.clone(),
            Status::InitEventsCreate { on_completion, .. } => on_completion.clone(),
            Status::Ready { .. } => unreachable!(),
        }
    }

    pub fn set_init_events(&mut self, events_uid: Uid) {
        let Status::InitPollCreate {
            init_uid,
            poll_uid,
            ref on_completion,
        } = *self
        else {
            panic!("set_init_events_create called from wrong state {:?}", self)
        };
        let on_completion = on_completion.clone();

        *self = Status::InitEventsCreate {
            init_uid,
            poll_uid,
            events_uid,
            on_completion,
        };
    }

    pub fn set_init_ready(&mut self) {
        let Status::InitEventsCreate {
            init_uid,
            poll_uid,
            events_uid,
            on_completion: _,
        } = *self
        else {
            panic!("set_init_events_create called from wrong state {:?}", self)
        };

        *self = Status::Ready {
            init_uid,
            poll_uid,
            events_uid,
        };
    }
}

#[derive(Debug)]
pub enum ListenerStatus {
    New,
    Ready,
    Error,
}

#[derive(Debug)]
pub struct Listener {
    pub status: ListenerStatus,
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
            status: ListenerStatus::New,
            address,
            on_completion,
            events: None,
        }
    }

    pub fn add_event(&mut self, event: &MioEvent) {
        let new_event = if event.error {
            ListenerEvent::Error
        } else if event.read_closed || event.write_closed {
            ListenerEvent::Closed
        } else {
            let previous_event = &self.events;

            match previous_event {
                Some(ListenerEvent::Closed) => ListenerEvent::Closed, // TODO: log message that we keep previous event
                Some(ListenerEvent::Error) => ListenerEvent::Error, // TODO: log message that we keep previous event
                Some(ListenerEvent::AcceptPending(count)) => {
                    ListenerEvent::AcceptPending(count.saturating_add(1))
                }
                None => ListenerEvent::AcceptPending(0),
            }
        };

        self.events = Some(new_event);
    }

    pub fn get_events(&self) -> Option<Event> {
        self.events
            .as_ref()
            .and_then(|ev| Some(Event::Listener(ev.clone())))
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
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug)]
pub enum ConnectionEvent {
    Ready { recv: bool, send: bool },
    Closed,
    Error,
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

    pub fn add_event(&mut self, event: &MioEvent) {
        let new_event = if event.error {
            ConnectionEvent::Error
        } else if event.read_closed || event.write_closed {
            ConnectionEvent::Closed
        } else {
            let previous_event = &self.events;

            match previous_event {
                Some(ConnectionEvent::Closed) => ConnectionEvent::Closed, // TODO: log message that we keep previous event
                Some(ConnectionEvent::Error) => ConnectionEvent::Error, // TODO: log message that we keep previous event
                Some(ConnectionEvent::Ready { .. }) => ConnectionEvent::Ready {
                    recv: event.readable,
                    send: event.writable,
                },
                None => ConnectionEvent::Ready {
                    recv: false,
                    send: false,
                },
            }
        };

        self.events = Some(new_event);
    }

    pub fn get_events(&self) -> Option<Event> {
        self.events
            .as_ref()
            .and_then(|ev| Some(Event::Connection(ev.clone())))
    }

    pub fn set_events(&mut self, events: ConnectionEvent) {
        self.events = Some(events);
    }
}

#[derive(Debug)]
pub struct SendRequest {
    pub connection: Uid,
    pub data: Rc<[u8]>,
    pub bytes_sent: usize,
    pub send_on_poll: bool,
    pub on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
}

impl SendRequest {
    pub fn new(
        connection: Uid,
        data: Rc<[u8]>,
        send_on_poll: bool,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) -> Self {
        Self {
            connection,
            data,
            bytes_sent: 0,
            send_on_poll,
            on_completion,
        }
    }
}

#[derive(Debug)]
pub enum TcpObject {
    Listener(RefCell<Listener>),
    PollRequest(RefCell<PollRequest>),
    Connection(RefCell<Connection>),
    SendRequest(RefCell<SendRequest>),
}

impl TcpObject {
    pub fn add_event(&self, event: &MioEvent) {
        match self {
            TcpObject::Listener(listener) => listener.borrow_mut().add_event(event),
            TcpObject::PollRequest(_) => unreachable!(),
            TcpObject::Connection(connection) => connection.borrow_mut().add_event(event),
            TcpObject::SendRequest(_) => unreachable!(),
        }
    }

    pub fn get_events(&self) -> Option<Event> {
        match self {
            TcpObject::Listener(listener) => listener.borrow().get_events(),
            TcpObject::PollRequest(_) => unreachable!(),
            TcpObject::Connection(connection) => connection.borrow().get_events(),
            TcpObject::SendRequest(_) => unreachable!(),
        }
    }
}

impl From<RefCell<Listener>> for TcpObject {
    fn from(listener: RefCell<Listener>) -> Self {
        TcpObject::Listener(listener)
    }
}

impl From<RefCell<PollRequest>> for TcpObject {
    fn from(request: RefCell<PollRequest>) -> Self {
        TcpObject::PollRequest(request)
    }
}

impl From<RefCell<Connection>> for TcpObject {
    fn from(connection: RefCell<Connection>) -> Self {
        TcpObject::Connection(connection)
    }
}

impl From<RefCell<SendRequest>> for TcpObject {
    fn from(request: RefCell<SendRequest>) -> Self {
        TcpObject::SendRequest(request)
    }
}

pub struct TcpState {
    pub status: Status,
    objects: Objects<TcpObject>,
}

impl TcpState {
    pub fn new() -> Self {
        Self {
            status: Status::New,
            objects: Objects::<TcpObject>::new(),
        }
    }

    fn new_obj<T>(&mut self, uid: Uid, obj: T)
    where
        TcpObject: From<RefCell<T>>,
    {
        self.objects
            .insert(uid, RefCell::new(obj).into())
            .unwrap_or_else(|| panic!("Attempt to re-use existing uid {:?}", uid));
    }

    pub fn remove_obj(&mut self, uid: Uid) {
        self.objects
            .remove(&uid)
            .unwrap_or_else(|| panic!("Attempt to remove inexisting object (Uid: {:?}", uid));
    }

    pub fn new_listener(
        &mut self,
        uid: Uid,
        address: String,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        self.new_obj(uid, Listener::new(address, on_completion))
    }

    pub fn new_poll(
        &mut self,
        uid: Uid,
        objects: Vec<Uid>,
        timeout: Option<u64>,
        on_completion: CompletionRoutine<(Uid, PollResult)>,
    ) {
        assert!(objects.iter().all(|uid| self.objects.contains_key(uid)));

        self.new_obj(uid, PollRequest::new(objects, timeout, on_completion))
    }

    pub fn new_incoming_connection(
        &mut self,
        uid: Uid,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        self.new_obj(
            uid,
            Connection::new(ConnectionType::Incoming, on_completion),
        )
    }

    pub fn new_send_request(
        &mut self,
        uid: Uid,
        connection: Uid,
        data: Rc<[u8]>,
        send_on_poll: bool,
        on_completion: CompletionRoutine<(Uid, Result<(), String>)>,
    ) {
        self.new_obj(
            uid,
            SendRequest::new(connection, data, send_on_poll, on_completion),
        )
    }

    pub fn get_obj(&self, uid: Uid) -> &TcpObject {
        self.objects
            .get(&uid)
            .unwrap_or_else(|| panic!("TcpObject not found for uid {:?}", uid))
    }

    pub fn add_obj_event(&mut self, event: &MioEvent) {
        self.get_obj(event.token).add_event(event)
    }

    pub fn get_obj_events(&self, uid: Uid) -> Option<Event> {
        self.get_obj(uid).get_events()
    }

    pub fn obj_as_listener(&self, uid: Uid) -> Ref<Listener> {
        let obj = self.get_obj(uid);
        match obj {
            TcpObject::Listener(listener) => listener.borrow(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    pub fn obj_as_connection(&self, uid: Uid) -> Ref<Connection> {
        let obj = self.get_obj(uid);
        match obj {
            TcpObject::Connection(connection) => connection.borrow(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    pub fn obj_as_connection_mut(&self, uid: Uid) -> RefMut<Connection> {
        let obj = self.get_obj(uid);
        match obj {
            TcpObject::Connection(connection) => connection.borrow_mut(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    pub fn obj_as_poll_request(&self, uid: Uid) -> Ref<PollRequest> {
        let obj = self.get_obj(uid);
        match obj {
            TcpObject::PollRequest(request) => request.borrow(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    pub fn obj_as_send_request_mut(&self, uid: Uid) -> RefMut<SendRequest> {
        let obj = self.get_obj(uid);
        match obj {
            TcpObject::SendRequest(request) => request.borrow_mut(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    pub fn pending_send_requests(&self) -> Vec<Uid> {
        self.objects
            .iter()
            .filter_map(|(uid, obj)| match obj {
                TcpObject::SendRequest(request) if request.borrow().send_on_poll => Some(*uid),
                _ => None,
            })
            .collect()
    }
}
