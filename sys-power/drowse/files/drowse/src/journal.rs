//! The journal: one line on stderr with the prefix of its level, which
//! journald reads for a unit of systemd (`sd-daemon(3)`). A line below the
//! verbosity is not written; an error is always written.

use std::sync::atomic::{AtomicU8, Ordering};

/// The level of a line of the journal, the least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Level {
    /// A step that changes nothing outside: idle with nothing entered, its
    /// cancel, a wait for a leave, times moved while not idle.
    Debug,
    /// A change of state.
    #[default]
    Info,
    /// A cancel.
    Notice,
    /// A source not there, a refusal, a command that waits or is killed.
    Warning,
}

impl Level {
    /// Every level, the least first.
    pub const ALL: [Self; 4] = [Self::Debug, Self::Info, Self::Notice, Self::Warning];

    /// The name of the level in the settings and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Notice => "notice",
            Self::Warning => "warning",
        }
    }

    /// The level of a name; none for a name not known.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.name() == name)
    }

    /// The prefix of the level for journald.
    fn prefix(self) -> &'static str {
        match self {
            Self::Debug => "<7>",
            Self::Info => "<6>",
            Self::Notice => "<5>",
            Self::Warning => "<4>",
        }
    }
}

/// The least level written; `Level::Info` until it is set.
static VERBOSITY: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Sets the least level written.
pub fn set_verbosity(level: Level) {
    VERBOSITY.store(level as u8, Ordering::Relaxed);
}

/// Whether a line of this level is written.
fn written(level: Level) -> bool {
    level as u8 >= VERBOSITY.load(Ordering::Relaxed)
}

/// Writes one line to the journal, when its level is not below the
/// verbosity.
pub fn write(level: Level, text: &str) {
    if written(level) {
        eprintln!("{}{}", level.prefix(), text.replace('\n', " "));
    }
}

/// Writes an error and the errors under it, on one line, at the level
/// `err`.
pub fn error(error: &dyn std::error::Error) {
    eprintln!("<3>{}", crate::error::chain(error).replace('\n', " "));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_is_read_by_its_name() {
        for level in Level::ALL {
            assert_eq!(Level::from_name(level.name()), Some(level));
        }
        assert_eq!(Level::from_name("err"), None);
    }

    #[test]
    fn the_levels_go_from_debug_to_warning() {
        assert!(Level::Debug < Level::Info);
        assert!(Level::Info < Level::Notice);
        assert!(Level::Notice < Level::Warning);
    }
}
