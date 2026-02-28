use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub heartbeat_interval: Duration,
    pub request_timeout: Duration,
    pub client_mpsc_channel_capacity: usize,
    pub frontend_static_files: Arc<PathBuf>,
}
