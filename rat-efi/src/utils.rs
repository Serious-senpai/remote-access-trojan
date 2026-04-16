use core::time::Duration;

use log::info;
use uefi::boot;

pub fn countdown(seconds: u64) {
    for i in 0..seconds {
        info!("Counting down {} seconds...", seconds - i);
        boot::stall(Duration::from_secs(1));
    }
}

pub fn find_pattern(buffer: &[u8], pattern: &[u8]) -> Option<usize> {
    for index in 0..buffer.len().saturating_sub(pattern.len()) {
        if buffer[index..index + pattern.len()] == *pattern {
            return Some(index);
        }
    }

    None
}
