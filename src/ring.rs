use crate::futex;
use crate::shm::Shm;
use crate::Result;
use std::io::{Read, Write};
use std::{cmp, intrinsics, io, mem, ptr};

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

    pub fn new(name: &str, master: bool, size: u32) -> Result<Buffer> {
        let total_size = Self::HEADER_SIZE + size as usize * 2 + 1;
        let mut buf = Buffer(Shm::open(name, total_size, master)?);
        if master {
            buf.header_mut().init(size * 2 + 1); // 可能会溢出
        }
        Ok(buf)
    }
}

impl Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let head = self.header().head();
            let tail = self.header().tail();

            return if head == tail {
                self.header_mut().reader_wait(tail);
                continue;
            } else if head < tail {
                let need_copy = cmp::min(buf.len(), (tail - head) as usize);
                copy(&self.data()[head as usize..], &mut buf[..need_copy]);
                self.header_mut().set_head(head + need_copy as u32);
                self.header_mut().writer_notify();
                Ok(need_copy)
            } else {
                let size = self.header().size;
                let need_copy = cmp::min((tail + size - head) as usize, buf.len());
                if need_copy <= (size - head) as usize {
                    copy(&self.data_mut()[head as usize..], &mut buf[..need_copy]);
                    self.header_mut().set_head(head + need_copy as u32);
                } else {
                    let first_write = (size - head) as usize;
                    copy(&self.data_mut()[head as usize..], &mut buf[..first_write]);
                    let left = need_copy - first_write;
                    copy(&self.data_mut()[..left], &mut buf[first_write..]);
                    self.header_mut().set_head(left as _);
                }
                self.header_mut().writer_notify();
                Ok(need_copy)
            };
        }
    }
}

// fn copy<R: Read + ?Sized, W: Write + ?Sized>(r: &R, w: &mut W) -> io::Result<u64> {
//     #[allow(mutable_transmutes)]
//     unsafe { io::copy::<R, W>(mem::transmute(r), w) }
// }

fn copy<R: AsRef<[u8]>, W: AsMut<[u8]>>(src: R, mut dst: W) -> u64 {
    let r = src.as_ref();
    let w = dst.as_mut();
    let max = cmp::min(r.len(), w.len());
    unsafe {
        ptr::copy(r.as_ptr(), w.as_mut_ptr(), max);
    }
    max as _
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            let head = self.header().head();
            let tail = self.header().tail();

            return if head > 0 && tail == head - 1 {
                self.header_mut().writer_wait(head);
                continue;
            } else if tail < head {
                let need_copy = cmp::min(buf.len(), (head - tail - 1) as usize);
                copy(&buf[..need_copy], &mut self.data_mut()[tail as usize..]);
                self.header_mut().set_tail(tail + need_copy as u32);
                self.header_mut().reader_notify();
                Ok(need_copy)
            } else {
                let size = self.header().size;
                let need_copy = cmp::min(buf.len(), (size + head - 1 - tail) as usize);
                if need_copy <= (size - tail) as usize {
                    copy(&buf[..need_copy], &mut self.data_mut()[tail as usize..]);
                    self.header_mut().set_tail(tail + need_copy as u32);
                } else {
                    let first_copy = (size - tail) as usize;
                    copy(&buf[..first_copy], &mut self.data_mut()[tail as usize..]);
                    let left = need_copy - first_copy;
                    copy(&buf[first_copy..], &mut self.data_mut()[..left]);
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
