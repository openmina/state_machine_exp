use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::rc::Rc;
use std::time::Duration;

use crate::automaton::state::{Objects, Uid};

use super::action::{MioEvent, PollEventsResult, TcpReadResult, TcpWriteResult};

#[derive(Debug)]
pub struct TcpConnection {
    stream: TcpStream,
    address: std::net::SocketAddr,
}

pub struct MioState {
    poll_objects: RefCell<Objects<Poll>>,
    events_objects: RefCell<Objects<Events>>,
    tcp_listener_objects: RefCell<Objects<TcpListener>>,
    tcp_connection_objects: RefCell<Objects<TcpConnection>>,
}

impl MioState {
    fn new_poll(&mut self, uid: Uid, obj: Poll) {
        if self.poll_objects.borrow_mut().insert(uid, obj).is_some() {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    fn new_events(&mut self, uid: Uid, obj: Events) {
        if self.events_objects.borrow_mut().insert(uid, obj).is_some() {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    fn new_tcp_listener(&mut self, uid: Uid, obj: TcpListener) {
        if self
            .tcp_listener_objects
            .borrow_mut()
            .insert(uid, obj)
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    fn new_tcp_connection(&mut self, uid: Uid, obj: TcpConnection) {
        if self
            .tcp_connection_objects
            .borrow_mut()
            .insert(uid, obj)
            .is_some()
        {
            panic!("Attempt to re-use existing uid {:?}", uid)
        }
    }

    pub fn poll_create(&mut self, uid: Uid) -> bool {
        Poll::new()
            .and_then(|poll_obj| Ok(self.new_poll(uid, poll_obj)))
            .is_ok()
    }

    pub fn poll_register_tcp_server(
        &mut self,
        poll_uid: &Uid,
        tcp_listener_uid: &Uid,
        token: Uid,
    ) -> bool {
        let mut tcp_listener_objects = self.tcp_listener_objects.borrow_mut();

        let tcp_listener = tcp_listener_objects
            .get_mut(tcp_listener_uid)
            .unwrap_or_else(|| panic!("TcpListener object (uid {:?}) not found", tcp_listener_uid));

        if let Some(poll) = self.poll_objects.borrow().get(poll_uid) {
            poll.registry()
                .register(tcp_listener, Token(token.into()), Interest::READABLE)
                .is_ok()
        } else {
            panic!("Poll object not found (uid: {:?}", poll_uid)
        }
    }

    pub fn poll_register_tcp_connection(
        &mut self,
        poll_uid: &Uid,
        connection_uid: &Uid,
        token: Uid,
    ) -> bool {
        let mut tcp_connection_objects = self.tcp_connection_objects.borrow_mut();

        let Some(TcpConnection { stream, .. }) = tcp_connection_objects.get_mut(connection_uid)
        else {
            panic!("TcpConnection object not found (Uid: {:?}", connection_uid)
        };

        if let Some(poll) = self.poll_objects.borrow().get(poll_uid) {
            poll.registry()
                .register(
                    stream,
                    Token(token.into()),
                    Interest::READABLE.add(Interest::WRITABLE),
                )
                .is_ok()
        } else {
            panic!("Poll object not found (uid: {:?}", poll_uid)
        }
    }

    pub fn poll_events(
        &mut self,
        poll_uid: &Uid,
        events_uid: &Uid,
        timeout: Option<u64>,
    ) -> PollEventsResult {
        if let Some(poll) = self.poll_objects.borrow_mut().get_mut(poll_uid) {
            let mut events_object = self.events_objects.borrow_mut();
            let Some(events) = events_object.get_mut(events_uid) else {
                panic!("Events object not found (uid: {:?})", events_uid)
            };

            let timeout = timeout.and_then(|ms| Some(Duration::from_millis(ms)));

            match poll.poll(events, timeout) {
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    PollEventsResult::Interrupted
                }
                Err(err) => PollEventsResult::Error(err.to_string()),
                Ok(_) => {
                    let events = events
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
        } else {
            panic!("Poll object not found (uid: {:?}", poll_uid)
        }
    }

    pub fn events_create(&mut self, uid: Uid, capacity: usize) {
        self.new_events(uid, Events::with_capacity(capacity));
    }

    pub fn tcp_listen(&mut self, uid: Uid, address: String) -> Result<(), String> {
        match address.parse() {
            Ok(address) => match TcpListener::bind(address) {
                Ok(tcp_listener) => {
                    self.new_tcp_listener(uid, tcp_listener);
                    Ok(())
                }
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn tcp_accept(&mut self, uid: Uid, listener_uid: &Uid) -> Result<(), String> {
        let accept_result = {
            let tcp_listener_objects = self.tcp_listener_objects.borrow();
            let tcp_listener = tcp_listener_objects
                .get(listener_uid)
                .unwrap_or_else(|| panic!("TcpListener object (uid {:?}) not found", uid));

            tcp_listener.accept()
        };

        match accept_result {
            Ok((stream, address)) => {
                Ok(self.new_tcp_connection(uid, TcpConnection { stream, address }))
            }
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn tcp_write(&mut self, connection_uid: &Uid, data: &[u8]) -> TcpWriteResult {
        let mut tcp_connection_objects = self.tcp_connection_objects.borrow_mut();

        let Some(TcpConnection { stream, .. }) = tcp_connection_objects.get_mut(connection_uid)
        else {
            panic!("TcpConnection object not found (Uid: {:?}", connection_uid)
        };

        match stream.write(data) {
            Ok(written) => {
                if written < data.len() {
                    TcpWriteResult::WrittenPartial(written)
                } else {
                    TcpWriteResult::WrittenAll
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => TcpWriteResult::Interrupted,
            Err(err) => TcpWriteResult::Error(err.to_string()),
        }
    }

    pub fn tcp_read(&mut self, connection_uid: &Uid, len: usize) -> TcpReadResult {
        let mut tcp_connection_objects = self.tcp_connection_objects.borrow_mut();

        let Some(TcpConnection { stream, .. }) = tcp_connection_objects.get_mut(connection_uid)
        else {
            panic!("TcpConnection object not found (Uid: {:?}", connection_uid)
        };

        let mut recv_buf = vec![0u8; len];

        match stream.read(&mut recv_buf) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => TcpReadResult::Interrupted,
            Err(err) => TcpReadResult::Error(err.to_string()),
            Ok(0) => TcpReadResult::ConnectionClosed,
            Ok(read) if read < len => {
                recv_buf.truncate(read);

                TcpReadResult::ReadPartial {
                    bytes_read: Rc::new(recv_buf),
                    remaining: len - read,
                }
            }
            Ok(_) => TcpReadResult::ReadAll(Rc::new(recv_buf)),
        }
    }
}
