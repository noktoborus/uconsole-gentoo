//! The holder of `on`: `drowse-inhibit hold <why>`, started by the tool.
//! It takes the inhibitor in the process group of the tool, so polkit can
//! ask on the terminal. Then it leaves the terminal (a session of its
//! own: no `SIGHUP` at a logout, no `SIGINT` of the terminal), says its
//! word on stdout and holds the inhibitor until `SIGTERM`.

use std::io::Write;

use rustix::process::setsid;
use tokio::signal::unix::{SignalKind, signal};

use super::{WHO, take};
use crate::error::{Error, Result, chain};

/// The first argument that runs the holder.
pub const VERB: &str = "hold";

/// The word of the holder when it holds the inhibitor; else it says why
/// not.
pub const HELD: &str = "held";

/// Takes the inhibitor, says the word, and holds it until `SIGTERM`.
pub async fn hold(why: &str) -> Result<()> {
    let taken = detached(why).await;
    let word = match &taken {
        Ok(_) => HELD.to_owned(),
        Err(error) => chain(error),
    };
    let mut out = std::io::stdout().lock();
    writeln!(out, "{word}")
        .and_then(|()| out.flush())
        .map_err(|source| Error::HolderWord { source })?;
    drop(out);
    let (inhibitor, mut terminate) = taken?;
    terminate.recv().await;
    drop(inhibitor);
    Ok(())
}

/// Takes the inhibitor, leaves the terminal, and follows `SIGTERM`.
async fn detached(why: &str) -> Result<(std::os::fd::OwnedFd, tokio::signal::unix::Signal)> {
    let inhibitor = take(WHO, why).await?;
    setsid().map_err(|errno| Error::Detach {
        source: errno.into(),
    })?;
    let terminate = signal(SignalKind::terminate()).map_err(|source| Error::Signal {
        name: "SIGTERM",
        source,
    })?;
    Ok((inhibitor, terminate))
}
