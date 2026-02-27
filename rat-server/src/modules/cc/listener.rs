use rat_common::schema::ClientMessage;
use tokio::sync::{mpsc, oneshot};

pub struct ClientOnceListener {
    pub predicate: Box<dyn Fn(&ClientMessage) -> bool + Send + Sync + 'static>,
    pub completer: oneshot::Sender<ClientMessage>,
}

pub struct ClientPersistentListener {
    pub predicate: Box<dyn Fn(&ClientMessage) -> bool + Send + Sync + 'static>,
    pub sender: mpsc::Sender<ClientMessage>,
}
