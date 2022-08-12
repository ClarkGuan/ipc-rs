use ipc::{flags, Result};
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process;
use std::time::Instant;

fn main() -> Result<()> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("wrong argument count (< 3)");
        process::exit(1);
    }

    let size: isize = args[1].parse()?;
    let count: isize = args[2].parse()?;

    let mut buf = Vec::with_capacity(size as _);
    buf.resize(buf.capacity(), 0);

    let (mut stream1, mut stream2) = UnixStream::pair()?;

    match ipc::fork()? {
        0 => {
            let mut sum: isize = 0;
            for _ in 0..count {
                sum += stream1.read(&mut buf)? as isize;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }

        pid => {
            let start = Instant::now();
            for _ in 0..count {
                if stream2.write(&buf)? != buf.len() {
                    eprintln!("write error");
                    process::exit(1);
                }
            }
            let duration = Instant::now().duration_since(start);
            let sec = duration.as_micros() as f64 / 1000000f64;
            println!(
                "{:.0} MB/s\t{:.0} msgs/s",
                (size * count) as f64 / sec / (1024 * 1024) as f64,
                count as f64 / sec
            );

            ipc::waitpid(pid, flags::WNOHANG)?;
        }
    }

    Ok(())
}
