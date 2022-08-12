use ipc::sem::Semaphore;
use ipc::{flags, Result};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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

    let path = "./uds-test";
    let sem = Semaphore::open("/sem_test", flags::O_RDWR | flags::O_CREAT, 0o666, 0)?;

    match ipc::fork()? {
        0 => {
            let listener = UnixListener::bind(path)?;
            sem.post()?;

            let (mut stream, _) = listener.accept()?;
            let mut sum: isize = 0;
            for _ in 0..count {
                sum += stream.read(&mut buf)? as isize;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }

        pid => {
            sem.wait()?;
            sem.unlink_self()?;

            let mut stream = UnixStream::connect(path)?;
            let start = Instant::now();
            for _ in 0..count {
                if stream.write(&buf)? != buf.len() {
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
