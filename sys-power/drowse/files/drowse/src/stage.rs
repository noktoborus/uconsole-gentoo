//! `drowsed stage action|resume <instance>`: the command a unit of a stage
//! runs. It reads the line of the stage from the settings file and puts
//! the command in its place by `exec`, with no shell.

use std::fmt;
use std::os::unix::process::CommandExt;
use std::path::Path;

use crate::settings::{self, Action, Command, Power};

/// What a unit of a stage runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// `Action` of the stage, at the start of the unit.
    Action,
    /// `Resume` of the stage, at the stop of the unit.
    Resume,
}

impl Part {
    /// Reads `action` or `resume`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "action" => Some(Self::Action),
            "resume" => Some(Self::Resume),
            _ => None,
        }
    }
}

/// Why a stage cannot run its command.
#[derive(Debug)]
pub enum StageError {
    /// The instance is not `battery-<n>` or `ac-<n>`.
    Instance(String),
    /// The settings file cannot be read.
    Settings(crate::Error),
    /// No such stage in the settings.
    NoStage(String),
    /// The action of the stage is of logind, not a command.
    NotACommand(String),
    /// The command cannot be put in place.
    Exec {
        /// The program.
        program: String,
        /// The error of the system.
        source: std::io::Error,
    },
}

impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instance(instance) => write!(
                f,
                "{instance}: not an instance of a stage (battery-<n>, ac-<n>)"
            ),
            Self::Settings(_) => f.write_str("cannot read the settings"),
            Self::NoStage(instance) => write!(f, "{instance}: no such stage in the settings"),
            Self::NotACommand(instance) => {
                write!(f, "{instance}: the action is of logind, not a command")
            }
            Self::Exec { program, .. } => write!(f, "cannot run {program}"),
        }
    }
}

impl std::error::Error for StageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Settings(source) => Some(source),
            Self::Exec { source, .. } => Some(source),
            Self::Instance(_) | Self::NoStage(_) | Self::NotACommand(_) => None,
        }
    }
}

/// Reads an instance: its power source and the number of its stage.
pub fn parse_instance(instance: &str) -> Option<(Power, usize)> {
    let (power, number) = instance.rsplit_once('-')?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let power = Power::ALL
        .into_iter()
        .find(|known| known.instance() == power)?;
    Some((power, number.parse().ok()?))
}

/// The command of a part of a stage, if it has one. `None`: nothing to run.
pub fn command_of(
    settings: &settings::Settings,
    instance: &str,
    part: Part,
) -> Result<Option<Command>, StageError> {
    let (power, number) =
        parse_instance(instance).ok_or_else(|| StageError::Instance(instance.to_owned()))?;
    let stage = settings
        .stages(power)
        .get(number)
        .ok_or_else(|| StageError::NoStage(instance.to_owned()))?;
    match part {
        Part::Action => match &stage.action {
            Some(Action::Command(command)) => Ok(Some(command.clone())),
            Some(Action::Logind(_)) => Err(StageError::NotACommand(instance.to_owned())),
            None => Ok(None),
        },
        Part::Resume => Ok(stage.resume.clone()),
    }
}

/// Puts the command of a part of a stage in place of this process. It
/// returns only when there is nothing to run (`Ok`) or on an error.
pub fn exec(instance: &str, part: Part) -> Result<(), StageError> {
    let (settings, _) = settings::load(Path::new(settings::PATH)).map_err(StageError::Settings)?;
    let Some(command) = command_of(&settings, instance, part)? else {
        return Ok(());
    };
    let (program, args) = command
        .words()
        .split_first()
        .ok_or_else(|| StageError::NoStage(instance.to_owned()))?;
    let source = std::process::Command::new(program).args(args).exec();
    Err(StageError::Exec {
        program: program.clone(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "\
[IdleBattery]
IdleTime=0
Countdown=Never
Action=tuned-adm profile powersave
Resume=tuned-adm auto_profile

[IdleBattery-1]
Action=poweroff

[IdleAC]
Action=nmcli radio all off
";

    #[test]
    fn instances() {
        assert_eq!(parse_instance("battery-0"), Some((Power::Battery, 0)));
        assert_eq!(parse_instance("ac-12"), Some((Power::Ac, 12)));
        for wrong in ["battery", "battery-", "battery-x", "dc-1", "ac--1", "-1"] {
            assert_eq!(parse_instance(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn the_parts_of_a_stage() {
        let (settings, _) = settings::parse(TEXT);
        let action = command_of(&settings, "battery-0", Part::Action)
            .unwrap()
            .unwrap();
        assert_eq!(action.words(), ["tuned-adm", "profile", "powersave"]);
        let resume = command_of(&settings, "battery-0", Part::Resume)
            .unwrap()
            .unwrap();
        assert_eq!(resume.words(), ["tuned-adm", "auto_profile"]);
        assert!(
            command_of(&settings, "ac-0", Part::Resume)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            command_of(&settings, "battery-1", Part::Action),
            Err(StageError::NotACommand(_))
        ));
        assert!(matches!(
            command_of(&settings, "ac-3", Part::Action),
            Err(StageError::NoStage(_))
        ));
    }
}
