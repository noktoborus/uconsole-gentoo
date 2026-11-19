//! The errors of the crate.

use std::fmt;
use std::path::PathBuf;

/// A failure the program cannot go on from.
#[derive(Debug)]
pub enum Error {
    /// The settings file is there but cannot be read.
    ReadSettings {
        /// The path of the settings file.
        path: PathBuf,
        /// The error of the system.
        source: std::io::Error,
    },
    /// The settings file is not valid YAML or has an unknown key or value.
    SettingsYaml {
        /// The error from the YAML reader.
        source: serde_yaml_ng::Error,
    },
    /// A section sets a logind action and a command.
    SettingsActionConflict {
        /// The place of the section.
        place: String,
    },
    /// A logind action is not supported by drowse.
    SettingsActionUnknown {
        /// The action as written.
        action: String,
        /// The place of the section.
        place: String,
    },
    /// A logind action is not the last section of its chain.
    SettingsActionNotLast {
        /// The name of the section: `main.battery.1`.
        place: String,
    },
    /// A command in the settings cannot be split into words.
    SettingsCommand {
        /// The command as written.
        command: String,
        /// The place of the section.
        place: String,
    },
    /// A call on the system bus failed.
    Bus {
        /// What the call was for.
        what: &'static str,
        /// The error of the bus.
        source: zbus::Error,
    },
    /// The monitor of sd-login cannot start.
    SessionMonitor {
        /// The error of the system.
        source: std::io::Error,
    },
    /// A signal cannot be followed.
    Signal {
        /// The name of the signal.
        name: &'static str,
        /// The error of the system.
        source: std::io::Error,
    },
    /// logind left the bus: the service exits, and systemd starts it again.
    LogindGone,
    /// The agent of polkit on the terminal cannot be started.
    Agent {
        /// The error of the system.
        source: std::io::Error,
    },
    /// The holder of the inhibitor cannot be started, or its word cannot
    /// be read.
    HolderStart {
        /// The error of the system.
        source: std::io::Error,
    },
    /// The holder ended with no word.
    HolderGone,
    /// The holder cannot write its word.
    HolderWord {
        /// The error of the system.
        source: std::io::Error,
    },
    /// The holder cannot leave the terminal.
    Detach {
        /// The error of the system.
        source: std::io::Error,
    },
    /// The holder did not get the inhibitor: logind or polkit said no.
    InhibitRefused {
        /// What the holder said.
        reason: String,
    },
    /// The command to run under the inhibitor cannot be started.
    Run {
        /// The program of the command.
        program: String,
        /// The error of the system.
        source: std::io::Error,
    },
    /// The process that holds an inhibitor cannot be stopped.
    InhibitStop {
        /// Its pid.
        pid: u32,
        /// The error of the system.
        source: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadSettings { path, .. } => {
                write!(f, "cannot read the settings file {}", path.display())
            }
            Self::SettingsYaml { source } => match source.location() {
                Some(location) => {
                    write!(f, "invalid settings at line {}: {source}", location.line())
                }
                None => write!(f, "invalid settings: {source}"),
            },
            Self::SettingsActionConflict { place } => {
                write!(f, "a logind action cannot have a command in {place}")
            }
            Self::SettingsActionUnknown { action, place } => {
                write!(f, "unknown logind action {action} in {place}")
            }
            Self::SettingsActionNotLast { place } => {
                write!(f, "logind action in {place} is not last")
            }
            Self::SettingsCommand { command, place } => {
                write!(f, "invalid command in {place}: {command}")
            }
            Self::Bus { what, .. } => write!(f, "cannot {what}"),
            Self::SessionMonitor { .. } => f.write_str("cannot start the monitor of sd-login"),
            Self::Signal { name, .. } => write!(f, "cannot follow {name}"),
            Self::LogindGone => f.write_str("logind left the bus"),
            Self::Agent { .. } => f.write_str("cannot start pkttyagent"),
            Self::HolderStart { .. } => f.write_str("cannot start the holder of the inhibitor"),
            Self::HolderGone => f.write_str("the holder of the inhibitor ended with no word"),
            Self::HolderWord { .. } => f.write_str("the holder cannot write its word"),
            Self::Detach { .. } => f.write_str("the holder cannot leave the terminal"),
            Self::InhibitRefused { reason } => write!(f, "the inhibitor was refused: {reason}"),
            Self::Run { program, .. } => write!(f, "cannot run {program}"),
            Self::InhibitStop { pid, .. } => write!(f, "cannot stop the inhibitor of pid {pid}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadSettings { source, .. } => Some(source),
            Self::SettingsYaml { source } => Some(source),
            Self::Bus { source, .. } => Some(source),
            Self::SessionMonitor { source }
            | Self::Signal { source, .. }
            | Self::Agent { source }
            | Self::HolderStart { source }
            | Self::HolderWord { source }
            | Self::Detach { source }
            | Self::Run { source, .. }
            | Self::InhibitStop { source, .. } => Some(source),
            Self::SettingsActionConflict { .. }
            | Self::SettingsActionUnknown { .. }
            | Self::SettingsActionNotLast { .. }
            | Self::SettingsCommand { .. }
            | Self::LogindGone
            | Self::HolderGone
            | Self::InhibitRefused { .. } => None,
        }
    }
}

/// The result of the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// An error and the errors under it, on one line.
pub fn chain(error: &dyn std::error::Error) -> String {
    let mut line = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        line.push_str(": ");
        line.push_str(&cause.to_string());
        source = cause.source();
    }
    line
}
