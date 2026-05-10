#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod framework;
#[cfg(feature = "std")]
pub mod logger;
#[cfg(feature = "std")]
pub mod reader;
#[cfg(feature = "std")]
pub mod schema;
#[cfg(feature = "std")]
pub mod snowflake;
pub mod utils;
#[cfg(any(windows, target_os = "uefi"))]
pub mod windows;
