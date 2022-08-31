use ipc::flags;
use ipc::ring::Buffer;
use ipc::sem::{Semaphore, SemaphoreLike};
use ipc::Result;
use std::io::{Read, Write};
use std::time::Instant;
use std::{env, process};

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

    let sem = Semaphore::open("/sem_test", flags::O_CREAT | flags::O_RDWR, 0o666, 0)?;

    match ipc::fork()? {
        0 => {
            let mut ring_buf = Buffer::new("/shm_ring", true, size as _)?;
            sem.post();

            let mut sum: isize = 0;
            loop {
                let n = ring_buf.read(&mut buf)? as isize;
                sum += n;
                if sum == count * size {
                    break;
                }
            }
        }

        pid => {
            sem.wait();
            sem.unlink_self();

            let mut ring_buf = Buffer::new("/shm_ring", false, size as _)?;
            let start = Instant::now();
            for i in 0..count {
                println!("write count: {}", i+1);
                let mut tmp = &buf[..];
                while tmp.len() > 0 {
                    let n = ring_buf.write(tmp)?;
                    tmp = &tmp[n..];
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
        }
    }

    Ok(())
}
