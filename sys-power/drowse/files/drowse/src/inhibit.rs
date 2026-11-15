//! drowse-inhibit: the inhibitor of system held by hand. A child
//! `systemd-inhibit` in a process group of its own holds it, as long as it
//! lives; the tool does not wait for it.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use rust_i18n::t;
use rustix::process::{Pid, Signal, getuid, kill_process_group};
use zbus::Connection;

use crate::error::{Error, Result};
use crate::world::logind::ManagerProxy;

/// The `who` of the inhibitor of the tool.
pub const WHO: &str = "drowse-inhibit";

/// How often the tool looks for its new inhibitor.
const LOOK_EVERY: Duration = Duration::from_millis(100);

/// How long the tool waits for its new inhibitor.
const LOOK_FOR: Duration = Duration::from_secs(5);

/// What the tool did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The inhibitor is held, for this reason.
    Held(String),
    /// The inhibitor is let go.
    LetGo,
    /// There was no inhibitor of the tool to let go.
    NothingToLetGo,
}

impl Outcome {
    /// The message for the user, in the language of the session.
    pub fn message(&self) -> String {
        match self {
            Self::Held(why) => t!("inhibit.on", why = why).into_owned(),
            Self::LetGo => t!("inhibit.off").into_owned(),
            Self::NothingToLetGo => t!("inhibit.not_held").into_owned(),
        }
    }
}

/// The manager of logind on the system bus.
async fn logind() -> Result<ManagerProxy<'static>> {
    let conn = Connection::system().await.map_err(|source| Error::Bus {
        what: "connect to the system bus",
        source,
    })?;
    ManagerProxy::new(&conn).await.map_err(|source| Error::Bus {
        what: "make the proxy of logind",
        source,
    })
}

/// The pids of the inhibitors of the tool of this user.
async fn own_pids(logind: &ManagerProxy<'static>) -> Result<Vec<u32>> {
    let uid = getuid().as_raw();
    let rows = logind
        .list_inhibitors()
        .await
        .map_err(|source| Error::Bus {
            what: "list the inhibitors of logind",
            source,
        })?;
    Ok(rows
        .into_iter()
        .filter(|(_, who, _, _, owner, _)| who == WHO && *owner == uid)
        .map(|(_, _, _, _, _, pid)| pid)
        .collect())
}

/// Takes the inhibitor; a new one takes the place of the one before, so
/// there is no moment with none.
pub async fn on(why: Option<&str>) -> Result<Outcome> {
    let why = why.map_or_else(|| t!("inhibit.why_default").into_owned(), str::to_owned);
    let logind = logind().await?;
    let before = own_pids(&logind).await?;
    let mut child = spawn(&why).map_err(|source| Error::InhibitStart { source })?;
    wait_until_held(&logind, &mut child).await?;
    let_go(&before)?;
    Ok(Outcome::Held(why))
}

/// Lets the inhibitor go.
pub async fn off() -> Result<Outcome> {
    let logind = logind().await?;
    let pids = own_pids(&logind).await?;
    if pids.is_empty() {
        return Ok(Outcome::NothingToLetGo);
    }
    let_go(&pids)?;
    Ok(Outcome::LetGo)
}

/// Takes the inhibitor when it is not held, else lets it go.
pub async fn toggle(why: Option<&str>) -> Result<Outcome> {
    let logind = logind().await?;
    if own_pids(&logind).await?.is_empty() {
        on(why).await
    } else {
        off().await
    }
}

/// Starts `systemd-inhibit` in a process group of its own, its stdin,
/// stdout and stderr on `/dev/null`.
fn spawn(why: &str) -> std::io::Result<Child> {
    Command::new("systemd-inhibit")
        .args([
            "--mode=block-weak",
            "--what=sleep:shutdown",
            &format!("--who={WHO}"),
            &format!("--why={why}"),
            "sleep",
            "infinity",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

/// Waits until the inhibitor of the child is in logind, or the child ends
/// (logind or polkit said no).
async fn wait_until_held(logind: &ManagerProxy<'static>, child: &mut Child) -> Result<()> {
    let mut waited = Duration::ZERO;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|source| Error::InhibitStart { source })?
        {
            return Err(Error::InhibitRefused {
                status: status.to_string(),
            });
        }
        if own_pids(logind).await?.contains(&child.id()) {
            return Ok(());
        }
        if waited >= LOOK_FOR {
            let_go(&[child.id()])?;
            return Err(Error::InhibitNotSeen);
        }
        tokio::time::sleep(LOOK_EVERY).await;
        waited += LOOK_EVERY;
    }
}

/// Sends `SIGTERM` to the process groups of these pids: `systemd-inhibit`
/// and its `sleep` end, and the inhibitor with them. A group that is gone
/// is no error.
fn let_go(pids: &[u32]) -> Result<()> {
    for &pid in pids {
        let Some(group) = i32::try_from(pid).ok().and_then(Pid::from_raw) else {
            continue;
        };
        match kill_process_group(group, Signal::TERM) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(errno) => {
                return Err(Error::InhibitStop {
                    pid,
                    source: errno.into(),
                });
            }
        }
    }
    Ok(())
}
