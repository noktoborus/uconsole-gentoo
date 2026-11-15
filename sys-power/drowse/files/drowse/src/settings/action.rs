//! The action of a stage: an action of logind or a command.

use std::fmt;

/// An action of logind, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogindAction {
    /// `poweroff`.
    Poweroff,
    /// `halt`.
    Halt,
    /// `reboot`.
    Reboot,
    /// `soft-reboot`.
    SoftReboot,
    /// `kexec`.
    Kexec,
    /// `suspend`.
    Suspend,
    /// `hibernate`.
    Hibernate,
    /// `hybrid-sleep`.
    HybridSleep,
    /// `suspend-then-hibernate`.
    SuspendThenHibernate,
    /// `sleep`.
    Sleep,
    /// `lock`.
    Lock,
    /// `ignore`.
    Ignore,
}

/// Every action of logind with its name.
const LOGIND_ACTIONS: &[(LogindAction, &str)] = &[
    (LogindAction::Poweroff, "poweroff"),
    (LogindAction::Halt, "halt"),
    (LogindAction::Reboot, "reboot"),
    (LogindAction::SoftReboot, "soft-reboot"),
    (LogindAction::Kexec, "kexec"),
    (LogindAction::Suspend, "suspend"),
    (LogindAction::Hibernate, "hibernate"),
    (LogindAction::HybridSleep, "hybrid-sleep"),
    (LogindAction::SuspendThenHibernate, "suspend-then-hibernate"),
    (LogindAction::Sleep, "sleep"),
    (LogindAction::Lock, "lock"),
    (LogindAction::Ignore, "ignore"),
];

impl LogindAction {
    /// The action of logind with this name, if there is one.
    pub fn from_name(name: &str) -> Option<Self> {
        LOGIND_ACTIONS
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(action, _)| *action)
    }

    /// The name of the action, as logind and the settings write it.
    pub fn name(self) -> &'static str {
        LOGIND_ACTIONS
            .iter()
            .find(|(action, _)| *action == self)
            .map(|(_, name)| *name)
            .unwrap_or_default()
    }
}

impl fmt::Display for LogindAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A command of the settings: its line as written, and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    line: String,
    words: Vec<String>,
}

impl Command {
    /// Splits a line as a shell does, with no shell: quotes and
    /// backslashes, no `|`, `&&` or `$VAR`. None for a line with no words
    /// or with a quote not closed.
    pub fn parse(line: &str) -> Option<Self> {
        let words = shlex::split(line)?;
        if words.is_empty() {
            return None;
        }
        Some(Self {
            line: line.trim().to_owned(),
            words,
        })
    }

    /// The line as written in the settings.
    pub fn line(&self) -> &str {
        &self.line
    }

    /// The words: the program, then its arguments.
    pub fn words(&self) -> &[String] {
        &self.words
    }
}

/// The action of a stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// An action of logind.
    Logind(LogindAction),
    /// A command, run in the unit of its stage.
    Command(Command),
}

impl Action {
    /// Reads `Action=`: a name of logind, else a command.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(action) = LogindAction::from_name(value) {
            return Some(Self::Logind(action));
        }
        Command::parse(value).map(Self::Command)
    }

    /// The action as it is written: the name of logind or the line.
    pub fn as_written(&self) -> &str {
        match self {
            Self::Logind(action) => action.name(),
            Self::Command(command) => command.line(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logind_names() {
        for (action, name) in LOGIND_ACTIONS {
            assert_eq!(LogindAction::from_name(name), Some(*action));
            assert_eq!(action.name(), *name);
        }
        assert_eq!(LogindAction::from_name("PowerOff"), None);
    }

    #[test]
    fn action_of_logind() {
        assert_eq!(
            Action::parse(" poweroff "),
            Some(Action::Logind(LogindAction::Poweroff))
        );
    }

    #[test]
    fn action_command() {
        let Some(Action::Command(command)) = Action::parse("nmcli radio all off") else {
            panic!("not a command");
        };
        assert_eq!(command.words(), ["nmcli", "radio", "all", "off"]);
        assert_eq!(command.line(), "nmcli radio all off");
    }

    #[test]
    fn command_quotes() {
        let command = Command::parse(r#"/usr/bin/foo "a b" 'c d' e\ f"#).unwrap();
        assert_eq!(command.words(), ["/usr/bin/foo", "a b", "c d", "e f"]);
    }

    #[test]
    fn command_wrong() {
        assert_eq!(Command::parse(""), None);
        assert_eq!(Command::parse("   "), None);
        assert_eq!(Command::parse("foo \"bar"), None);
    }

    #[test]
    fn no_shell() {
        let command = Command::parse("echo $HOME | cat").unwrap();
        assert_eq!(command.words(), ["echo", "$HOME", "|", "cat"]);
    }
}
