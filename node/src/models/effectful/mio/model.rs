use super::action::MioOutputAction;
use super::state::MioState;
use crate::automaton::action::Dispatcher;
use crate::automaton::model::{Output, OutputModel};
use crate::automaton::runner::{RegisterModel, RunnerBuilder};
use crate::automaton::state::ModelState;


// The `MioState` struct, implementing the `OutputModel` trait, provides the
// interface layer between the state-machine and the MIO crate for asynchronous
// I/O operations.
//
// It includes a set of operations for handling I/O actions such as:
// - Creating new poll objects for monitoring numerous I/O events.
// - Registering/deregistering TCP servers and connections with poll objects.
// - Polling events for asynchronous I/O notifications.
// - Managing TCP connections, including listening for, accepting, and
//   establishing connections, closing active connections, and reading/writing
//   data over established TCP connections.
//
// Each of these operations corresponds to a variant in `MioOutputAction`.
// The `process_output` function handles these actions by invoking the
// appropriate function in `MioState`, and dispatches the result back as a
// caller-defined `InputAction`.

impl RegisterModel for MioState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.model_output(Output::<Self>(Self::new()))
    }
}

impl OutputModel for MioState {
    type Action = MioOutputAction;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher) {
        match action {
            MioOutputAction::PollCreate { poll, on_result } => {
                dispatcher.dispatch_back(&on_result, (poll, self.poll_create(poll)));
            }
            MioOutputAction::PollRegisterTcpServer {
                poll,
                tcp_listener,
                on_result,
            } => {
                dispatcher.dispatch_back(
                    &on_result,
                    (
                        tcp_listener,
                        self.poll_register_tcp_server(&poll, tcp_listener),
                    ),
                );
            }
            MioOutputAction::PollRegisterTcpConnection {
                poll,
                connection,
                on_result,
            } => {
                dispatcher.dispatch_back(
                    &on_result,
                    (
                        connection,
                        self.poll_register_tcp_connection(&poll, connection),
                    ),
                );
            }
            MioOutputAction::PollDeregisterTcpConnection {
                poll,
                connection,
                on_result,
            } => {
                dispatcher.dispatch_back(
                    &on_result,
                    (
                        connection,
                        self.poll_deregister_tcp_connection(&poll, connection),
                    ),
                );
            }
            MioOutputAction::PollEvents {
                uid,
                poll,
                events,
                timeout,
                on_result,
            } => {
                dispatcher
                    .dispatch_back(&on_result, (uid, self.poll_events(&poll, &events, timeout)));
            }
            MioOutputAction::EventsCreate {
                uid,
                capacity,
                on_result,
            } => {
                self.events_create(uid, capacity);
                dispatcher.dispatch_back(&on_result, uid);
            }
            MioOutputAction::TcpListen {
                tcp_listener,
                address,
                on_result,
            } => {
                dispatcher.dispatch_back(
                    &on_result,
                    (tcp_listener, self.tcp_listen(tcp_listener, address)),
                );
            }
            MioOutputAction::TcpAccept {
                connection,
                tcp_listener,
                on_result,
            } => {
                dispatcher.dispatch_back(
                    &on_result,
                    (connection, self.tcp_accept(connection, &tcp_listener)),
                );
            }
            MioOutputAction::TcpConnect {
                connection,
                address,
                on_result,
            } => {
                dispatcher.dispatch_back(
                    &on_result,
                    (connection, self.tcp_connect(connection, address)),
                );
            }
            MioOutputAction::TcpClose {
                connection,
                on_result,
            } => {
                self.tcp_close(&connection);
                dispatcher.dispatch_back(&on_result, connection);
            }
            MioOutputAction::TcpWrite {
                uid,
                connection: connection_uid,
                data,
                on_result,
            } => {
                dispatcher.dispatch_back(&on_result, (uid, self.tcp_write(&connection_uid, &data)));
            }
            MioOutputAction::TcpRead {
                uid,
                connection,
                len,
                on_result,
            } => {
                dispatcher.dispatch_back(&on_result, (uid, self.tcp_read(&connection, len)));
            }
            MioOutputAction::TcpGetPeerAddress {
                connection,
                on_result,
            } => {
                dispatcher
                    .dispatch_back(&on_result, (connection, self.tcp_peer_address(&connection)));
            }
        }
    }
}
