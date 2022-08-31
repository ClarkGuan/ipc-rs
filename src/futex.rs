use crate::Result;
use libc::{c_int, timespec};
use std::ptr;
use std::time::Duration;

unsafe fn syscall_futex(
    addr: *const c_int,
    op: c_int,
    val: c_int,
    timeout: *const timespec,
    addr2: *const c_int,
    val3: c_int,
) -> c_int {
    loop {
        let ret: c_int = libc::syscall(libc::SYS_futex, addr, op, val, timeout, addr2, val3) as _;
        if ret == -1 && *libc::__errno_location() == libc::EINTR {
            continue;
        }
        return ret;
    }
}

#[derive(Debug)]
pub enum WaitResult {
    OK,
    ValNotEqual,
    Timeout,
}

pub fn futex_wait(addr: &i32, val: i32) -> Result<WaitResult> {
    unsafe {
        if syscall_futex(
            addr,
            libc::FUTEX_WAIT,
            val,
            ptr::null::<timespec>(),
            ptr::null::<c_int>(),
            0,
        ) == -1
        {
            if *libc::__errno_location() == libc::EAGAIN {
                return Ok(WaitResult::ValNotEqual);
            }
            return_errno!("futex");
        }

        Ok(WaitResult::OK)
    }
}

pub fn futex_wake(addr: &i32, num_processes: i32) -> Result<i32> {
    unsafe {
        let ret = syscall_futex(
            addr,
            libc::FUTEX_WAKE,
            num_processes,
            ptr::null::<timespec>(),
            ptr::null::<c_int>(),
            0,
        );
        if ret == -1 {
            return_errno!("futex");
        }

        Ok(ret)
    }
}

pub fn futex_timed_wait(addr: &i32, val: i32, timeout: Duration) -> Result<WaitResult> {
    let timespec = timespec {
        tv_sec: timeout.as_secs() as _,
        tv_nsec: timeout.subsec_nanos() as _,
    };
    unsafe {
        let ret = syscall_futex(
            addr,
            libc::FUTEX_WAIT,
            val,
            &timespec,
            ptr::null::<c_int>(),
            0,
        );
        if ret == -1 {
            match *libc::__errno_location() {
                libc::ETIMEDOUT => return Ok(WaitResult::Timeout),
                libc::EAGAIN => return Ok(WaitResult::ValNotEqual),
                _ => return_errno!("futex"),
            }
        }

        Ok(WaitResult::OK)
    }
}
