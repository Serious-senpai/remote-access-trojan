mod modules;

use std::ffi::{CStr, c_char, c_int};
use std::fs::OpenOptions;
use std::sync::{Arc, mpsc};
use std::{ptr, thread};

use log::{LevelFilter, error};
use rat_common::module::Module;
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use tokio::runtime;

use crate::modules::server::Server;

struct _Server {
    pub module: Arc<Server>,
    pub worker: thread::JoinHandle<()>,
}

pub struct ServerHandle {
    _private: [u8; 0],
}

/// # Safety
/// The `log_path` pointer must be a valid null-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn initialize_logger(level: c_int, log_path: *const c_char) -> c_int {
    let level = match level {
        0 => LevelFilter::Off,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        5 => LevelFilter::Trace,
        _ => return 1,
    };
    let log_path = unsafe { CStr::from_ptr(log_path) }
        .to_string_lossy()
        .to_string();

    let config = ConfigBuilder::new()
        .set_location_level(level)
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    match OpenOptions::new().append(true).create(true).open(log_path) {
        Ok(file) => {
            if let Err(e) = CombinedLogger::init(vec![
                WriteLogger::new(level, config.clone(), file),
                TermLogger::new(level, config, TerminalMode::Stderr, ColorChoice::Auto),
            ]) {
                eprintln!("Failed to initialize logger: {e}");
                return 1;
            };

            0
        }
        Err(e) => {
            eprintln!("Failed to open log file: {e}");
            1
        }
    }
}

fn _report_initialization<T>(send: &mpsc::SyncSender<T>, value: T) {
    if let Err(e) = send.send(value) {
        error!("Failed to send initialization result back: {e}");
    }
}

/// # Safety
/// The `address` pointer must be a valid null-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start_server(address: *const c_char) -> *mut ServerHandle {
    let address = unsafe { CStr::from_ptr(address) }
        .to_string_lossy()
        .to_string();

    let (send, receive) = mpsc::sync_channel(1);
    let worker =
        thread::spawn(
            move || match runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(runtime) => {
                    runtime.block_on(async move {
                        match Server::bind(address).await {
                            Ok(server) => {
                                _report_initialization(&send, Some(server.clone()));
                                if let Err(e) = server.run().await {
                                    error!("Server runtime error: {e}");
                                }
                            }
                            Err(e) => {
                                error!("Failed to bind server: {e}");
                                _report_initialization(&send, None);
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to initialize Tokio runtime: {e}");
                    _report_initialization(&send, None);
                }
            },
        );

    let received = receive.recv().unwrap_or_else(|e| {
        error!("Failed to receive initialization result: {e}");
        None
    });

    match received {
        Some(server) => Box::into_raw(Box::new(_Server {
            module: server,
            worker,
        })) as *mut ServerHandle,
        None => ptr::null_mut(),
    }
}

/// # Safety
/// The `server` pointer must be a pointer returned by [`start_server`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stop_server(server: *mut ServerHandle) {
    let server = server as *mut _Server;
    if server.is_null() {
        return;
    }

    let server = unsafe { Box::from_raw(server) };
    server.module.stop();

    if let Err(e) = server.worker.join() {
        error!("Failed to join Tokio runtime thread: {e:?}");
    }
}
