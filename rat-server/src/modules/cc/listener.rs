use rat_common::schema::ClientMessage;
use tokio::sync::oneshot;

pub struct ClientMessageListener {
    pub predicate: Box<dyn Fn(&ClientMessage) -> bool + Send + Sync + 'static>,
    pub completer: oneshot::Sender<ClientMessage>,
}
