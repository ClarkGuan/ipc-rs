use fast_log::Config;
use ipc::sem::Semaphore;
use ipc::{flags, Result};
use log::{error, info};
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use std::{env, thread};
use std::{fs, process};

fn init_logger(path: &str) {
    let _ = fs::remove_file(path);
    fast_log::init(Config::new().file(path)).expect("fast_log::init");
}

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
            init_logger("udp_svr.log");

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
                        fast_log::flush().expect("fast_log::flush");
                        eprintln!("server error happen!!");
                        thread::sleep(Duration::from_secs(3));
                        return Err(err.into());
                    }
                }
            }
            if sum != count * size {
                error!("sum error: {} != {}", sum, count * size);
            }
        }
        pid => {
            init_logger("udp_client.log");

            // wait for peer to start
            sem.wait()?;
            sem.unlink_self()?;

            let udp_cli = loop {
                match UdpSocket::bind("127.0.0.1:12345") {
                    Ok(udp) => break udp,
                    _ => {
                        error!("UDP client retry binding");
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            };
            udp_cli.connect("127.0.0.1:18899")?;
            let start = Instant::now();
            for c in 0..count {
                info!("client prepare to send: {}", c);
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
                        fast_log::flush().expect("fast_log::flush");
                        eprintln!("client error happen!!");
                        return Err(err.into());
                    }
                }
            }
            info!("client finish sending: {:?}", Instant::now());
            let duration = Instant::now().duration_since(start);
            let sec = duration.as_micros() as f64 / 1000000f64;
            println!(
                "{:.0} MB/s\t{:.0} msgs/s",
                (size * count) as f64 / sec / (1024 * 1024) as f64,
                count as f64 / sec
            );

            ipc::waitpid(pid, flags::WNOHANG)?;
            info!("parent exit! child pid: {}", pid);
        }
    }

    Ok(())
}
