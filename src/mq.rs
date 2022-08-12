use crate::Result;
use std::ffi::CString;
use std::fmt::{Debug, Formatter};
use std::mem::MaybeUninit;
use std::{io, mem, ptr};

pub struct MessageQueue {
    inner: libc::mqd_t,
    name: String,
}

impl MessageQueue {
    pub fn open(name: &str, flags: isize, mode: isize) -> Result<MessageQueue> {
        unsafe {
            let c_name = CString::new(name)?;
            let fd = libc::mq_open(
                c_name.as_ptr(),
                flags as _,
                mode as libc::mode_t,
                ptr::null::<libc::mq_attr>(),
            );
            if fd == -1 {
                return_errno!("mq_open");
            }
            Ok(MessageQueue {
                inner: fd,
                name: name.to_string(),
            })
        }
    }

    pub fn attributes(&self) -> Result<MQAttribute> {
        unsafe {
            let mut attr: libc::mq_attr = MaybeUninit::uninit().assume_init();
            if libc::mq_getattr(self.inner, &mut attr) == -1 {
                return_errno!("mq_getattr");
            }
            Ok(MQAttribute(attr))
        }
    }

    pub fn set_attributes(&mut self, attr: &MQAttribute) -> Result<()> {
        unsafe {
            if libc::mq_setattr(self.inner, &attr.0, std::ptr::null_mut()) == -1 {
                return_errno!("mq_setattr");
            }
            Ok(())
        }
    }

    pub fn unlink_self(self) -> Result<()> {
        Self::unlink(&self.name)
    }

    pub fn unlink(name: &str) -> Result<()> {
        unsafe {
            let c_name = CString::new(name)?;
            if libc::mq_unlink(c_name.as_ptr()) == -1 {
                return_errno!("mq_unlink");
            }
            Ok(())
        }
    }
}

impl Drop for MessageQueue {
    fn drop(&mut self) {
        unsafe {
            assert_ne!(libc::mq_close(self.inner), -1);
        }
    }
}

impl io::Write for MessageQueue {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unsafe {
            let size = buf.len();
            let ret = libc::mq_send(self.inner, buf.as_ptr() as *const _, size as _, 0);
            if ret == -1 {
                return Err(io::Error::from_raw_os_error(*libc::__errno_location() as _));
            }
            Ok(size)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        // nothing to do
        Ok(())
    }
}

impl io::Read for MessageQueue {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        unsafe {
            let n = libc::mq_receive(
                self.inner,
                buf.as_mut_ptr() as *mut _,
                buf.len() as _,
                ptr::null_mut(),
            );
            if n == -1 {
                return Err(io::Error::from_raw_os_error(*libc::__errno_location() as _));
            }
            Ok(n as _)
        }
    }
}

impl Debug for MessageQueue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageQueue")
            .field("name: ", &self.name)
            .finish()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct MQAttribute(libc::mq_attr);

impl MQAttribute {
    pub fn new() -> MQAttribute {
        unsafe { MQAttribute(mem::zeroed()) }
    }

    pub fn set_flags(&mut self, flags: isize) -> &mut Self {
        self.0.mq_flags = flags as _;
        self
    }

    pub fn set_max_message_count(&mut self, count: isize) -> &mut Self {
        self.0.mq_maxmsg = count as _;
        self
    }

    pub fn set_message_size(&mut self, size: isize) -> &mut Self {
        self.0.mq_msgsize = size as _;
        self
    }

    pub fn flags(&self) -> isize {
        self.0.mq_flags as _
    }

    pub fn max_message_count(&self) -> isize {
        self.0.mq_maxmsg as _
    }

    pub fn message_size(&self) -> isize {
        self.0.mq_msgsize as _
    }

    pub fn current_message_count(&self) -> isize {
        self.0.mq_curmsgs as _
    }
}

impl Default for MQAttribute {
    fn default() -> Self {
        Self::new()
    }
}
