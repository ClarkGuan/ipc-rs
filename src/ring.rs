use crate::sem::RawSemaphore;
use crate::Result;
use libc::c_void;
use std::ffi::CString;
use std::io::{Read, Write};
use std::{cmp, intrinsics, mem, ptr};

#[repr(C)]
struct Header {
    head: usize,
    tail: usize,
    size: usize,
    sem_reader: RawSemaphore,
    sem_writer: RawSemaphore,
}

impl Header {
    fn init(&mut self, size: usize) {
        self.head = 0;
        self.tail = 0;
        self.size = size;
        self.sem_writer.init(0);
        self.sem_reader.init(0);
    }
}

pub struct Buffer {
    header: *mut Header,
    data: *mut u8,
    master: bool,
    shm_name: String,
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    const HEADER_SIZE: usize = mem::size_of::<Header>();

    pub fn new(name: &str, master: bool, size: usize) -> Result<Buffer> {
        let size = size + 1;
        unsafe {
            let cstr = CString::new(name).expect("CString::new");
            let flags = if master {
                libc::O_RDWR | libc::O_CREAT
            } else {
                libc::O_RDWR
            };
            let shm_fd = libc::shm_open(cstr.as_ptr(), flags, 0o666);
            if shm_fd == -1 {
                return_errno!("shm_open");
            }
            let total_size = size + Self::HEADER_SIZE;
            if libc::ftruncate64(shm_fd, total_size as _) == -1 {
                libc::close(shm_fd);
                return_errno!("ftruncate64");
            }
            let addr = libc::mmap(
                ptr::null_mut::<c_void>(),
                total_size as _,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                shm_fd,
                0,
            );
            if addr == libc::MAP_FAILED {
                libc::close(shm_fd);
                return_errno!("mmap");
            }

            if libc::close(shm_fd) == -1 {
                return_errno!("close");
            }

            let header = addr as *mut Header;
            let mut buffer = Buffer {
                header,
                data: (addr as *mut u8).add(Self::HEADER_SIZE),
                master,
                shm_name: name.to_owned(),
            };
            buffer.as_header_mut().init(size);
            Ok(buffer)
        }
    }

    fn as_header_mut(&mut self) -> &mut Header {
        unsafe { &mut (*self.header) }
    }

    fn as_header(&self) -> &Header {
        unsafe { &(*self.header) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.header);
            if libc::munmap(
                self.header as _,
                (self.as_header().size + Self::HEADER_SIZE) as _,
            ) == -1
            {
                panic_errno!("munmap");
            }
            if self.master {
                let c_string = CString::new(&*self.shm_name).expect("CString::new");
                if libc::shm_unlink(c_string.as_ptr()) == -1 {
                    panic_errno!("shm_unlink");
                }
            }
        }
    }
}

impl Read for Buffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        unsafe {
            let head = intrinsics::atomic_load(&self.as_header().head);
            let tail = intrinsics::atomic_load(&self.as_header().tail);

            return if head == tail {
                Ok(0)
            } else if head < tail {
                let need_copy = cmp::min(buf.len(), tail - head);
                ptr::copy(self.data.add(head), buf.as_mut_ptr(), need_copy);
                intrinsics::atomic_xadd(&mut self.as_header_mut().head, need_copy);
                Ok(need_copy)
            } else {
                let size = self.as_header().size;
                let need_copy = cmp::min(tail + size - head, buf.len());
                if need_copy <= size - head {
                    ptr::copy(self.data.add(head), buf.as_mut_ptr(), need_copy);
                    intrinsics::atomic_xadd(&mut self.as_header_mut().head, need_copy);
                } else {
                    let first_write = size - head;
                    ptr::copy(self.data.add(head), buf.as_mut_ptr(), first_write);
                    let left = need_copy - first_write;
                    ptr::copy(self.data, buf.as_mut_ptr().add(first_write), left);
                    intrinsics::atomic_store(&mut self.as_header_mut().head, left);
                }
                Ok(need_copy)
            };
        }
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        unsafe {
            let head = intrinsics::atomic_load(&self.as_header().head);
            let tail = intrinsics::atomic_load(&self.as_header().tail);

            return if head > 0 && tail == head - 1 {
                Ok(0)
            } else if tail < head {
                let need_write = cmp::min(buf.len(), head - tail - 1);
                ptr::copy(
                    buf.as_ptr(),
                    self.data.add(self.as_header().tail),
                    need_write,
                );
                intrinsics::atomic_xadd(&mut self.as_header_mut().tail, need_write);
                Ok(need_write)
            } else {
                let size = self.as_header().size;
                let need_write = cmp::min(buf.len(), size + head - 1 - tail);
                if need_write <= size - tail {
                    ptr::copy(
                        buf.as_ptr(),
                        self.data.add(self.as_header().tail),
                        need_write,
                    );
                    intrinsics::atomic_xadd(&mut self.as_header_mut().tail, need_write);
                } else {
                    let first_copy = size - tail;
                    ptr::copy(
                        buf.as_ptr(),
                        self.data.add(self.as_header().tail),
                        first_copy,
                    );
                    let left = need_write - first_copy;
                    ptr::copy(buf.as_ptr().add(first_copy), self.data, left);
                    intrinsics::atomic_store(&mut self.as_header_mut().tail, left);
                }
                Ok(need_write)
            };
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
