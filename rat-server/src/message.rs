use std::net::SocketAddr;

use rat_common::messages::ClientMessage;

#[derive(Debug)]
pub enum InternalMessage {
    Connect {
        peer: SocketAddr,
    },
    Disconnect {
        peer: SocketAddr,
    },
    Message {
        peer: SocketAddr,
        data: ClientMessage,
    },
}
