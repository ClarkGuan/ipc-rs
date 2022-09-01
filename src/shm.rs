#![allow(dead_code)]

use crate::Result;
use std::ffi::CString;
use std::{ptr, slice};

#[repr(C)]
#[derive(Debug)]
pub struct Shm {
    addr: *mut u8,
    size: usize,
    owner: bool,
    name: String,
}

unsafe impl Send for Shm {}
unsafe impl Sync for Shm {}

impl Shm {
    pub fn open(name: &str, size: usize, owner: bool) -> Result<Shm> {
        unsafe {
            let cstr = CString::new(name).expect("CString::new");
            let flags = if owner {
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDWR
            };
            let shm_fd = libc::shm_open(cstr.as_ptr(), flags, 0o666);
            if shm_fd == -1 {
                return_errno!("shm_open");
            }
            if libc::ftruncate64(shm_fd, size as _) == -1 {
                libc::close(shm_fd);
                return_errno!("ftruncate64");
            }
            let addr = libc::mmap(
                ptr::null_mut::<libc::c_void>(),
                size as _,
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
                if libc::munmap(addr, size as _) == -1 {
                    panic_errno!("munmap");
                }
                if owner {
                    if libc::shm_unlink(cstr.as_ptr()) == -1 {
                        panic_errno!("shm_unlink");
                    }
                }
                return_errno!("close");
            }
            Ok(Shm {
                addr: addr as _,
                size,
                owner,
                name: name.to_string(),
            })
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.addr, self.size) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.addr, self.size) }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.addr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn owner(&self) -> bool {
        self.owner
    }
}

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe {
            if libc::munmap(self.addr as _, self.size as _) == -1 {
                panic_errno!("munmap");
            }
            if self.owner {
                let c_string = CString::new(&*self.name).expect("CString::new");
                if libc::shm_unlink(c_string.as_ptr()) == -1 {
                    panic_errno!("shm_unlink");
                }
            }
        }
    }
}
