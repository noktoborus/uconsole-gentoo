//! What is wrong in the settings file. The service writes each to the
//! journal and goes on: a wrong key is passed over, a wrong value takes its
//! default.

use std::fmt;

use super::Power;

/// A thing wrong in the settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// A `Key=value` before any section.
    KeyOutsideSection {
        /// The line.
        line: usize,
    },
    /// A line that is no section, no entry and no comment.
    NotAnEntry {
        /// The line.
        line: usize,
    },
    /// A section that is not `[Sessions]` and not of a stage.
    UnknownSection {
        /// The name of the section.
        name: String,
        /// The first line of an entry in it.
        line: usize,
    },
    /// A key the service does not know.
    UnknownKey {
        /// The section.
        section: String,
        /// The key.
        key: String,
        /// The line.
        line: usize,
    },
    /// A value that cannot be read; the default is taken.
    WrongValue {
        /// The section.
        section: String,
        /// The key.
        key: String,
        /// The value as written.
        value: String,
        /// The line.
        line: usize,
    },
    /// Both `[Idle<power>]` and `[Idle<power>-0]` are there: one section.
    FirstStageTwice {
        /// The power source of the stage.
        power: Power,
    },
    /// A number is missing in the stages: the ones after it are passed over.
    GapInStages {
        /// The power source of the stages.
        power: Power,
        /// The number that is missing.
        missing: u32,
        /// The sections passed over.
        passed_over: Vec<String>,
    },
    /// `Resume` of a stage whose action is of logind, passed over.
    ResumeOfLogind {
        /// The section.
        section: String,
        /// The line.
        line: usize,
    },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyOutsideSection { line } => {
                write!(f, "line {line}: a key before any section; passed over")
            }
            Self::NotAnEntry { line } => {
                write!(
                    f,
                    "line {line}: not a section, a key or a comment; passed over"
                )
            }
            Self::UnknownSection { name, line } => {
                write!(f, "line {line}: no such section [{name}]; passed over")
            }
            Self::UnknownKey { section, key, line } => {
                write!(
                    f,
                    "line {line}: no such key {key} in [{section}]; passed over"
                )
            }
            Self::WrongValue {
                section,
                key,
                value,
                line,
            } => write!(
                f,
                "line {line}: {key}={value} in [{section}] cannot be read; the default is taken"
            ),
            Self::FirstStageTwice { power } => {
                let base = power.section();
                write!(
                    f,
                    "[{base}] and [{base}-0] are both there; they are one section, the later key wins"
                )
            }
            Self::GapInStages {
                power,
                missing,
                passed_over,
            } => write!(
                f,
                "no [{}-{missing}]; the stages end there, passed over: {}",
                power.section(),
                passed_over.join(", ")
            ),
            Self::ResumeOfLogind { section, line } => write!(
                f,
                "line {line}: Resume in [{section}] is for a command, not an action of logind; passed over"
            ),
        }
    }
}
