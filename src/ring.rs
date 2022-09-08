use crate::futex;
use crate::shm::Shm;
use crate::Result;
use std::io::{Read, Write};
use std::{cmp, intrinsics, io, mem};

#[derive(Debug)]
#[repr(C)]
struct Header {
    head: u32, // u32 为了与 futex 对齐
    tail: u32, // u32 为了与 futex 对齐
    size: u32,
}

impl Header {
    fn init(&mut self, size: u32) {
        self.head = 0;
        self.tail = 0;
        self.size = size;
    }

    fn head(&self) -> u32 {
        unsafe { intrinsics::atomic_load(&self.head) }
    }

    fn tail(&self) -> u32 {
        unsafe { intrinsics::atomic_load(&self.tail) }
    }

    fn set_head(&mut self, val: u32) {
        unsafe {
            intrinsics::atomic_store(&mut self.head, val);
        }
    }

    fn set_tail(&mut self, val: u32) {
        unsafe {
            intrinsics::atomic_store(&mut self.tail, val);
        }
    }

    fn reader_wait(&mut self, expect_tail: u32) {
        futex::futex_wait(&self.tail, expect_tail).expect("futex::futex_wait");
    }

    fn writer_wait(&mut self, expect_head: u32) {
        futex::futex_wait(&self.head, expect_head).expect("futex::futex_wait");
    }

    fn reader_notify(&mut self) {
        futex::futex_wake(&self.tail, 1).expect("futex::futex_wake");
    }

    fn writer_notify(&mut self) {
        futex::futex_wake(&self.head, 1).expect("futex::futex_wake");
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct Buffer(Shm);

impl Buffer {
    fn header(&self) -> &Header {
        unsafe { &*mem::transmute::<_, *const Header>(self.0.as_ptr()) }
    }

    fn header_mut(&mut self) -> &mut Header {
        unsafe { &mut *mem::transmute::<_, *mut Header>(self.0.as_mut_ptr()) }
    }

    fn data(&self) -> &[u8] {
        &self.0.as_slice()[Self::HEADER_SIZE..]
    }

    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.0.as_mut_slice()[Self::HEADER_SIZE..]
    }
}

impl Buffer {
    const HEADER_SIZE: usize = mem::size_of::<Header>();
    const MAX_LOCK_RETRY_COUNT: usize = 256;

    pub fn new(name: &str, master: bool, size: u32) -> Result<Buffer> {
        let data_size = size * 4 + 1; // 缓存大小直接影响读写效率
        let total_size = Self::HEADER_SIZE + data_size as usize;
        let mut buf = Buffer(Shm::open(name, total_size, master)?);
        if master {
            buf.header_mut().init(data_size); // 可能会溢出
        }
        Ok(buf)
    }
}

impl Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut retry_count = 0;
        loop {
            let head = self.header().head();
            let tail = self.header().tail();

            return if head == tail {
                if retry_count < Self::MAX_LOCK_RETRY_COUNT {
                    retry_count += 1;
                    continue;
                }
                self.header_mut().reader_wait(tail);
                continue;
            } else if head < tail {
                let need_copy = cmp::min(buf.len(), (tail - head) as usize);
                (&mut buf[..need_copy]).write(&self.data()[head as usize..])?;
                self.header_mut().set_head(head + need_copy as u32);
                self.header_mut().writer_notify();
                Ok(need_copy)
            } else {
                let size = self.header().size;
                let need_copy = cmp::min((tail + size - head) as usize, buf.len());
                if need_copy <= (size - head) as usize {
                    (&mut buf[..need_copy]).write(&self.data_mut()[head as usize..])?;
                    self.header_mut().set_head(head + need_copy as u32);
                } else {
                    let first_write = (size - head) as usize;
                    (&mut buf[..first_write]).write(&self.data_mut()[head as usize..])?;
                    let left = need_copy - first_write;
                    (&mut buf[first_write..]).write(&self.data_mut()[..left])?;
                    self.header_mut().set_head(left as _);
                }
                self.header_mut().writer_notify();
                Ok(need_copy)
            };
        }
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut retry_count = 0;
        loop {
            let head = self.header().head();
            let tail = self.header().tail();

            return if head > 0 && tail == head - 1 {
                if retry_count < Self::MAX_LOCK_RETRY_COUNT {
                    retry_count += 1;
                    continue;
                }
                self.header_mut().writer_wait(head);
                continue;
            } else if tail < head {
                let need_copy = cmp::min(buf.len(), (head - tail - 1) as usize);
                (&mut self.data_mut()[tail as usize..]).write(&buf[..need_copy])?;
                self.header_mut().set_tail(tail + need_copy as u32);
                self.header_mut().reader_notify();
                Ok(need_copy)
            } else {
                let size = self.header().size;
                let need_copy = cmp::min(buf.len(), (size + head - 1 - tail) as usize);
                if need_copy <= (size - tail) as usize {
                    (&mut self.data_mut()[tail as usize..]).write(&buf[..need_copy])?;
                    self.header_mut().set_tail(tail + need_copy as u32);
                } else {
                    let first_copy = (size - tail) as usize;
                    (&mut self.data_mut()[tail as usize..]).write(&buf[..first_copy])?;
                    let left = need_copy - first_copy;
                    (&mut self.data_mut()[..left]).write(&buf[first_copy..])?;
                    self.header_mut().set_tail(left as _);
                }
                self.header_mut().reader_notify();
                Ok(need_copy)
            };
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
