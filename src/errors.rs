use crate::sem::Semaphore;
use crate::Result;
use std::ffi::{CStr, NulError};
use std::str::Utf8Error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("UTF8 string error: {0}")]
    Utf8(#[from] Utf8Error),

    #[error("C style string nul error: {0}")]
    Null(#[from] NulError),

    #[error("errno: {0}, msg: {1}")]
    Errno(libc::c_int, String),

    #[error("parse int error: {0}")]
    Int(#[from] std::num::ParseIntError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("sem errno: {1}, msg: {2}")]
    Sem(Semaphore, libc::c_int, String),
}

impl Error {
    pub fn into_errno(self) -> (libc::c_int, String) {
        match self {
            Error::Errno(errno, msg) => (errno, msg),
            _ => panic!("can't into"),
        }
    }

    pub fn into_sem(self) -> (Semaphore, libc::c_int, String) {
        match self {
            Error::Sem(sem, errno, msg) => (sem, errno, msg),
            _ => panic!("can't into"),
        }
    }
}

pub(crate) fn strerror(errno: i32) -> Result<String> {
    unsafe {
        let cstr = CStr::from_ptr(libc::strerror(errno as _));
        Ok(cstr.to_str()?.to_string())
    }
}

macro_rules! return_errno {
    ($msg: expr) => {{
        let errno = *libc::__errno_location();
        return Err($crate::Error::Errno(
            errno,
            format!("{}: {}", $msg, $crate::errors::strerror(errno)?),
        ));
    }};

    () => {
        let errno = *libc::__errno_location();
        return Err($crate::Error::Errno(
            errno,
            $crate::errors::strerror(errno)?,
        ));
    };
}
