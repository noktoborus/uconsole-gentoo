//! `drowse-inhibit [--why <text>] -- <command>`: the tool takes the
//! inhibitor and holds it while the command runs. The command has the
//! terminal; the tool waits, and gives its exit code.

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};

use rustix::io::{FdFlags, fcntl_setfd};
use tokio::signal::unix::{SignalKind, signal};

use super::{Kind, UNTIL, take, why_or_default};
use crate::error::{Error, Result};

/// The exit code of a command killed by a signal: 128 and the signal, as
/// in a shell.
const KILLED_BASE: i32 = 128;

/// The `who`: `until` and the command, as typed.
fn who(command: &[String]) -> String {
    let line =
        shlex::try_join(command.iter().map(String::as_str)).unwrap_or_else(|_| command.join(" "));
    format!("{UNTIL}{line}")
}

/// The exit code of the tool for this end of the command.
fn exit_code(status: ExitStatus) -> u8 {
    let code = status
        .code()
        .or_else(|| status.signal().map(|signal| KILLED_BASE + signal))
        .unwrap_or(1);
    u8::try_from(code & 0xff).unwrap_or(1)
}

/// Holds the inhibitor while the command runs; gives its exit code.
pub async fn run(kind: Kind, why: Option<&str>, command: &[String]) -> Result<u8> {
    let Some((program, args)) = command.split_first() else {
        return Ok(0);
    };
    let why = why_or_default(why);
    let mut inhibitors = Vec::new();
    for what in kind.whats() {
        let inhibitor = take(&who(command), &why, what).await?;
        fcntl_setfd(&inhibitor, FdFlags::CLOEXEC).map_err(|errno| Error::Run {
            program: program.clone(),
            source: errno.into(),
        })?;
        inhibitors.push(inhibitor);
    }
    let _interrupt = followed(SignalKind::interrupt(), "SIGINT")?;
    let _quit = followed(SignalKind::quit(), "SIGQUIT")?;
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|source| Error::Run {
            program: program.clone(),
            source,
        })?;
    drop(inhibitors);
    Ok(exit_code(status))
}

/// Follows a signal of the terminal, so it reaches the command only: the
/// tool lives on, and the inhibitor with it, while the command runs.
fn followed(kind: SignalKind, name: &'static str) -> Result<tokio::signal::unix::Signal> {
    signal(kind).map_err(|source| Error::Signal { name, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_is_until_and_the_command_as_typed() {
        let command = ["emerge", "-uDN", "@world", "a b"].map(str::to_owned);
        assert_eq!(who(&command), "until emerge -uDN @world 'a b'");
    }

    #[test]
    fn exit_code_of_a_signal_is_128_and_more() {
        assert_eq!(exit_code(ExitStatus::from_raw(9)), 137);
        assert_eq!(exit_code(ExitStatus::from_raw(3 << 8)), 3);
    }
}
