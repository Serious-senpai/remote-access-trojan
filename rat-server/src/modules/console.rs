use std::io::Write;
use std::sync::{Arc, Weak};
use std::{io, mem};

use async_trait::async_trait;
use rat_common::module::{Module, ModuleState};
use rat_common::utils::acquire_free_mutex;
use tokio::io::{AsyncReadExt, Stdin, stdin};
use tokio::sync::Mutex;

use crate::modules::server::Server;

pub struct Console {
    _server: Weak<Server>,
    _stdin: Mutex<Stdin>,
    _current_buffer: Mutex<Vec<u8>>,
    _state: Arc<ModuleState>,
}

impl Console {
    pub fn new(server: Weak<Server>) -> Self {
        Self {
            _server: server,
            _stdin: Mutex::new(stdin()),
            _current_buffer: Mutex::new(vec![]),
            _state: Arc::new(ModuleState::new()),
        }
    }

    pub async fn process_command(&self, line: &str) {}
}

#[async_trait]
impl Module for Console {
    type EventType = io::Result<Vec<u8>>;

    fn name(&self) -> &str {
        "Console"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: std::sync::Arc<Self>) -> Self::EventType {
        print!("rat-server>");
        let _ = io::stdout().flush();

        let mut stdin = acquire_free_mutex(&self._stdin);
        let mut buffer = [0u8; 512];
        let n = stdin.read(&mut buffer).await?;
        Ok(buffer[..n].to_vec())
    }

    async fn handle(self: std::sync::Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let new_data = match event {
            Ok(d) => d,
            Err(e) => {
                self.stop();
                return Err(e.into());
            }
        };
        let mut buffer = acquire_free_mutex(&self._current_buffer);

        for byte in new_data {
            buffer.push(byte);
            if byte == b'\n' {
                let mut new_buffer = vec![];
                mem::swap(&mut new_buffer, &mut buffer);

                match String::from_utf8(new_buffer) {
                    Ok(command) => self.process_command(command.trim()).await,
                    Err(e) => {
                        eprintln!("Invalid UTF-8 sequence: {e}");
                    }
                }
            }
        }

        Ok(())
    }
}
