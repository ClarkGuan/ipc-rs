use crate::futex;
use crate::futex::WaitResult;
use crate::Result;
use libc::c_void;
use std::ffi::CString;
use std::io::{Read, Write};
use std::{cmp, intrinsics, mem, ptr};

#[repr(transparent)]
#[derive(Debug)]
struct Cond {
    val: i32,
}

impl Cond {
    fn init(&self) {
        unsafe {
            intrinsics::atomic_store(mem::transmute(&self.val), 0);
        }
    }

    fn wait(&self) {
        unsafe {
            let (_, ret) = intrinsics::atomic_cxchg(mem::transmute(&self.val), 0, 1);
            assert!(ret);
            match futex::futex_wait(&self.val, 1) {
                Ok(WaitResult::ValNotEqual) | Err(_) => panic!("futex_wait error"),
                _ => (),
            }
        }
    }

    fn notify(&self) {
        unsafe {
            let (old, ret) = intrinsics::atomic_cxchg(mem::transmute(&self.val), 1, 0);
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
        let size = size * 2 + 1;
        unsafe {
            let cstr = CString::new(name).expect("CString::new");
            let flags = if master {
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL
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
            loop {
                let head = intrinsics::atomic_load(&self.as_header().head);
                let tail = intrinsics::atomic_load(&self.as_header().tail);

                return if head == tail {
                    if let (_, true) =
                        intrinsics::atomic_cxchg(&mut self.as_header_mut().tail, tail, tail)
                    {
                        println!("reader sleep");
                        self.as_header().reader.wait();
                        println!("reader awake");
                    }
                    continue;
                } else if head < tail {
                    let need_copy = cmp::min(buf.len(), tail - head);
                    ptr::copy(self.data.add(head), buf.as_mut_ptr(), need_copy);
                    intrinsics::atomic_xadd(&mut self.as_header_mut().head, need_copy);

                    self.as_header().writer.notify();
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

                    self.as_header().writer.notify();
                    Ok(need_copy)
                };
            }
        }
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        unsafe {
            loop {
                let head = intrinsics::atomic_load(&self.as_header().head);
                let tail = intrinsics::atomic_load(&self.as_header().tail);

                return if head > 0 && tail == head - 1 {
                    if let (_, true) =
                        intrinsics::atomic_cxchg(&mut self.as_header_mut().head, head, head)
                    {
                        println!("writer sleep");
                        self.as_header().writer.wait();
                        println!("writer awake");
                    }
                    continue;
                } else if tail < head {
                    let need_write = cmp::min(buf.len(), head - tail - 1);
                    ptr::copy(
                        buf.as_ptr(),
                        self.data.add(self.as_header().tail),
                        need_write,
                    );
                    intrinsics::atomic_xadd(&mut self.as_header_mut().tail, need_write);

                    self.as_header().reader.notify();
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

                    self.as_header().reader.notify();
                    Ok(need_write)
                };
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
