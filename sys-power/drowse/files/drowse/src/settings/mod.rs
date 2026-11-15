//! The settings of the service: the file `/etc/drowse.conf`, a section a
//! stage, the stages of the battery and of AC.

pub mod action;
mod syntax;
pub mod time;
pub mod warning;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use action::{Action, Command, LogindAction};
pub use time::Span;
pub use warning::Warning;

use crate::error::{Error, Result};
use syntax::Entry;

/// The path of the settings file.
pub const PATH: &str = "/etc/drowse.conf";

/// A power source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Power {
    /// On battery.
    Battery,
    /// On AC, or the power source is not known.
    Ac,
}

impl Power {
    /// Both power sources.
    pub const ALL: [Self; 2] = [Self::Battery, Self::Ac];

    /// The name of the first section of its stages.
    pub fn section(self) -> &'static str {
        match self {
            Self::Battery => "IdleBattery",
            Self::Ac => "IdleAC",
        }
    }

    /// The name in an instance of a unit of a stage: `battery`, `ac`.
    pub fn instance(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Ac => "ac",
        }
    }

    /// The default idle time of its first stage.
    fn default_idle_time(self) -> Duration {
        match self {
            Self::Battery => Duration::from_secs(15 * 60),
            Self::Ac => Duration::from_secs(60 * 60),
        }
    }
}

/// The default time of the announce.
const DEFAULT_COUNTDOWN: Duration = Duration::from_secs(2 * 60);

/// The default action of the first stage.
const DEFAULT_ACTION: LogindAction = LogindAction::Poweroff;

/// The keys of a stage.
const KEYS: [&str; 5] = ["IdleTime", "Countdown", "Action", "ActionHint", "Resume"];

/// A stage: a count of idle, an announce, an action and its undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// The idle time before the announce; for a further stage, from the
    /// start of the action before it.
    pub idle_time: Span,
    /// How long the announce runs before the action.
    pub countdown: Span,
    /// The action.
    pub action: Action,
    /// The name of the action to show, if set.
    pub hint: Option<String>,
    /// The command that undoes a command of `action`.
    pub resume: Option<Command>,
}

impl Stage {
    /// The name of the action to show: `ActionHint`, or `Action` as written.
    pub fn shown_name(&self) -> &str {
        self.hint
            .as_deref()
            .unwrap_or_else(|| self.action.as_written())
    }

    /// A first stage with only its defaults.
    fn default_for(power: Power) -> Self {
        Self {
            idle_time: Span::Time(power.default_idle_time()),
            countdown: Span::Time(DEFAULT_COUNTDOWN),
            action: Action::Logind(DEFAULT_ACTION),
            hint: None,
            resume: None,
        }
    }
}

/// The settings: the stages of each power source, the first one always
/// there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    battery: Vec<Stage>,
    ac: Vec<Stage>,
}

impl Settings {
    /// The stages of a power source, the first one first.
    pub fn stages(&self, power: Power) -> &[Stage] {
        match power {
            Power::Battery => &self.battery,
            Power::Ac => &self.ac,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            battery: vec![Stage::default_for(Power::Battery)],
            ac: vec![Stage::default_for(Power::Ac)],
        }
    }
}

/// Reads the settings file. No file gives the defaults; a file that is
/// there but cannot be read is an error.
pub fn load(path: &Path) -> Result<(Settings, Vec<Warning>)> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(parse(&text)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok((Settings::default(), Vec::new()))
        }
        Err(source) => Err(Error::ReadSettings {
            path: PathBuf::from(path),
            source,
        }),
    }
}

/// A value of a key as written, with its line.
#[derive(Debug, Clone)]
struct Written {
    value: String,
    line: usize,
}

/// The keys of one section of a stage as written; a later key wins.
#[derive(Debug, Clone, Default)]
struct RawStage {
    section: String,
    keys: BTreeMap<&'static str, Written>,
}

/// Reads the text of a settings file.
pub fn parse(text: &str) -> (Settings, Vec<Warning>) {
    let (entries, mut warnings) = syntax::parse(text);
    let raw = collect(&entries, &mut warnings);
    let mut stages_of = |power: Power| build_stages(power, &raw, &mut warnings);
    let battery = stages_of(Power::Battery);
    let ac = stages_of(Power::Ac);
    (Settings { battery, ac }, warnings)
}

/// Sorts the entries into the stages they belong to.
fn collect(entries: &[Entry], warnings: &mut Vec<Warning>) -> BTreeMap<(Power, u32), RawStage> {
    let mut raw: BTreeMap<(Power, u32), RawStage> = BTreeMap::new();
    let mut unknown_sections = BTreeSet::new();
    let mut first_written_as: BTreeMap<Power, BTreeSet<bool>> = BTreeMap::new();
    for entry in entries {
        let Some(place) = stage_of_section(&entry.section) else {
            if unknown_sections.insert(entry.section.clone()) {
                warnings.push(Warning::UnknownSection {
                    name: entry.section.clone(),
                    line: entry.line,
                });
            }
            continue;
        };
        if place.number == 0 {
            first_written_as
                .entry(place.power)
                .or_default()
                .insert(place.with_number);
        }
        let Some(key) = KEYS.iter().find(|known| **known == entry.key) else {
            warnings.push(Warning::UnknownKey {
                section: entry.section.clone(),
                key: entry.key.clone(),
                line: entry.line,
            });
            continue;
        };
        let stage = raw.entry((place.power, place.number)).or_default();
        if stage.section.is_empty() {
            stage.section = entry.section.clone();
        }
        stage.keys.insert(
            key,
            Written {
                value: entry.value.clone(),
                line: entry.line,
            },
        );
    }
    for (power, forms) in first_written_as {
        if forms.len() > 1 {
            warnings.push(Warning::FirstStageTwice { power });
        }
    }
    raw
}

/// Where a section belongs: its power source, its number, and whether the
/// number was written (`-0`).
struct Place {
    power: Power,
    number: u32,
    with_number: bool,
}

/// The stage a section name names, if it names one.
fn stage_of_section(name: &str) -> Option<Place> {
    Power::ALL.into_iter().find_map(|power| {
        let rest = name.strip_prefix(power.section())?;
        if rest.is_empty() {
            return Some(Place {
                power,
                number: 0,
                with_number: false,
            });
        }
        let digits = rest.strip_prefix('-')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Place {
            power,
            number: digits.parse().ok()?,
            with_number: true,
        })
    })
}

/// Builds the stages of a power source: the first, then the next ones by
/// number while the numbers go one by one and each has an `Action`.
fn build_stages(
    power: Power,
    raw: &BTreeMap<(Power, u32), RawStage>,
    warnings: &mut Vec<Warning>,
) -> Vec<Stage> {
    let first = raw.get(&(power, 0));
    let mut stages = vec![build_first(power, first, warnings)];
    let mut number = 1;
    while let Some(next) = raw.get(&(power, number)) {
        match build_further(power, next, warnings) {
            Some(stage) => stages.push(stage),
            None => {
                warnings.push(Warning::StageWithNoAction {
                    section: next.section.clone(),
                });
                return stages;
            }
        }
        number += 1;
    }
    let passed_over: Vec<String> = raw
        .range((power, number)..=(power, u32::MAX))
        .map(|(_, stage)| stage.section.clone())
        .collect();
    if !passed_over.is_empty() {
        warnings.push(Warning::GapInStages {
            power,
            missing: number,
            passed_over,
        });
    }
    stages
}

/// Builds the first stage: each key not set or wrong takes its default.
fn build_first(power: Power, raw: Option<&RawStage>, warnings: &mut Vec<Warning>) -> Stage {
    let defaults = Stage::default_for(power);
    let Some(raw) = raw else {
        return defaults;
    };
    let action = read_key(raw, "Action", Action::parse, warnings).unwrap_or(defaults.action);
    finish_stage(power, raw, action, warnings)
}

/// Builds a further stage; none when its `Action` is not set or wrong.
fn build_further(power: Power, raw: &RawStage, warnings: &mut Vec<Warning>) -> Option<Stage> {
    let action = read_key(raw, "Action", Action::parse, warnings)?;
    Some(finish_stage(power, raw, action, warnings))
}

/// Reads the keys of a stage other than `Action`.
fn finish_stage(
    power: Power,
    raw: &RawStage,
    action: Action,
    warnings: &mut Vec<Warning>,
) -> Stage {
    let defaults = Stage::default_for(power);
    let idle_time = read_key(raw, "IdleTime", time::parse, warnings).unwrap_or(defaults.idle_time);
    let countdown = read_key(raw, "Countdown", time::parse, warnings).unwrap_or(defaults.countdown);
    let hint = raw
        .keys
        .get("ActionHint")
        .map(|written| written.value.clone())
        .filter(|hint| !hint.is_empty());
    let resume = read_resume(raw, &action, warnings);
    Stage {
        idle_time,
        countdown,
        action,
        hint,
        resume,
    }
}

/// Reads `Resume`: an empty value is none; for an action of logind it is
/// passed over.
fn read_resume(raw: &RawStage, action: &Action, warnings: &mut Vec<Warning>) -> Option<Command> {
    let written = raw.keys.get("Resume").filter(|w| !w.value.is_empty())?;
    if let Action::Logind(_) = action {
        warnings.push(Warning::ResumeOfLogind {
            section: raw.section.clone(),
            line: written.line,
        });
        return None;
    }
    read_key(raw, "Resume", Command::parse, warnings)
}

/// Reads a key that is set; a value that cannot be read is a warning.
fn read_key<T>(
    raw: &RawStage,
    key: &'static str,
    read: impl Fn(&str) -> Option<T>,
    warnings: &mut Vec<Warning>,
) -> Option<T> {
    let written = raw.keys.get(key)?;
    let value = read(&written.value);
    if value.is_none() {
        warnings.push(Warning::WrongValue {
            section: raw.section.clone(),
            key: key.to_owned(),
            value: written.value.clone(),
            line: written.line,
        });
    }
    value
}

#[cfg(test)]
mod tests;
