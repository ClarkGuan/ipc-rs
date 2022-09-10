#![allow(dead_code)]

use crate::{errors::libc_errno, Result};
use libc::{c_int, timespec};
use std::ptr;
use std::time::{Duration, Instant};

unsafe fn syscall_futex(
    addr: *const c_int,
    op: c_int,
    val: c_int,
    timeout: *const timespec,
    addr2: *const c_int,
    val3: c_int,
) -> c_int {
    if timeout.is_null() {
        loop {
            let ret: c_int =
                libc::syscall(libc::SYS_futex, addr, op, val, timeout, addr2, val3) as _;
            if ret == -1 && libc_errno() == libc::EINTR {
                continue;
            }
            return ret;
        }
    } else {
        let now = Instant::now();
        let timeout = Duration::from_timespec(&*timeout);
        loop {
            let left = timeout.saturating_sub(now.elapsed());
            let ret: c_int = libc::syscall(
                libc::SYS_futex,
                addr,
                op,
                val,
                &left.as_timespec(),
                addr2,
                val3,
            ) as _;
            if ret == -1 && libc_errno() == libc::EINTR {
                continue;
            }
            return ret;
        }
    }
}

#[derive(Debug)]
pub enum WaitResult {
    OK,
    ValNotEqual,
    Timeout,
}

pub fn futex_wait(addr: &u32, val: u32) -> Result<WaitResult> {
    unsafe {
        if syscall_futex(
            addr as *const u32 as _,
            libc::FUTEX_WAIT,
            val as _,
            ptr::null::<timespec>(),
            ptr::null::<c_int>(),
            0,
        ) == -1
        {
            if libc_errno() == libc::EAGAIN {
                return Ok(WaitResult::ValNotEqual);
            }
            return_errno!("futex");
        }

        Ok(WaitResult::OK)
    }
}

pub fn futex_wake(addr: &u32, num_processes: u32) -> Result<u32> {
    unsafe {
        let ret = syscall_futex(
            addr as *const u32 as _,
            libc::FUTEX_WAKE,
            num_processes as _,
            ptr::null::<timespec>(),
            ptr::null::<c_int>(),
            0,
        );
        if ret == -1 {
            return_errno!("futex");
        }

        Ok(ret as _)
    }
}

pub fn futex_timed_wait(addr: &u32, val: u32, timeout: Duration) -> Result<WaitResult> {
    unsafe {
        let ret = syscall_futex(
            addr as *const u32 as _,
            libc::FUTEX_WAIT,
            val as _,
            &timeout.as_timespec(),
            ptr::null::<c_int>(),
            0,
        );
        if ret == -1 {
            match libc_errno() {
                libc::ETIMEDOUT => return Ok(WaitResult::Timeout),
                libc::EAGAIN => return Ok(WaitResult::ValNotEqual),
                _ => return_errno!("futex"),
            }
        }

        Ok(WaitResult::OK)
    }
}

trait AsTimespec {
    fn as_timespec(&self) -> timespec;
    fn from_timespec(tm: &timespec) -> Self;
}

impl AsTimespec for Duration {
    fn as_timespec(&self) -> timespec {
        timespec {
            tv_sec: self.as_secs() as _,
            tv_nsec: self.subsec_nanos() as _,
        }
    }

    fn from_timespec(tm: &timespec) -> Self {
        Duration::new(tm.tv_sec as _, tm.tv_nsec as _)
    }
}
