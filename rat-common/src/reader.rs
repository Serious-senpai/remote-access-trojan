use tokio::io::AsyncReadExt;

pub struct Reader<R>
where
    R: AsyncReadExt + Unpin,
{
    _buffer: [u8; 1024],
    pub stream: R,
}

impl<R> Reader<R>
where
    R: AsyncReadExt + Unpin,
{
    pub fn new(stream: R) -> Self {
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
