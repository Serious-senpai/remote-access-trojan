pub mod cli;
pub mod client;
#[cfg(windows)]
pub mod service;
pub mod sessions;

pub type UniversalSocketAddr = String;
