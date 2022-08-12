use ipc::flags;
use ipc::mq::MessageQueue;
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

    let mut msg_queue = MessageQueue::open("/mq_test", flags::O_CREAT | flags::O_RDWR, 0o666)?;
    let mut attribute = msg_queue.attributes()?;
    attribute.set_max_message_count(10000);
    msg_queue.set_attributes(&attribute)?;

    let mut buf = Vec::with_capacity(attribute.message_size() as _);
    buf.resize(buf.capacity(), 0);

    match ipc::fork()? {
        0 => {
            let mut sum: isize = 0;
            for _ in 0..count {
                sum += msg_queue.read(&mut buf)? as isize;
            }
            if sum != count * size {
                eprintln!("sum error: {} != {}", sum, count * size);
            }
        }
        pid => {
            let start = Instant::now();
            for _ in 0..count {
                let tmp = &buf[..size as usize];
                if msg_queue.write(tmp)? != tmp.len() {
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
            msg_queue.unlink_self()?;
        }
    }

    Ok(())
}
