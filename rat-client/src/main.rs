use rat_common::messages::{ClientMessage, ServerMessage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = TcpStream::connect("localhost:8000").await?;
    let mut buffer = vec![0u8; 1024];
    stream.readable().await?;

    loop {
        let n = stream.read(&mut buffer).await?;
        let message = postcard::from_bytes_cobs::<ServerMessage>(&mut buffer[..n])?;
        println!("{message:?}");

        match message {
            ServerMessage::Ping { value } => {
                let response = ClientMessage::Pong { value: value + 1 };
                let bytes = postcard::to_stdvec_cobs(&response)?;
                stream.write_all(&bytes).await?;
            }
        }
    }
}
