use std::fmt::Display;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SnowflakeId(u128);

static _COUNTER: AtomicU32 = AtomicU32::new(0);

impl SnowflakeId {
    const _TAIL_BITS: u32 = 32;
    const _TIMESTAMP_BITS: u32 = 128 - Self::_TAIL_BITS;

    /// Unix timestamp for 2026-01-01T00:00:00Z
    const _EPOCH_2026_MS: u128 = 1767225600000;

    pub fn new() -> Self {
        let elapsed_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            - Self::_EPOCH_2026_MS;

        let time_part = elapsed_ms << Self::_TAIL_BITS;
        let counter = _COUNTER.fetch_add(1, Ordering::Relaxed);

        Self(time_part | u128::from(counter))
    }
}

impl Display for SnowflakeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
