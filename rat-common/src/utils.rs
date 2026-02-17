use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::{Mutex, MutexGuard};

pub struct TcpReader {
    _buffer: [u8; 1024],
    pub stream: OwnedReadHalf,
}

impl TcpReader {
    pub fn new(stream: OwnedReadHalf) -> Self {
        Self {
            _buffer: [0; 1024],
            stream,
        }
    }

    pub async fn read(&mut self) -> anyhow::Result<usize> {
        Ok(self.stream.read(&mut self._buffer).await?)
    }

    pub fn prefix(&self, size: usize) -> &[u8] {
        &self._buffer[..size]
    }
}

pub fn acquire_free_mutex<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.try_lock()
        .expect("This mutex should never be contended")
}
