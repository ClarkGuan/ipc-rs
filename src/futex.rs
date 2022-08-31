use crate::Result;
use libc::{c_int, timespec};
use std::ops::Add;
use std::ptr;
use std::time::{Duration, SystemTime};

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

pub(crate) fn futex_wait(addr: &i32, val: i32) -> Result<()> {
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
            return_errno!("futex");
        }

        Ok(())
    }
}

pub(crate) fn futex_wake(addr: &i32, num_processes: i32) -> Result<i32> {
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

pub(crate) fn futex_timed_wait(addr: &i32, val: i32, timeout: Duration) -> Result<bool> {
    let target = SystemTime::UNIX_EPOCH
        .elapsed()
        .expect("SystemTime::UNIX_EPOCH.elapsed()")
        .add(timeout);
    let timespec = timespec {
        tv_sec: target.as_secs() as _,
        tv_nsec: target.subsec_nanos() as _,
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
            if *libc::__errno_location() == libc::ETIMEDOUT {
                return Ok(true);
            }
            return_errno!("futex");
        }

        Ok(false)
    }
}
