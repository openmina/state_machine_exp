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
                poll,
                tcp_listener,
                token,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        token,
                        self.poll_register_tcp_server(poll, tcp_listener, token),
                    ),
                );
            }
            MioAction::PollRegisterTcpConnection {
                poll,
                connection,
                token,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (
                        token,
                        self.poll_register_tcp_connection(poll, connection, token),
                    ),
                );
            }
            MioAction::PollEvents {
                poll,
                events,
                timeout,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (events, self.poll_events(poll, events, timeout)),
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
                listener,
                on_completion,
            } => {
                dispatcher
                    .completion_dispatch(&on_completion, (uid, self.tcp_accept(uid, listener)));
            }
            MioAction::TcpWrite {
                uid,
                connection,
                data,
                on_completion,
            } => {
                dispatcher
                    .completion_dispatch(&on_completion, (uid, self.tcp_write(connection, &data)));
            }
            MioAction::TcpRead {
                uid,
                connection,
                len_bytes,
                on_completion,
            } => {
                dispatcher.completion_dispatch(
                    &on_completion,
                    (uid, self.tcp_read(connection, len_bytes)),
                );
            }
        }
    }
}
