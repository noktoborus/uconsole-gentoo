//! drowse-inhibit: the inhibitor of system held by hand. The tool calls
//! `Inhibit` of logind itself; the process that calls it holds it.
//!
//! - `on`: a holder, `drowse-inhibit hold`, takes it, leaves the terminal
//!   and holds it until `off` (`hold`).
//! - `-- <command>`: the tool takes it and holds it while the command
//!   runs (`run`).
//!
//! On a terminal, polkit asks its password there (`agent`).

mod agent;
pub mod hold;
pub mod run;

use std::io::{BufRead, BufReader};
use std::os::fd::OwnedFd;
use std::process::{Command, Stdio};

use rust_i18n::t;
use rustix::process::{Pid, Signal, getuid, kill_process_group};
use zbus::Connection;

use crate::error::{Error, Result};
use crate::world::logind::ManagerProxy;
use crate::world::what::Holds;

/// The `who` of the inhibitor of `on`.
pub const WHO: &str = "drowse-inhibit";

/// The start of the `who` of the inhibitor of `-- <command>`; the command
/// follows.
pub const UNTIL: &str = "until ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The kinds of work an inhibitor holds.
pub enum Kind {
    /// Hold command sections.
    Idle,
    /// Hold logind sleep and shutdown actions.
    Power,
    /// Hold both kinds.
    All,
}

impl Kind {
    /// The inhibitors to take for this kind.
    pub fn whats(self) -> &'static [&'static str] {
        match self {
            Self::Idle => &["idle"],
            Self::Power => &["shutdown:sleep"],
            Self::All => &["idle", "shutdown:sleep"],
        }
    }
}

/// The mode: the service keeps to it; a power off by hand passes it.
const MODE: &str = "block-weak";

/// Whether an inhibitor of this `who` is one of the tool.
pub fn is_own(who: &str) -> bool {
    who == WHO || who.starts_with(UNTIL)
}

/// What the tool did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The inhibitors of this kind are held, for this reason.
    Held {
        /// What they hold.
        kind: Kind,
        /// Why.
        why: String,
    },
    /// The inhibitor is let go.
    LetGo,
    /// There was no inhibitor of the tool to let go.
    NothingToLetGo,
}

impl Outcome {
    /// The message for the user, in the language of the session.
    pub fn message(&self) -> String {
        match self {
            Self::Held { kind, why } => {
                let kind = match kind {
                    Kind::Idle => t!("inhibit.kind_idle"),
                    Kind::Power => t!("inhibit.kind_power"),
                    Kind::All => t!("inhibit.kind_all"),
                };
                t!("inhibit.on", kind = kind, why = why).into_owned()
            }
            Self::LetGo => t!("inhibit.off").into_owned(),
            Self::NothingToLetGo => t!("inhibit.not_held").into_owned(),
        }
    }
}

/// The `why` given, or the default one.
fn why_or_default(why: Option<&str>) -> String {
    why.map_or_else(|| t!("inhibit.why_default").into_owned(), str::to_owned)
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

/// Takes an inhibitor of logind for this process. With a terminal, the
/// password polkit asks for is typed there.
pub(crate) async fn take(who: &str, why: &str, what: &str) -> Result<OwnedFd> {
    let logind = logind().await?;
    let agent = agent::Agent::start_on_terminal()?;
    let fd = logind
        .inhibit(what, who, why, MODE)
        .await
        .map_err(|source| Error::Bus {
            what: "take an inhibitor of logind",
            source,
        })?;
    drop(agent);
    Ok(fd.into())
}

/// The pids of the inhibitors of `on` of this user.
async fn own_pids(logind: &ManagerProxy<'static>, kind: Kind) -> Result<Vec<u32>> {
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
        .filter(|(what, who, _, _, owner, _)| {
            who == WHO && *owner == uid && matches_kind(what, kind)
        })
        .map(|(_, _, _, _, _, pid)| pid)
        .collect())
}

/// Takes the inhibitor; a new one takes the place of the one before, so
/// there is no moment with none. The tool waits for the holder's word: the
/// password is typed meanwhile.
pub async fn on(kind: Kind, why: Option<&str>) -> Result<Outcome> {
    let why = why_or_default(why);
    let logind = logind().await?;
    for &what in kind.whats() {
        let before = own_pids_for_what(&logind, what).await?;
        let word = start_holder(&why, what)?;
        match word.as_str() {
            hold::HELD => let_go(&before)?,
            "" => return Err(Error::HolderGone),
            reason => {
                return Err(Error::InhibitRefused {
                    reason: reason.to_owned(),
                });
            }
        }
    }
    Ok(Outcome::Held { kind, why })
}

/// Starts the holder in the process group of the tool, so polkit can ask
/// on the terminal, and reads its first line.
fn start_holder(why: &str, what: &str) -> Result<String> {
    let exe = std::env::current_exe().map_err(|source| Error::HolderStart { source })?;
    let mut child = Command::new(exe)
        .args([hold::VERB, why, what])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| Error::HolderStart { source })?;
    let Some(out) = child.stdout.take() else {
        return Err(Error::HolderGone);
    };
    let mut line = String::new();
    BufReader::new(out)
        .read_line(&mut line)
        .map_err(|source| Error::HolderStart { source })?;
    Ok(line.trim_end().to_owned())
}

/// Lets the inhibitor go.
pub async fn off(kind: Kind) -> Result<Outcome> {
    let logind = logind().await?;
    let pids = own_pids(&logind, kind).await?;
    if pids.is_empty() {
        return Ok(Outcome::NothingToLetGo);
    }
    let_go(&pids)?;
    Ok(Outcome::LetGo)
}

/// Takes the inhibitor when it is not held, else lets it go.
pub async fn toggle(kind: Kind, why: Option<&str>) -> Result<Outcome> {
    let logind = logind().await?;
    if own_pids(&logind, kind).await?.is_empty() {
        on(kind, why).await
    } else {
        off(kind).await
    }
}

/// The click of the module of waybar: none, then `idle` (no section is
/// entered), then all (no power off or sleep either), then none.
pub async fn cycle(why: Option<&str>) -> Result<Outcome> {
    let logind = logind().await?;
    let idle = !own_pids(&logind, Kind::Idle).await?.is_empty();
    let power = !own_pids(&logind, Kind::Power).await?.is_empty();
    match cycle_next(idle, power) {
        Some(kind) => on(kind, why).await,
        None => off(Kind::All).await,
    }
}

/// The next step of `cycle` from what of the tool is held: the kind to
/// hold, or none to let all go. A power hold, alone or not, is the end
/// of the round.
fn cycle_next(idle: bool, power: bool) -> Option<Kind> {
    match (idle, power) {
        (false, false) => Some(Kind::Idle),
        (true, false) => Some(Kind::All),
        (_, true) => None,
    }
}

async fn own_pids_for_what(logind: &ManagerProxy<'static>, what: &str) -> Result<Vec<u32>> {
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
        .filter(|(found, who, _, _, owner, _)| {
            who == WHO && *owner == uid && same_what(found, what)
        })
        .map(|(_, _, _, _, _, pid)| pid)
        .collect())
}

fn same_what(found: &str, wanted: &str) -> bool {
    Holds::parse(found) == Holds::parse(wanted)
}

fn matches_kind(what: &str, kind: Kind) -> bool {
    let wanted = kind.whats().iter().map(|what| Holds::parse(what));
    wanted
        .into_iter()
        .any(|want| Holds::parse(what).meets(want))
}

/// Sends `SIGTERM` to the process groups of these pids. A holder leads a
/// session and a group of its own, so its pid is its group. A group that
/// is gone is no error.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logind_order_matches_the_requested_inhibitor() {
        assert_eq!(Kind::Power.whats(), &["shutdown:sleep"]);
        assert!(matches_kind("shutdown:sleep", Kind::Power));
        assert!(matches_kind("sleep:shutdown", Kind::Power));
        assert!(!matches_kind("idle", Kind::Power));
    }

    #[test]
    fn cycle_holds_the_sections_then_the_power_then_nothing() {
        assert_eq!(cycle_next(false, false), Some(Kind::Idle));
        assert_eq!(cycle_next(true, false), Some(Kind::All));
        assert_eq!(cycle_next(true, true), None);
        assert_eq!(cycle_next(false, true), None);
    }

    #[test]
    fn an_existing_inhibitor_is_found_without_order_mattering() {
        assert!(same_what("shutdown:sleep", "sleep:shutdown"));
        assert!(!same_what("shutdown", "sleep:shutdown"));
    }
}
