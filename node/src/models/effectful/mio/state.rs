//use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::cell::{Ref, RefCell, RefMut};
use std::io::{self, Read, Write};
use std::rc::Rc;
//use std::str::from_utf8;
use std::time::Duration;

use crate::automaton::state::{Objects, Uid};

use super::action::{
    MioEvent, PollEventsResult, TcpReadResult, TcpWriteResult,
};

#[derive(Debug)]
pub struct TcpConnection {
    stream: TcpStream,
    address: std::net::SocketAddr,
}

#[derive(Debug)]
pub enum MioObject {
    Poll(RefCell<Poll>),
    Event(RefCell<Events>),
    TcpListener(RefCell<TcpListener>),
    TcpConnection(RefCell<TcpConnection>),
}

impl From<RefCell<Poll>> for MioObject {
    fn from(poll: RefCell<Poll>) -> Self {
        MioObject::Poll(poll)
    }
}

impl From<RefCell<Events>> for MioObject {
    fn from(event: RefCell<Events>) -> Self {
        MioObject::Event(event)
    }
}

impl From<RefCell<TcpListener>> for MioObject {
    fn from(listener: RefCell<TcpListener>) -> Self {
        MioObject::TcpListener(listener)
    }
}

impl From<RefCell<TcpConnection>> for MioObject {
    fn from(connection: RefCell<TcpConnection>) -> Self {
        MioObject::TcpConnection(connection)
    }
}

pub struct MioState(Objects<MioObject>);

impl MioState {
    fn new_obj<T>(&mut self, uid: Uid, obj: T)
    where
        MioObject: From<RefCell<T>>,
    {
        self.0
            .insert(uid, RefCell::new(obj).into())
            .unwrap_or_else(|| panic!("Attempt to re-use existing uid {:?}", uid));
    }

    fn get_obj(&self, uid: Uid) -> &MioObject {
        self.0
            .get(&uid)
            .unwrap_or_else(|| panic!("MioObject not found for uid {:?}", uid))
    }

    fn obj_as_poll(&self, uid: Uid) -> Ref<Poll> {
        let obj = self.get_obj(uid);
        match obj {
            MioObject::Poll(poll) => poll.borrow(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    fn obj_as_poll_mut(&self, uid: Uid) -> RefMut<Poll> {
        let obj = self.get_obj(uid);
        match obj {
            MioObject::Poll(poll) => poll.borrow_mut(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    fn obj_as_tcp_listener(&self, uid: Uid) -> Ref<TcpListener> {
        let obj = self.get_obj(uid);
        match obj {
            MioObject::TcpListener(server) => server.borrow(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    fn obj_as_tcp_listener_mut(&self, uid: Uid) -> RefMut<TcpListener> {
        let obj = self.get_obj(uid);
        match obj {
            MioObject::TcpListener(server) => server.borrow_mut(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    fn obj_as_tcp_connection_mut(&self, uid: Uid) -> RefMut<TcpConnection> {
        let obj = self.get_obj(uid);
        match obj {
            MioObject::TcpConnection(connection) => connection.borrow_mut(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    fn obj_as_events_mut(&self, uid: Uid) -> RefMut<Events> {
        let obj = self.get_obj(uid);
        match obj {
            MioObject::Event(events) => events.borrow_mut(),
            _ => panic!("Uid found for object of wrong type: {:?}", obj),
        }
    }

    pub fn poll_create(&mut self, uid: Uid) -> bool {
        match Poll::new() {
            Ok(poll) => {
                self.new_obj(uid, poll);
                true
            }
            Err(_) => false,
        }
    }

    pub fn poll_register_tcp_server(&mut self, poll: Uid, tcp_listener: Uid, token: Uid) -> bool {
        let poll = self.obj_as_poll(poll);
        let mut server = self.obj_as_tcp_listener_mut(tcp_listener);

        poll.registry()
            .register(&mut *server, Token(token.into()), Interest::READABLE)
            .is_ok()
    }

    pub fn poll_register_tcp_connection(&mut self, poll: Uid, connection: Uid, token: Uid) -> bool {
        let poll = self.obj_as_poll(poll);
        let TcpConnection { stream, .. } = &mut *self.obj_as_tcp_connection_mut(connection);

        poll.registry()
            .register(
                stream,
                Token(token.into()),
                Interest::READABLE.add(Interest::WRITABLE),
            )
            .is_ok()
    }

    pub fn poll_events(
        &mut self,
        poll: Uid,
        events: Uid,
        timeout: Option<u64>,
    ) -> PollEventsResult {
        let mut poll = self.obj_as_poll_mut(poll);
        let mut events_instance = self.obj_as_events_mut(events);
        let timeout = timeout.and_then(|ms| Some(Duration::from_millis(ms)));

        match poll.poll(&mut *events_instance, timeout) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => PollEventsResult::Interrupted,
            Err(err) => PollEventsResult::Error(err.to_string()),
            Ok(_) => {
                let events = events_instance
                    .iter()
                    .map(|event| MioEvent {
                        token: event.token().0.into(),
                        readable: event.is_readable(),
                        writable: event.is_writable(),
                        error: event.is_error(),
                        read_closed: event.is_read_closed(),
                        write_closed: event.is_write_closed(),
                        priority: event.is_priority(),
                        aio: event.is_aio(),
                        lio: event.is_lio(),
                    })
                    .collect();
                PollEventsResult::Events(events)
            }
        }
    }

    pub fn events_create(&mut self, uid: Uid, capacity: usize) {
        self.new_obj(uid, Events::with_capacity(capacity));
    }

    pub fn tcp_listen(&mut self, uid: Uid, address: String) -> Result<(), String> {
        match address.parse() {
            Ok(address) => match TcpListener::bind(address) {
                Ok(tcp_listener) => {
                    self.new_obj(uid, tcp_listener);
                    Ok(())
                }
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn tcp_accept(&mut self, uid: Uid, listener: Uid) -> Result<(), String> {
        let result = self.obj_as_tcp_listener(listener).accept();

        match result {
            Err(err) => Err(err.to_string()),
            Ok((stream, address)) => {
                self.new_obj(uid, TcpConnection { stream, address });
                Ok(())
            }
        }
    }

    pub fn tcp_write(&mut self, connection: Uid, data: &[u8]) -> TcpWriteResult {
        let TcpConnection { stream, .. } = &mut *self.obj_as_tcp_connection_mut(connection);

        match stream.write(data) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => TcpWriteResult::Interrupted,
            Err(err) => TcpWriteResult::Error(err.to_string()),
            Ok(written) if written < data.len() => TcpWriteResult::WrittenPartial(written),
            Ok(_) => TcpWriteResult::WrittenAll,
        }
    }

    pub fn tcp_read(&mut self, connection: Uid, len_bytes: usize) -> TcpReadResult {
        let TcpConnection { stream, .. } = &mut *self.obj_as_tcp_connection_mut(connection);
        let mut recv_buf = vec![0u8; len_bytes];

        match stream.read(&mut recv_buf) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => TcpReadResult::Interrupted,
            Err(err) => TcpReadResult::Error(err.to_string()),
            Ok(0) => TcpReadResult::ConnectionClosed,
            Ok(read) if read < len_bytes => TcpReadResult::ReadPartial {
                bytes_read: Rc::new(recv_buf),
                remaining: len_bytes - read,
            },
            Ok(_) => TcpReadResult::ReadAll(Rc::new(recv_buf)),
        }
    }
}
