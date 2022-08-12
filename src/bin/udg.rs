use ipc::{flags, Result};
use std::os::unix::net::UnixDatagram;
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};

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

    let path = "./udg-test";

    match ipc::fork()? {
        0 => {
            let datagram = UnixDatagram::bind(path)?;
            let mut sum: isize = 0;
            for _ in 0..count {
                sum += datagram.recv(&mut buf)? as isize;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }

        pid => {
            // waiting for the pear to bind
            thread::sleep(Duration::from_secs(1));

            let datagram = UnixDatagram::unbound()?;
            datagram.connect(path)?;
            let start = Instant::now();
            for _ in 0..count {
                if datagram.send(&buf)? != buf.len() {
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
            let _ = fs::remove_file(path);
        }
    }

    Ok(())
}
