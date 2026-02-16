use rat_common::messages::{ClientMessage, ServerMessage, SystemInfo};
use sysinfo::System;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:12110").await?;
    let mut buffer = vec![0u8; 1024];

    stream.writable().await?;

    let message = ClientMessage::SystemInfoUpdate {
        info: SystemInfo {
            name: System::name().unwrap_or_default(),
            kernel_version: System::kernel_version().unwrap_or_default(),
            os_version: System::os_version().unwrap_or_default(),
            host_name: System::host_name().unwrap_or_default(),
        },
    };
    let bytes = postcard::to_allocvec_cobs(&message)?;
    stream.write_all(&bytes).await?;

    stream.readable().await?;

    loop {
        let n = stream.read(&mut buffer).await?;
        let message = postcard::from_bytes_cobs::<ServerMessage>(&mut buffer[..n])?;
        println!("{message:?}");
    }
}
