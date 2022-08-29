use ipc::sem::{Semaphore, SemaphoreLike};
use ipc::{flags, Result};
use log::{error, info};
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
            info!("pid: {}", ipc::getpid());

            let mut sum: isize = 0;
            let udp_svr = loop {
                match UdpSocket::bind("127.0.0.1:18899") {
                    Ok(udp) => break udp,
                    _ => {
                        error!("UDP server retry binding");
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            };
            // notify parent process
            sem.post()?;
            // 超时时间到达后，无论如何都要退出
            // 有可能出现丢包，造成没读到 count 个包无法退出循环
            udp_svr.set_read_timeout(Some(Duration::from_secs(1)))?;
            for c in 0..count {
                match udp_svr.recv(&mut buf) {
                    Ok(n) => sum += n as isize,
                    Err(err) => {
                        error!(
                            "IO error kind: {:?}, count: {}, instant: {:?}",
                            err.kind(),
                            c,
                            Instant::now()
                        );
                        eprintln!(
                            "Packet loss found when recv, expect {} actually {}",
                            count, c
                        );
                        return Err(err.into());
                    }
                }
            }
            if sum != count * size {
                error!("sum error: {} != {}", sum, count * size);
            }
        }
        pid => {
            info!("pid: {}", ipc::getpid());

            // wait for peer to start
            sem.wait()?;
            sem.unlink_self()?;

            let udp_cli = loop {
                match UdpSocket::bind("127.0.0.1:0") {
                    Ok(udp) => break udp,
                    _ => {
                        error!("UDP client retry binding");
                    }
                }
            };
            udp_cli.connect("127.0.0.1:18899")?;
            let start = Instant::now();
            for c in 0..count {
                match udp_cli.send(&buf) {
                    Ok(n) => {
                        if n != buf.len() {
                            error!("write error");
                            process::exit(1);
                        }
                    }
                    Err(err) => {
                        error!(
                            "IO error kind: {:?}, count: {}, instant: {:?}",
                            err.kind(),
                            c,
                            Instant::now()
                        );
                        eprintln!("client error happen!!");
                        return Err(err.into());
                    }
                }
            }
            let duration = start.elapsed();
            let sec = duration.as_micros() as f64 / 1000000f64;
            println!(
                "{:.0} MB/s\t{:.0} msgs/s",
                (size * count) as f64 / sec / (1024 * 1024) as f64,
                count as f64 / sec
            );

            let (pid, status) = ipc::waitpid(pid, 0)?;
            info!("parent exit! child pid: {}, status: {}", pid, status);
        }
    }

    Ok(())
}
