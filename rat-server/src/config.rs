use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub heartbeat_interval: Duration,
    pub request_timeout: Duration,
}
