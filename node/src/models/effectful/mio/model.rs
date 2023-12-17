use super::action::MioOutputAction;
use super::state::MioState;
use crate::automaton::action::Dispatcher;
use crate::automaton::model::OutputModel;

impl OutputModel for MioState {
    type Action = MioOutputAction;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher) {
        match action {
            MioOutputAction::PollCreate { uid, on_completion } => {
                dispatcher.completion_dispatch(&on_completion, (uid, self.poll_create(uid)));
            }
            MioOutputAction::PollRegisterTcpServer {
                poll_uid,
                tcp_listener_uid,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        tcp_listener_uid,
                        self.poll_register_tcp_server(&poll_uid, tcp_listener_uid),
                    ),
                );
            }
            MioOutputAction::PollRegisterTcpConnection {
                poll_uid,
                connection_uid,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        connection_uid,
                        self.poll_register_tcp_connection(&poll_uid, connection_uid),
                    ),
                );
            }
            MioOutputAction::PollDeregisterTcpConnection {
                poll_uid,
                connection_uid,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        connection_uid,
                        self.poll_deregister_tcp_connection(&poll_uid, connection_uid),
                    ),
                );
            }
            MioOutputAction::PollEvents {
                uid,
                poll_uid,
                events_uid,
                timeout,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, self.poll_events(&poll_uid, &events_uid, timeout)),
                );
            }
            MioOutputAction::EventsCreate {
                uid,
                capacity,
                on_completion,
            } => {
                self.events_create(uid, capacity);
                dispatcher.completion_dispatch(&on_completion, uid);
            }
            MioOutputAction::TcpListen {
                uid,
                address,
                on_completion,
            } => {
                dispatcher
                    .completion_dispatch(&on_completion, (uid, self.tcp_listen(uid, address)));
            }
            MioOutputAction::TcpAccept {
                uid,
                listener_uid,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, self.tcp_accept(uid, &listener_uid)),
                );
            }
            MioOutputAction::TcpConnect {
                uid,
                address,
                on_completion,
            } => {
                dispatcher
                    .completion_dispatch(&on_completion, (uid, self.tcp_connect(uid, address)));
            }
            MioOutputAction::TcpClose {
                connection_uid,
                on_completion,
            } => {
                self.tcp_close(&connection_uid);
                dispatcher.completion_dispatch(&on_completion, connection_uid);
            }
            MioOutputAction::TcpWrite {
                uid,
                connection_uid,
                data,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, self.tcp_write(&connection_uid, &data)),
                );
            }
            MioOutputAction::TcpRead {
                uid,
                connection_uid,
                len,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, self.tcp_read(&connection_uid, len)),
                );
            }
            MioOutputAction::TcpGetPeerAddress {
                connection_uid,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (connection_uid, self.tcp_peer_address(&connection_uid)),
                );
            }
        }
    }
}
