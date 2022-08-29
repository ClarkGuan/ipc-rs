use crate::{Error, Result};
use libc::c_uint;
use std::ffi::CString;
use std::mem::MaybeUninit;

pub trait SemaphoreLike {
    fn value(&self) -> Result<usize>;
    fn post(&self) -> Result<()>;
    fn wait(&self) -> Result<()>;
    fn try_wait(&self) -> Result<()>;
}

#[derive(Debug)]
struct RawSem(*mut libc::sem_t);

unsafe impl Send for RawSem {}
unsafe impl Sync for RawSem {}

impl SemaphoreLike for RawSem {
    fn value(&self) -> Result<usize> {
        unsafe {
            let mut val: libc::c_int = MaybeUninit::uninit().assume_init();
            if libc::sem_getvalue(self.0, &mut val) == -1 {
                return_errno!("sem_getvalue");
            }
            Ok(val as _)
        }
    }

    fn post(&self) -> Result<()> {
        unsafe {
            if libc::sem_post(self.0) == -1 {
                return_errno!("sem_post");
            }
            Ok(())
        }
    }

    fn wait(&self) -> Result<()> {
        unsafe {
            while libc::sem_wait(self.0) == -1 {
                if *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                return_errno!("sem_wait");
            }
            Ok(())
        }
    }

    fn try_wait(&self) -> Result<()> {
        unsafe {
            if libc::sem_trywait(self.0) == -1 {
                return_errno!("sem_trywait");
            }
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct AnonymousSemaphore(RawSem);

impl AnonymousSemaphore {
    pub fn init(raw: *mut libc::sem_t, val: usize) -> Result<AnonymousSemaphore> {
        unsafe {
            if libc::sem_init(raw, 1, val as _) == -1 {
                return_errno!("sem_init");
            }
            Ok(AnonymousSemaphore(RawSem(raw)))
        }
    }
}

impl Drop for AnonymousSemaphore {
    fn drop(&mut self) {
        unsafe {
            assert_ne!(libc::sem_destroy(self.0.0), -1);
        }
    }
}

impl SemaphoreLike for AnonymousSemaphore {
    fn value(&self) -> Result<usize> {
        self.0.value()
    }

    fn post(&self) -> Result<()> {
        self.0.post()
    }

    fn wait(&self) -> Result<()> {
        self.0.wait()
    }

    fn try_wait(&self) -> Result<()> {
        self.0.try_wait()
    }
}

#[derive(Debug)]
pub struct Semaphore {
    raw: RawSem,
    name: String,
}

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
                raw: RawSem(sem),
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

}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            assert_ne!(libc::sem_close(self.raw.0), -1);
        }
    }
}

impl SemaphoreLike for Semaphore {
    fn value(&self) -> Result<usize> {
        self.raw.value()
    }

    fn post(&self) -> Result<()> {
        self.raw.post()
    }

    fn wait(&self) -> Result<()> {
        self.raw.wait()
    }

    fn try_wait(&self) -> Result<()> {
        self.raw.try_wait()
    }
}
