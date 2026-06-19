use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub heartbeat_interval: Duration,
    pub request_timeout: Duration,
    pub client_mpsc_channel_capacity: usize,
    pub frontend_static_files: Arc<PathBuf>,
    pub static_files_dir: Arc<PathBuf>,
    pub tls_admin_cert_path: Arc<PathBuf>,
    pub tls_admin_key_path: Arc<PathBuf>,
    pub tls_cc_cert_path: Arc<PathBuf>,
    pub tls_cc_key_path: Arc<PathBuf>,
}
