use std::io;
use std::io::Read;

use named_pipe::PipeOptions;

const VM_PIPE_NAME: &str = r"\\.\pipe\vm-log";

fn main() -> io::Result<()> {
    let server = PipeOptions::new(VM_PIPE_NAME).single()?;
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
