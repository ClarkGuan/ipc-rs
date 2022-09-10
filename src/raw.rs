use crate::errors::libc_errno;
use std::io::{self, Read, Write};

pub(crate) struct RawFd(pub(crate) libc::c_int);

impl Read for RawFd {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        inner_raed(self.0, buf)
    }
}

impl Write for RawFd {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        inner_write(self.0, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RawFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

fn inner_raed(fd: libc::c_int, buf: &mut [u8]) -> io::Result<usize> {
    unsafe {
        let n = libc::read(fd, buf.as_mut_ptr() as _, buf.len() as _);
        if n == -1 {
            return Err(io::Error::from_raw_os_error(libc_errno() as _));
        }
        Ok(n as _)
    }
}

fn inner_write(fd: libc::c_int, buf: &[u8]) -> io::Result<usize> {
    unsafe {
        let n = libc::write(fd, buf.as_ptr() as _, buf.len() as _);
        if n == -1 {
            return Err(io::Error::from_raw_os_error(libc_errno() as _));
        }
        Ok(n as _)
    }
}
