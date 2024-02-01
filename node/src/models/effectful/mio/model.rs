use super::action::{MioEffectfulAction, PollResult, TcpAcceptResult, TcpReadResult, TcpWriteResult};
use super::state::MioState;
use crate::automaton::action::Dispatcher;
use crate::automaton::model::{Effectful, EffectfulModel};
use crate::automaton::runner::{RegisterModel, RunnerBuilder};
use crate::automaton::state::ModelState;

// The `MioState` struct, implementing the `EffectfulModel` trait, provides the
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
// Each of these operations corresponds to a variant in `MioAction`.
// The `process_effectful` function handles these actions by invoking the
// appropriate function in `MioState`, and dispatches the result back as a
// caller-defined `PureAction`.

impl RegisterModel for MioState {
    fn register<Substate: ModelState>(builder: RunnerBuilder<Substate>) -> RunnerBuilder<Substate> {
        builder.model_effectful(Effectful::<Self>(Self::new()))
    }
}

impl EffectfulModel for MioState {
    type Action = MioEffectfulAction;

    fn process_effectful(&mut self, action: Self::Action, dispatcher: &mut Dispatcher) {
        match action {
            MioEffectfulAction::PollCreate { poll, on_result } => {
                // NOTE: use this pattern to inhibit side-effects when in replay mode
                let result = if dispatcher.is_replayer() {
                    // This value is ignored and it is replaced by whatever it
                    // is in the recording file.
                    Ok(())
                } else {
                    self.poll_create(poll)
                };

                dispatcher.dispatch_back(&on_result, (poll, result));
            }
            MioEffectfulAction::PollRegisterTcpServer {
                poll,
                tcp_listener,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    Ok(()) // Ignored
                } else {
                    self.poll_register_tcp_server(&poll, tcp_listener)
                };

                dispatcher.dispatch_back(&on_result, (tcp_listener, result));
            }
            MioEffectfulAction::PollRegisterTcpConnection {
                poll,
                connection,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    Ok(()) // Ignored
                } else {
                    self.poll_register_tcp_connection(&poll, connection)
                };

                dispatcher.dispatch_back(&on_result, (connection, result));
            }
            MioEffectfulAction::PollDeregisterTcpConnection {
                poll,
                connection,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    Ok(()) // Ignored
                } else {
                    self.poll_deregister_tcp_connection(&poll, connection)
                };

                dispatcher.dispatch_back(&on_result, (connection, result));
            }
            MioEffectfulAction::PollEvents {
                uid,
                poll,
                events,
                timeout,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    PollResult::Events(Vec::new()) // Ignored
                } else {
                    self.poll_events(&poll, &events, timeout)
                };

                dispatcher.dispatch_back(&on_result, (uid, result));
            }
            MioEffectfulAction::EventsCreate {
                uid,
                capacity,
                on_result,
            } => {
                if !dispatcher.is_replayer() {
                    self.events_create(uid, capacity);
                }

                dispatcher.dispatch_back(&on_result, uid);
            }
            MioEffectfulAction::TcpListen {
                tcp_listener,
                address,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    Ok(()) // Ignored
                } else {
                    self.tcp_listen(tcp_listener, address)
                };

                dispatcher.dispatch_back(&on_result, (tcp_listener, result));
            }
            MioEffectfulAction::TcpAccept {
                connection,
                tcp_listener,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    TcpAcceptResult::Success // Ignored
                } else {
                    self.tcp_accept(connection, &tcp_listener)
                };

                dispatcher.dispatch_back(&on_result, (connection, result));
            }
            MioEffectfulAction::TcpConnect {
                connection,
                address,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    Ok(()) // Ignored
                } else {
                    self.tcp_connect(connection, address)
                };

                dispatcher.dispatch_back(&on_result, (connection, result));
            }
            MioEffectfulAction::TcpClose {
                connection,
                on_result,
            } => {
                if !dispatcher.is_replayer() {
                    self.tcp_close(&connection);
                }

                dispatcher.dispatch_back(&on_result, connection);
            }
            MioEffectfulAction::TcpWrite {
                uid,
                connection: connection_uid,
                data,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    TcpWriteResult::WrittenAll // Ignored
                } else {
                    self.tcp_write(&connection_uid, &data)
                };

                dispatcher.dispatch_back(&on_result, (uid, result));
            }
            MioEffectfulAction::TcpRead {
                uid,
                connection,
                len,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    TcpReadResult::ReadAll(Vec::new()) // Ignored
                } else {
                    self.tcp_read(&connection, len)
                };

                dispatcher.dispatch_back(&on_result, (uid, result));
            }
            MioEffectfulAction::TcpGetPeerAddress {
                connection,
                on_result,
            } => {
                let result = if dispatcher.is_replayer() {
                    Ok(String::new()) // Ignored
                } else {
                    self.tcp_peer_address(&connection)
                };

                dispatcher.dispatch_back(&on_result, (connection, result));
            }
        }
    }
}
