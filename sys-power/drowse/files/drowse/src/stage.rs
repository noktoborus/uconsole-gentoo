//! `drowsed stage enter|leave <instance>`: the command a unit of a section
//! runs. It reads the command from the settings file and replaces itself
//! by `exec`, with no shell.

use std::fmt;
use std::os::unix::process::CommandExt;
use std::path::Path;

use crate::settings::{self, Command, Power, SectionKind};

/// What a unit of a section runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// `enter` of the section, at the start of the unit.
    Enter,
    /// `leave` of the section, at the stop of the unit.
    Leave,
}

impl Part {
    /// Reads `enter` or `leave`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "enter" => Some(Self::Enter),
            "leave" => Some(Self::Leave),
            _ => None,
        }
    }
}

/// Why a section cannot run its command.
#[derive(Debug)]
pub enum StageError {
    /// The instance is not `battery-<n>` or `ac-<n>`.
    Instance(String),
    /// The settings file cannot be read.
    Settings(crate::Error),
    /// No such section in the settings.
    NoStage(String),
    /// The section is a logind action, not a command.
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
                "{instance}: not an instance of a section (battery-<n>, ac-<n>)"
            ),
            Self::Settings(_) => f.write_str("cannot read the settings"),
            Self::NoStage(instance) => write!(f, "{instance}: no such section in the settings"),
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

/// Reads an instance: its power source and the number of its section.
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

/// The command of a part of a section, if it has one. `None`: nothing to run.
pub fn command_of(
    settings: &settings::Settings,
    instance: &str,
    part: Part,
) -> Result<Option<Command>, StageError> {
    let (power, number) =
        parse_instance(instance).ok_or_else(|| StageError::Instance(instance.to_owned()))?;
    let section = settings
        .sections(power)
        .get(number)
        .ok_or_else(|| StageError::NoStage(instance.to_owned()))?;
    match part {
        Part::Enter => match &section.kind {
            SectionKind::Command { enter, .. } => Ok(enter.clone()),
            SectionKind::Action(_) => Err(StageError::NotACommand(instance.to_owned())),
        },
        Part::Leave => match &section.kind {
            SectionKind::Command { leave, .. } => Ok(leave.clone()),
            SectionKind::Action(_) => Err(StageError::NotACommand(instance.to_owned())),
        },
    }
}

/// Puts the command of a part of a section in place of this process. It
/// returns only when there is nothing to run (`Ok`) or on an error.
pub fn exec(instance: &str, part: Part) -> Result<(), StageError> {
    let settings = settings::load(Path::new(settings::PATH)).map_err(StageError::Settings)?;
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
main:
  battery:
    - after: 0
      enter: tuned-adm profile powersave
      leave: tuned-adm auto_profile
    - action: poweroff
  ac:
    - enter: nmcli radio all off
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
        let settings = settings::parse(TEXT).unwrap();
        let action = command_of(&settings, "battery-0", Part::Enter)
            .unwrap()
            .unwrap();
        assert_eq!(action.words(), ["tuned-adm", "profile", "powersave"]);
        let resume = command_of(&settings, "battery-0", Part::Leave)
            .unwrap()
            .unwrap();
        assert_eq!(resume.words(), ["tuned-adm", "auto_profile"]);
        assert!(
            command_of(&settings, "ac-0", Part::Leave)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            command_of(&settings, "battery-1", Part::Enter),
            Err(StageError::NotACommand(_))
        ));
        assert!(matches!(
            command_of(&settings, "ac-3", Part::Enter),
            Err(StageError::NoStage(_))
        ));
    }
}
