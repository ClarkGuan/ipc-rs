extern crate alloc;

#[macro_use]
mod errors;

pub mod mq;
pub mod pipe;
pub(crate) mod raw;
pub mod sem;
pub mod flags;

pub use errors::Error;
pub type Result<T> = std::result::Result<T, Error>;

pub fn fork() -> Result<i32> {
    unsafe {
        let pid = libc::fork();
        if pid == -1 {
            return_errno!("fork");
        }
        Ok(pid)
    }
}

pub fn getpid() -> i32 {
    unsafe {
        libc::getpid()
    }
}

pub fn getppid() -> i32 {
    unsafe {
        libc::getppid()
    }
}
