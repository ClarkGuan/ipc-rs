use crate::futex;
use crate::futex::WaitResult;
use crate::shm::Shm;
use crate::Result;
use std::io::{Read, Write};
use std::{cmp, intrinsics, io, mem};

#[repr(transparent)]
#[derive(Debug)]
struct Cond {
    val: i32,
}

impl Cond {
    fn init(&mut self) {
        self.val = 0;
    }

    fn wait(&mut self) {
        unsafe {
            let (_, ret) = intrinsics::atomic_cxchg(&mut self.val, 0, 1);
            assert!(ret);
            match futex::futex_wait(&self.val, 1) {
                Ok(WaitResult::ValNotEqual) | Err(_) => panic!("futex_wait error"),
                _ => (),
            }
        }
    }

    fn notify(&mut self) {
        unsafe {
            let (old, ret) = intrinsics::atomic_cxchg(&mut self.val, 1, 0);
            if old == 0 {
                return;
            }
            assert!(ret);
            assert_eq!(futex::futex_wake(&self.val, 1).unwrap(), 1);
        }
    }
}

#[repr(C)]
struct Header {
    head: usize,
    tail: usize,
    size: usize,
    reader: Cond,
    writer: Cond,
}

impl Header {
    fn init(&mut self, size: usize) {
        self.head = 0;
        self.tail = 0;
        self.size = size;
        self.reader.init();
        self.writer.init();
    }

    fn head(&self) -> usize {
        unsafe { intrinsics::atomic_load(&self.head) }
    }

    fn tail(&self) -> usize {
        unsafe { intrinsics::atomic_load(&self.tail) }
    }

    fn set_head(&mut self, val: usize) {
        unsafe {
            intrinsics::atomic_store(&mut self.head, val);
        }
    }

    fn set_tail(&mut self, val: usize) {
        unsafe {
            intrinsics::atomic_store(&mut self.tail, val);
        }
    }

    fn reader_wait(&mut self) {
        self.reader.wait()
    }

    fn writer_wait(&mut self) {
        self.writer.wait()
    }

    fn reader_notify(&mut self) {
        self.reader.notify()
    }

    fn writer_notify(&mut self) {
        self.writer.notify()
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

    pub fn new(name: &str, master: bool, size: usize) -> Result<Buffer> {
        let size = Self::HEADER_SIZE + size * 2 + 1;
        let mut buf = Buffer(Shm::open(name, size, master)?);
        buf.header_mut().init(size * 2 + 1);
        Ok(buf)
    }
}

impl Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let head = self.header().head();
            let tail = self.header().tail();

            return if head == tail {
                self.header_mut().reader_wait();
                continue;
            } else if head < tail {
                let need_copy = cmp::min(buf.len(), tail - head);
                copy(&&self.data()[head..], &mut &mut buf[..need_copy])?;
                self.header_mut().set_head(head + need_copy);
                self.header_mut().writer_notify();
                Ok(need_copy)
            } else {
                let size = self.header().size;
                let need_copy = cmp::min(tail + size - head, buf.len());
                if need_copy <= size - head {
                    copy(&&self.data_mut()[head..], &mut &mut buf[..need_copy])?;
                    self.header_mut().set_head(head + need_copy);
                } else {
                    let first_write = size - head;
                    copy(&&self.data_mut()[head..], &mut &mut buf[..first_write])?;
                    let left = need_copy - first_write;
                    copy(&&self.data_mut()[..left], &mut &mut buf[first_write..])?;
                    self.header_mut().set_head(left);
                }
                self.header_mut().writer_notify();
                Ok(need_copy)
            };
        }
    }
}

fn copy<R: Read + ?Sized, W: Write + ?Sized>(r: &R, w: &mut W) -> io::Result<u64> {
    #[allow(mutable_transmutes)]
    unsafe { io::copy::<R, W>(mem::transmute(r), w) }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            let head = self.header().head();
            let tail = self.header().tail();

            return if head > 0 && tail == head - 1 {
                self.header_mut().writer_wait();
                continue;
            } else if tail < head {
                let need_copy = cmp::min(buf.len(), head - tail - 1);
                copy(&&buf[..need_copy], &mut &mut self.data_mut()[tail..])?;
                self.header_mut().set_tail(tail + need_copy);
                self.header_mut().reader_notify();
                Ok(need_copy)
            } else {
                let size = self.header().size;
                let need_copy = cmp::min(buf.len(), size + head - 1 - tail);
                if need_copy <= size - tail {
                    copy(&&buf[..need_copy], &mut &mut self.data_mut()[tail..])?;
                    self.header_mut().set_tail(tail + need_copy);
                } else {
                    let first_copy = size - tail;
                    copy(&&buf[..first_copy], &mut &mut self.data_mut()[tail..])?;
                    let left = need_copy - first_copy;
                    copy(&&buf[first_copy..], &mut &mut self.data_mut()[..left])?;
                    self.header_mut().set_tail(left);
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
