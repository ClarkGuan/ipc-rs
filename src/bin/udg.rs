use ipc::sem::Semaphore;
use ipc::{flags, Result};
use std::os::unix::net::UnixDatagram;
use std::time::Instant;
use std::{env, fs, process};

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
    let sem = Semaphore::open("/sem_test", flags::O_CREAT | flags::O_RDWR, 0o666, 0)?;

    match ipc::fork()? {
        0 => {
            let datagram = UnixDatagram::bind(path)?;
            sem.post()?;

            let mut sum: isize = 0;
            for _ in 0..count {
                sum += datagram.recv(&mut buf)? as isize;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }

        pid => {
            sem.wait()?;
            sem.unlink_self()?;

            let datagram = UnixDatagram::unbound()?;
            datagram.connect(path)?;
            let start = Instant::now();
            for _ in 0..count {
                if datagram.send(&buf)? != buf.len() {
                    eprintln!("write error");
                    process::exit(1);
                }
            }
            let duration = start.elapsed();
            let sec = duration.as_micros() as f64 / 1000000f64;
            println!(
                "{:.0} MB/s\t{:.0} msgs/s",
                (size * count) as f64 / sec / (1024 * 1024) as f64,
                count as f64 / sec
            );

            ipc::waitpid(pid, 0)?;
            let _ = fs::remove_file(path);
        }
    }

    Ok(())
}
