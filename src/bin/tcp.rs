use ipc::{flags, Result};
use std::env;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
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

    let sem = ipc::sem::Semaphore::open("/sem_test", flags::O_CREAT | flags::O_RDWR, 0o666, 0)?;

    match ipc::fork()? {
        0 => {
            let listener = TcpListener::bind("0.0.0.0:18899")?;
            sem.post()?;

            let (mut tcp, _) = listener.accept()?;
            let mut sum: isize = 0;
            loop {
                let n = tcp.read(&mut buf)? as isize;
                if n == 0 {
                    break;
                }
                sum += n;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }
        pid => {
            sem.wait()?;
            sem.unlink_self()?;

            let mut tcp = TcpStream::connect("0.0.0.0:18899")?;
            let start = Instant::now();
            for _ in 0..count {
                if tcp.write(&buf)? != buf.len() {
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
            // 防止死锁
            tcp.shutdown(Shutdown::Both)?;

            ipc::waitpid(pid, 0)?;
        }
    }

    Ok(())
}
