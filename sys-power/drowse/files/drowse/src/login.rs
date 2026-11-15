//! The monitor of sd-login: it wakes when logind writes the state of a
//! session again, as at its move to `closing`, which sends no signal on
//! D-Bus. The only `unsafe` of the crate, behind a safe type.
#![allow(unsafe_code)]

use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::ptr;

use libsystemd_sys::login::{
    sd_login_monitor, sd_login_monitor_flush, sd_login_monitor_get_fd, sd_login_monitor_new,
    sd_login_monitor_unref,
};
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

/// The category of the monitor: the sessions.
const SESSIONS: &std::ffi::CStr = c"session";

/// A monitor of sd-login and its file descriptor. `monitor` comes from
/// `sd_login_monitor_new`, lives while `Raw` lives and is freed once, at
/// its drop; every call on it is so on a live monitor.
struct Raw {
    monitor: *mut sd_login_monitor,
    fd: RawFd,
}

// SAFETY: the monitor is owned by one `Raw` and used by one task at a
// time; libsystemd keeps no tie to the thread that made it.
unsafe impl Send for Raw {}

impl AsRawFd for Raw {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        unsafe { sd_login_monitor_unref(self.monitor) };
    }
}

/// The monitor of the sessions of sd-login.
pub struct SessionMonitor {
    fd: AsyncFd<Raw>,
}

impl SessionMonitor {
    /// Starts a monitor of the sessions. The category is a C string, and
    /// `sd_login_monitor_new` writes the new monitor to `monitor`. The fd
    /// given to `AsyncFd` is that of the monitor: it stays open and the
    /// same until `Raw` drops, after `AsyncFd` lets it go.
    pub fn new() -> io::Result<Self> {
        let mut monitor: *mut sd_login_monitor = ptr::null_mut();
        let made = unsafe { sd_login_monitor_new(SESSIONS.as_ptr(), &mut monitor) };
        if made < 0 {
            return Err(io::Error::from_raw_os_error(-made));
        }
        let fd = unsafe { sd_login_monitor_get_fd(monitor) };
        let raw = Raw { monitor, fd };
        if fd < 0 {
            return Err(io::Error::from_raw_os_error(-fd));
        }
        let fd = unsafe { AsyncFd::register_with_interest(raw, Interest::READABLE) }?;
        Ok(Self { fd })
    }

    /// Waits until logind writes the state of a session again.
    pub async fn changed(&mut self) -> io::Result<()> {
        let mut ready = self.fd.readable_mut().await?;
        let raw = ready.get_inner_mut();
        let flushed = unsafe { sd_login_monitor_flush(raw.monitor) };
        ready.clear_ready();
        if flushed < 0 {
            return Err(io::Error::from_raw_os_error(-flushed));
        }
        Ok(())
    }
}
