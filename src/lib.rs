#![feature(core_intrinsics)]

use cfg_if::cfg_if;

#[macro_use]
mod errors;

pub mod flags;
pub(crate) mod futex;
pub mod pipe;
pub(crate) mod raw;
pub mod sem;

cfg_if! {
    if #[cfg(not(target_os = "android"))] {
        pub mod mq;
        pub mod ring;
        pub(crate) mod shm;
    }
}

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
    unsafe { libc::getpid() }
}

pub fn getppid() -> i32 {
    unsafe { libc::getppid() }
}

pub fn waitpid(pid: i32, options: isize) -> Result<(i32, isize)> {
    unsafe {
        let mut status: libc::c_int = 0;
        let ret = libc::waitpid(pid as _, &mut status, options as _);
        if ret == -1 {
            return_errno!("waitpid");
        }
        Ok((ret, status as _))
    }
}
