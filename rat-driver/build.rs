use std::env;
use std::time::Instant;

fn main() -> Result<(), wdk_build::ConfigError> {
    unsafe {
        env::set_var("REBUILD", format!("{:?}", Instant::now()));
    }
    println!("cargo:rerun-if-env-changed=REBUILD");

    wdk_build::configure_wdk_binary_build()
}
