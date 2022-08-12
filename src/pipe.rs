use crate::raw::RawFd;
use crate::Result;
use std::ffi::CString;
use std::{fs, io};

pub struct PipeReader(RawFd);
pub struct PipeWriter(RawFd);

impl io::Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub fn pipe() -> Result<(PipeReader, PipeWriter)> {
    unsafe {
        let mut fds: [libc::c_int; 2] = [0, 0];
        let ret = libc::pipe(fds.as_mut_ptr());
        if ret == -1 {
            return_errno!();
        }
        Ok((PipeReader(RawFd(fds[0])), PipeWriter(RawFd(fds[1]))))
    }
}

pub fn pipe2(flags: isize) -> Result<(PipeReader, PipeWriter)> {
    unsafe {
        let mut fds: [libc::c_int; 2] = [0, 0];
        let ret = libc::pipe2(fds.as_mut_ptr(), flags as _);
        if ret == -1 {
            return_errno!();
        }
        Ok((PipeReader(RawFd(fds[0])), PipeWriter(RawFd(fds[1]))))
    }
}

pub fn mkfifo(path: &str, mode: isize) -> Result<()> {
    unsafe {
        let path = CString::new(path)?;
        let ret = libc::mkfifo(path.as_ptr(), mode as _);
        if ret == -1 {
            return_errno!();
        }
        Ok(())
    }
}

pub struct Fifo {
    raw: RawFd,
    path: String,
}

impl Fifo {
    pub fn new(path: &str, flags: isize, mode: isize) -> Result<Fifo> {
        if fs::metadata(path).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists).into());
        }
        unsafe {
            let c_path = CString::new(path)?;
            let ret = libc::mkfifo(c_path.as_ptr(), mode as _);
            if ret == -1 {
                return_errno!();
            }
            let fd = libc::open(c_path.as_ptr(), flags as _);
            if fd == -1 {
                return_errno!();
            }
            Ok(Fifo { raw: RawFd(fd), path: path.to_string() })
        }
    }
}

impl io::Read for Fifo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.raw.read(buf)
    }
}

impl io::Write for Fifo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.raw.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.raw.flush()
    }
}

impl Drop for Fifo {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
