use crate::{Error, Result};
use libc::c_uint;
use std::ffi::CString;
use std::mem::MaybeUninit;

#[derive(Debug)]
pub struct Semaphore {
    raw: *mut libc::sem_t,
    name: String,
}

unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

impl Semaphore {
    pub fn open(name: &str, flags: isize, mode: isize, value: usize) -> Result<Semaphore> {
        unsafe {
            let c_name = CString::new(name)?;
            let sem = libc::sem_open(
                c_name.as_ptr(),
                flags as _,
                mode as libc::mode_t,
                value as c_uint,
            );

            if sem == libc::SEM_FAILED {
                return_errno!();
            }
            Ok(Semaphore {
                raw: sem,
                name: name.to_string(),
            })
        }
    }

    pub fn unlink(name: &str) -> Result<()> {
        unsafe {
            let c_name = CString::new(name)?;
            if libc::sem_unlink(c_name.as_ptr()) == -1 {
                return_errno!("sem_unlink");
            }
            Ok(())
        }
    }

    pub fn unlink_self(self) -> Result<()> {
        match Self::unlink(&self.name) {
            Ok(_) => Ok(()),
            Err(err) => {
                let (errno, msg) = err.into_errno();
                Err(Error::Sem(self, errno, msg))
            }
        }
    }

    pub fn value(&self) -> Result<usize> {
        unsafe {
            let mut val: libc::c_int = MaybeUninit::uninit().assume_init();
            if libc::sem_getvalue(self.raw, &mut val) == -1 {
                return_errno!("sem_getvalue");
            }
            Ok(val as _)
        }
    }

    pub fn post(&self) -> Result<()> {
        unsafe {
            if libc::sem_post(self.raw) == -1 {
                return_errno!("sem_post");
            }
            Ok(())
        }
    }

    pub fn wait(&self) -> Result<()> {
        unsafe {
            while libc::sem_wait(self.raw) == -1 {
                if *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                return_errno!("sem_wait");
            }
            Ok(())
        }
    }

    pub fn try_wait(&self) -> Result<()> {
        unsafe {
            if libc::sem_trywait(self.raw) == -1 {
                return_errno!("sem_trywait");
            }
            Ok(())
        }
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            assert_ne!(libc::sem_close(self.raw), -1);
        }
    }
}
