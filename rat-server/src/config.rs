use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub heartbeat_interval: Duration,
    pub request_timeout: Duration,
    pub client_mpsc_channel_capacity: usize,
}
