#![allow(dead_code)]

use crate::Result;
use core::fmt::Debug;
use libc::c_uint;
use std::ffi::CString;
use std::mem;
use std::mem::MaybeUninit;

pub trait SemaphoreLike: Debug {
    fn value(&self) -> usize;
    fn post(&self);
    fn wait(&self);
}

impl SemaphoreLike for *mut libc::sem_t {
    fn value(&self) -> usize {
        unsafe {
            let mut val: libc::c_int = MaybeUninit::uninit().assume_init();
            if libc::sem_getvalue(*self, &mut val) == -1 {
                panic_errno!("sem_getvalue");
            }
            val as _
        }
    }

    fn post(&self) {
        unsafe {
            if libc::sem_post(*self) == -1 {
                panic_errno!("sem_post");
            }
        }
    }

    fn wait(&self) {
        unsafe {
            while libc::sem_wait(*self) == -1 {
                if *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                panic_errno!("sem_wait");
            }
        }
    }
}

#[derive(Debug)]
pub enum Semaphore {
    Anonymous(libc::sem_t),
    Named(*mut libc::sem_t, String),
}

impl Semaphore {
    pub fn init(val: usize) -> Result<Semaphore> {
        unsafe {
            let sem = Semaphore::Anonymous(MaybeUninit::uninit().assume_init());
            if libc::sem_init(sem.as_ptr_mut(), 1, val as _) == -1 {
                return_errno!("sem_init");
            }
            Ok(sem)
        }
    }

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
            Ok(Semaphore::Named(sem, name.to_string()))
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

    pub fn unlink_self(self) {
        if let Semaphore::Named(_, ref name) = self {
            let _ = Self::unlink(name);
        }
    }

    fn as_ptr_mut(&self) -> *mut libc::sem_t {
        match self {
            &Semaphore::Anonymous(ref sem) => unsafe { mem::transmute(sem) },
            &Semaphore::Named(sem, ..) => sem,
        }
    }
}

impl SemaphoreLike for Semaphore {
    fn value(&self) -> usize {
        self.as_ptr_mut().value()
    }

    fn post(&self) {
        self.as_ptr_mut().post()
    }

    fn wait(&self) {
        self.as_ptr_mut().wait()
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        match self {
            &mut Semaphore::Anonymous(..) => unsafe {
                assert_ne!(libc::sem_destroy(self.as_ptr_mut()), -1);
            },

            &mut Semaphore::Named(..) => unsafe {
                assert_ne!(libc::sem_close(self.as_ptr_mut()), -1);
            },
        }
    }
}

impl From<*const u8> for &Semaphore {
    fn from(ptr: *const u8) -> Self {
        unsafe { &*(ptr as *const Semaphore) }
    }
}

impl From<*mut u8> for &mut Semaphore {
    fn from(ptr: *mut u8) -> Self {
        unsafe { &mut *(ptr as *mut Semaphore) }
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct RawSemaphore(libc::sem_t);

impl RawSemaphore {
    pub(crate) fn init(&self, val: usize) {
        if unsafe { libc::sem_init(self.as_ptr_mut(), 1, val as _) } == -1 {
            panic_errno!("sem_init");
        }
    }

    fn as_ptr_mut(&self) -> *mut libc::sem_t {
        unsafe { mem::transmute(&self.0) }
    }
}

impl SemaphoreLike for RawSemaphore {
    fn value(&self) -> usize {
        self.as_ptr_mut().value()
    }

    fn post(&self) {
        self.as_ptr_mut().post()
    }

    fn wait(&self) {
        self.as_ptr_mut().wait()
    }
}

impl Drop for RawSemaphore {
    fn drop(&mut self) {
        unsafe {
            assert_ne!(libc::sem_destroy(self.as_ptr_mut()), -1);
        }
    }
}
