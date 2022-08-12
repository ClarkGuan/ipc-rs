use ipc::sem::Semaphore;
use ipc::{flags, Result};
use std::net::UdpSocket;
use std::process;
use std::time::{Duration, Instant};
use std::{env, thread};

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

    let sem = Semaphore::open("/sem_test", flags::O_RDWR | flags::O_CREAT, 0o666, 0)?;

    match ipc::fork()? {
        0 => {
            let mut sum: isize = 0;
            let udp_svr = loop {
                match UdpSocket::bind("0.0.0.0:18899") {
                    Ok(udp) => break udp,
                    _ => {
                        eprintln!("start to retry");
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            };
            // notify parent process
            sem.post()?;
            // 超时 5s，无论如何都要退出
            // 有可能出现丢包，造成没读到 count 个包无法退出循环
            udp_svr.set_read_timeout(Some(Duration::from_secs(2)))?;
            for _ in 0..count {
                sum += udp_svr.recv(&mut buf)? as isize;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }
        pid => {
            // wait for peer to start
            sem.wait()?;
            sem.unlink_self()?;

            let udp_cli = UdpSocket::bind("0.0.0.0:12345")?;
            udp_cli.connect("0.0.0.0:18899")?;
            let start = Instant::now();
            for _ in 0..count {
                if udp_cli.send(&buf)? != buf.len() {
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
