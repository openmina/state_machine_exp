use super::action::MioAction;
use super::state::MioState;
use crate::automaton::action::Dispatcher;
use crate::automaton::model::OutputModel;

impl OutputModel for MioState {
    type Action = MioAction;

    fn process_output(&mut self, action: Self::Action, dispatcher: &mut Dispatcher) {
        match action {
            MioAction::PollCreate { uid, on_completion } => {
                dispatcher.completion_dispatch(&on_completion, (uid, self.poll_create(uid)));
            }
            MioAction::PollRegisterTcpServer {
                poll_uid,
                tcp_listener_uid,
                token,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        token,
                        self.poll_register_tcp_server(&poll_uid, &tcp_listener_uid, token),
                    ),
                );
            }
            MioAction::PollRegisterTcpConnection {
                poll_uid,
                connection_uid,
                token,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        token,
                        self.poll_register_tcp_connection(&poll_uid, &connection_uid, token),
                    ),
                );
            }
            MioAction::PollEvents {
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
            MioAction::EventsCreate {
                uid,
                capacity,
                on_completion,
            } => {
                self.events_create(uid, capacity);
                dispatcher.completion_dispatch(&on_completion, uid);
            }
            MioAction::TcpListen {
                uid,
                address,
                on_completion,
            } => {
                dispatcher
                    .completion_dispatch(&on_completion, (uid, self.tcp_listen(uid, address)));
            }
            MioAction::TcpAccept {
                uid,
                listener_uid,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, self.tcp_accept(uid, &listener_uid)),
                );
            }
            MioAction::TcpWrite {
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
            MioAction::TcpRead {
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
        }
    }
}
