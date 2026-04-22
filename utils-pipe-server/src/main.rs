use std::io;
use std::io::Read;

use clap::{Parser, crate_description, crate_version};
use named_pipe::PipeOptions;

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
struct _Arguments {
    /// Name of the pipe to listen on.
    #[arg(default_value = r"\\.\pipe\vm-log")]
    pub pipe_name: String,
}

fn main() -> io::Result<()> {
    let arguments = _Arguments::parse();

    let server = PipeOptions::new(arguments.pipe_name).single()?;
    println!("Waiting for VM to connect...");

    let mut pipe = server.wait()?;
    println!("VM connected!");

    let mut buf = [0u8; 1024];
    loop {
        let n = pipe.read(&mut buf)?;
        if n > 0 {
            print!("{}", String::from_utf8_lossy(&buf[..n]));
        }
    }
}
