//! The agent of polkit on the terminal: `pkttyagent`, for this process.
//! polkit asks its password on the terminal while the agent lives.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::process::{Child, Command, Stdio};

use rustix::io::{FdFlags, fcntl_setfd};
use rustix::process::{Pid, Signal, geteuid, kill_process};

use crate::error::{Error, Result};

/// The program of the agent.
const PROGRAM: &str = "pkttyagent";

/// The agent of polkit while it lives; it stops when dropped.
pub struct Agent {
    /// The process of `pkttyagent`.
    child: Child,
}

/// Whether polkit may ask this process a password: root is never asked.
fn may_be_asked() -> bool {
    !geteuid().is_root()
}

/// Whether this process has a controlling terminal: `pkttyagent` needs
/// one to ask there.
fn has_terminal() -> bool {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .is_ok()
}

impl Agent {
    /// Starts `pkttyagent` for this process, when it has a terminal, and
    /// waits until polkit has it. None for root, with no terminal, or when
    /// `pkttyagent` is not installed: polkit then asks no password here.
    pub fn start_on_terminal() -> Result<Option<Self>> {
        if !may_be_asked() || !has_terminal() {
            return Ok(None);
        }
        let (mut registered, notify) = std::io::pipe().map_err(|source| Error::Agent { source })?;
        fcntl_setfd(&notify, FdFlags::empty()).map_err(|errno| Error::Agent {
            source: errno.into(),
        })?;
        let spawned = Command::new(PROGRAM)
            .args(["--notify-fd", &notify.as_raw_fd().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn();
        drop(notify);
        let child = match spawned {
            Ok(child) => child,
            Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(Error::Agent { source }),
        };
        let agent = Self { child };
        let mut nothing = Vec::new();
        registered
            .read_to_end(&mut nothing)
            .map_err(|source| Error::Agent { source })?;
        Ok(Some(agent))
    }
}

impl Drop for Agent {
    /// `SIGTERM`, so `pkttyagent` sets the terminal back as it was.
    fn drop(&mut self) {
        let pid = i32::try_from(self.child.id()).ok().and_then(Pid::from_raw);
        if let Some(pid) = pid {
            let _ = kill_process(pid, Signal::TERM);
        }
        let _ = self.child.wait();
    }
}
