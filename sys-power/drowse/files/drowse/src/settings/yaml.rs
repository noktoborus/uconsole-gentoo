//! The settings file in YAML.

use std::fmt;
use std::time::Duration;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};

use crate::error::{Error, Result};

use super::action::{Command, LogindAction};
use super::time::{self, Span};

/// Reads and validates a settings file.
pub fn parse(text: &str) -> Result<SettingsFile> {
    let mut value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(text).map_err(|source| Error::SettingsYaml { source })?;
    value
        .apply_merge()
        .map_err(|source| Error::SettingsYaml { source })?;
    let raw: Option<RawFile> =
        serde_yaml_ng::from_value(value).map_err(|source| Error::SettingsYaml { source })?;
    raw.unwrap_or_default().try_into()
}

/// The whole settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsFile {
    /// The session classes that count.
    pub sessions: Sessions,
    /// Sections that chains can refer to with YAML aliases.
    pub sections: Vec<Section>,
    /// The chain used with the lid open or when no lid is set.
    pub main: Chains,
    /// The chain used with the lid closed, if set.
    pub lid: Option<Chains>,
}

/// The session settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sessions {
    /// The logind classes that count.
    pub classes: Vec<String>,
}

impl Default for Sessions {
    fn default() -> Self {
        Self {
            classes: super::sessions::DEFAULT
                .iter()
                .map(|class| (*class).to_owned())
                .collect(),
        }
    }
}

/// The battery and AC chains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chains {
    /// Sections used on battery.
    pub battery: Vec<Section>,
    /// Sections used on AC.
    pub ac: Vec<Section>,
}

/// One section in a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The idle time before entry.
    pub after: Span,
    /// The announce time before entry.
    pub countdown: Span,
    /// The text shown for this section.
    pub hint: Option<String>,
    /// What entering the section does.
    pub kind: SectionKind,
}

impl Section {
    /// The name to show: `hint`, or the action or command as written.
    pub fn shown_name(&self) -> &str {
        self.hint.as_deref().unwrap_or_else(|| match &self.kind {
            SectionKind::Action(action) => action.name(),
            SectionKind::Command {
                enter: Some(command),
                ..
            } => command.line(),
            SectionKind::Command { enter: None, .. } => "",
        })
    }
}

/// The kind of section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionKind {
    /// A command section, or an empty section.
    Command {
        /// The command run on entry.
        enter: Option<Command>,
        /// The command run on leave.
        leave: Option<Command>,
    },
    /// An action run by logind.
    Action(LogindAction),
}

impl Section {
    fn from_raw(raw: RawSection, place: &str) -> Result<Self> {
        let has_command = raw.enter.is_some() || raw.leave.is_some();
        let kind = match raw.action {
            Some(_) if has_command => {
                return Err(Error::SettingsActionConflict {
                    place: place.to_owned(),
                });
            }
            Some(action) => SectionKind::Action(parse_logind_action(&action, place)?),
            None => SectionKind::Command {
                enter: parse_command(raw.enter, place)?,
                leave: parse_command(raw.leave, place)?,
            },
        };
        let default_countdown = match kind {
            SectionKind::Action(_) => Span::Time(Duration::from_secs(120)),
            SectionKind::Command { .. } => Span::Never,
        };
        let hint = raw.hint.or_else(|| match &kind {
            SectionKind::Action(action) => Some(action.name().to_owned()),
            SectionKind::Command {
                enter: Some(command),
                ..
            } => Some(command.line().to_owned()),
            SectionKind::Command { enter: None, .. } => None,
        });
        Ok(Self {
            after: raw.after.map_or(Span::Never, |time| time.0),
            countdown: raw.countdown.map_or(default_countdown, |time| time.0),
            hint,
            kind,
        })
    }
}

fn parse_logind_action(action: &str, place: &str) -> Result<LogindAction> {
    let allowed = [
        LogindAction::Poweroff,
        LogindAction::Halt,
        LogindAction::Suspend,
        LogindAction::Hibernate,
        LogindAction::HybridSleep,
        LogindAction::SuspendThenHibernate,
        LogindAction::Sleep,
    ];
    LogindAction::from_name(action)
        .filter(|action| allowed.contains(action))
        .ok_or_else(|| Error::SettingsActionUnknown {
            action: action.to_owned(),
            place: place.to_owned(),
        })
}

fn parse_command(command: Option<String>, place: &str) -> Result<Option<Command>> {
    command
        .map(|line| {
            Command::parse(&line).ok_or_else(|| Error::SettingsCommand {
                command: line,
                place: place.to_owned(),
            })
        })
        .transpose()
}

impl TryFrom<RawFile> for SettingsFile {
    type Error = Error;

    fn try_from(raw: RawFile) -> Result<Self> {
        let mut sections = Vec::with_capacity(raw.sections.len());
        for (index, section) in raw.sections.into_iter().enumerate() {
            sections.push(Section::from_raw(
                section,
                &format!("sections[{}]", index + 1),
            )?);
        }
        let main = raw.main.into_chains("main")?;
        let lid = raw
            .lid
            .map(|chains| chains.into_chains("lid"))
            .transpose()?;
        let file = Self {
            sessions: Sessions {
                classes: raw
                    .sessions
                    .classes
                    .unwrap_or_else(|| Sessions::default().classes),
            },
            sections,
            main,
            lid,
        };
        validate_last_actions(&file.main, "main")?;
        if let Some(lid) = &file.lid {
            validate_last_actions(lid, "lid")?;
        }
        Ok(file)
    }
}

impl RawChains {
    fn into_chains(self, name: &str) -> Result<Chains> {
        Ok(Chains {
            battery: build_chain(self.battery, name, "battery")?,
            ac: build_chain(self.ac, name, "ac")?,
        })
    }
}

fn build_chain(raw: Vec<RawSection>, parent: &str, power: &str) -> Result<Vec<Section>> {
    raw.into_iter()
        .enumerate()
        .map(|(index, section)| {
            Section::from_raw(section, &format!("{parent}.{power}[{}]", index + 1))
        })
        .collect()
}

fn validate_last_actions(chains: &Chains, parent: &str) -> Result<()> {
    for (name, sections) in [("battery", &chains.battery), ("ac", &chains.ac)] {
        for (index, section) in sections.iter().enumerate() {
            if matches!(section.kind, SectionKind::Action(_)) && index + 1 != sections.len() {
                return Err(Error::SettingsActionNotLast {
                    chain: format!("{parent}.{name}"),
                    section: index,
                });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    sessions: RawSessions,
    #[serde(default)]
    sections: Vec<RawSection>,
    #[serde(default)]
    main: RawChains,
    #[serde(default)]
    lid: Option<RawChains>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSessions {
    classes: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChains {
    #[serde(default)]
    battery: Vec<RawSection>,
    #[serde(default)]
    ac: Vec<RawSection>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSection {
    after: Option<YamlTime>,
    countdown: Option<YamlTime>,
    hint: Option<String>,
    enter: Option<String>,
    leave: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct YamlTime(Span);

impl<'de> Deserialize<'de> for YamlTime {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TimeVisitor)
    }
}

struct TimeVisitor;

impl Visitor<'_> for TimeVisitor {
    type Value = YamlTime;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a number of seconds or a time string")
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlTime(Span::Time(Duration::from_secs(value))))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        let parsed = if value == "never" {
            Some(Span::Never)
        } else {
            time::parse(value)
        };
        parsed
            .map(YamlTime)
            .ok_or_else(|| E::custom("invalid time"))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn seconds(value: u64) -> Span {
        Span::Time(Duration::from_secs(value))
    }

    #[test]
    fn reads_the_example_and_resolves_aliases() {
        let file = parse(
            "sections:\n  - &save\n    after: 10m\n    enter: tuned-adm profile cm5-powersave\n    leave: systemctl try-restart tuned-ppd.service\n    hint: power save\nmain:\n  battery: [*save, {action: poweroff, after: 28m}]\n  ac: []\n",
        )
        .unwrap();
        assert_eq!(file.main.battery.len(), 2);
        assert_eq!(file.main.battery[0].after, seconds(600));
        assert_eq!(file.main.battery[0].hint.as_deref(), Some("power save"));
        assert_eq!(file.main.battery[1].countdown, seconds(120));
        assert!(file.main.ac.is_empty());
    }

    #[test]
    fn missing_alias_anchor_is_an_error() {
        assert!(parse("main:\n  battery: [*missing]\n").is_err());
    }

    #[test]
    fn reports_the_line_of_a_yaml_error() {
        let Err(error) = parse("main:\n  battery: [\n") else {
            panic!("expected a YAML error");
        };
        assert!(error.to_string().contains("line 3"));
    }

    #[test]
    fn merge_changes_one_key_and_keeps_the_rest() {
        let file = parse(
            "sections:\n  - &save\n    after: 10m\n    enter: tuned-adm profile cm5-powersave\n    hint: power save\nmain:\n  battery:\n    - <<: *save\n      after: 0\n",
        )
        .unwrap();
        let section = &file.main.battery[0];
        assert_eq!(section.after, seconds(0));
        assert_eq!(section.hint.as_deref(), Some("power save"));
        assert!(matches!(
            section.kind,
            SectionKind::Command { enter: Some(_), .. }
        ));
    }

    #[test]
    fn rejects_unknown_keys_and_wrong_types() {
        assert!(parse("unknown: true\n").is_err());
        assert!(parse("main: wrong\n").is_err());
        assert!(parse("main:\n  battery:\n    - after: true\n").is_err());
    }

    #[test]
    fn rejects_unknown_and_unsupported_logind_actions() {
        assert!(matches!(
            parse("main:\n  battery: [{action: power-down}]\n"),
            Err(Error::SettingsActionUnknown { .. })
        ));
        assert!(matches!(
            parse("main:\n  battery: [{action: reboot}]\n"),
            Err(Error::SettingsActionUnknown { .. })
        ));
    }

    #[test]
    fn rejects_action_with_enter_or_leave() {
        assert!(matches!(
            parse("main:\n  battery: [{action: poweroff, enter: echo}]\n"),
            Err(Error::SettingsActionConflict { .. })
        ));
        assert!(matches!(
            parse("main:\n  battery: [{action: poweroff, leave: echo}]\n"),
            Err(Error::SettingsActionConflict { .. })
        ));
    }

    #[test]
    fn logind_action_must_be_last() {
        let Err(Error::SettingsActionNotLast { chain, section }) =
            parse("main:\n  battery: [{action: poweroff}, {enter: echo}]\n")
        else {
            panic!("expected an action position error");
        };
        assert_eq!(chain, "main.battery");
        assert_eq!(section, 0);
    }

    #[test]
    fn applies_countdown_and_hint_defaults() {
        let file = parse("main:\n  battery: [{enter: echo}, {}, {action: poweroff}]\n").unwrap();
        assert_eq!(file.main.battery[0].countdown, Span::Never);
        assert_eq!(file.main.battery[0].hint.as_deref(), Some("echo"));
        assert_eq!(file.main.battery[1].countdown, Span::Never);
        assert_eq!(file.main.battery[1].hint, None);
        assert_eq!(file.main.battery[2].countdown, seconds(120));
        assert_eq!(file.main.battery[2].hint.as_deref(), Some("poweroff"));
    }

    #[test]
    fn enter_poweroff_is_a_command() {
        let file = parse("main:\n  battery: [{enter: poweroff}]\n").unwrap();
        assert!(matches!(
            &file.main.battery[0].kind,
            SectionKind::Command { enter: Some(command), .. } if command.words() == ["poweroff"]
        ));
    }

    #[test]
    fn absent_and_empty_chains_are_empty() {
        let file = parse("").unwrap();
        assert_eq!(file.sessions.classes.len(), 5);
        assert!(file.main.battery.is_empty());
        assert!(file.lid.is_none());
        let file = parse("lid:\n  battery: []\n").unwrap();
        assert!(file.lid.unwrap().ac.is_empty());
    }
}
