//! The settings of the service, read from `/etc/drowse.yaml`.

pub mod action;
pub mod chain;
pub mod sessions;
pub mod time;
pub mod yaml;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub use action::{Command, LogindAction};
pub use chain::{Chain, Group, Power};
pub use time::Span;
pub use yaml::{Chains, Section, SectionKind};

use crate::error::{Error, Result};
use crate::journal::Level;

/// The path of the settings file.
pub const PATH: &str = "/etc/drowse.yaml";

/// The settings used by the running service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    verbosity: Level,
    main: Chains,
    lid: Option<Chains>,
    classes: BTreeSet<String>,
}

impl Settings {
    /// The least level of the journal written.
    pub fn verbosity(&self) -> Level {
        self.verbosity
    }

    /// Sets the least level of the journal written, in place of that of
    /// the file.
    pub fn set_verbosity(&mut self, level: Level) {
        self.verbosity = level;
    }

    /// Whether a session of this class counts in the idle state.
    pub fn counts_class(&self, class: &str) -> bool {
        self.classes.contains(class)
    }

    /// The sections of a chain, the first one first. A chain not set is
    /// empty.
    pub fn sections(&self, chain: Chain) -> &[Section] {
        let chains = match chain.group {
            Group::Main => Some(&self.main),
            Group::Lid => self.lid.as_ref(),
        };
        chains.map_or(&[], |chains| chains.get(chain.power))
    }

    /// The chain in use: `lid` with the lid closed, when the file has
    /// `lid:`; else `main`, and the lid is not looked at.
    pub fn chain(&self, lid_closed: bool, power: Power) -> Chain {
        if lid_closed && self.lid.is_some() {
            Chain::lid(power)
        } else {
            Chain::main(power)
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            verbosity: Level::default(),
            main: Chains::default(),
            lid: None,
            classes: sessions::default_classes(),
        }
    }
}

/// Reads settings. No file or an empty file gives empty chains.
pub fn load(path: &Path) -> Result<Settings> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(source) => Err(Error::ReadSettings {
            path: PathBuf::from(path),
            source,
        }),
    }
}

/// Reads and validates a settings file.
pub fn parse(text: &str) -> Result<Settings> {
    let file = yaml::parse(text)?;
    Ok(Settings {
        verbosity: file.verbosity,
        main: file.main,
        lid: file.lid,
        classes: file.sessions.classes.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_section_has_one_name_in_the_unit_and_the_errors() {
        let chain = Chain::lid(Power::Battery);
        assert_eq!(
            crate::engine::unit_name(chain, 1),
            "drowsed-stage@lid.battery.2.service"
        );
        let error = parse("lid:\n  battery: [{after: 1m}, {after: 1m, action: nap}]\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains(&chain.section_name(1)), "{error}");
    }

    #[test]
    fn the_lid_picks_the_chain_only_when_the_file_has_lid() {
        let main = parse("main:\n  battery: [{after: 1m}]\n").unwrap();
        assert_eq!(
            main.chain(true, Power::Battery),
            Chain::main(Power::Battery)
        );
        let lid = parse("lid:\n  battery: [{after: 0}]\n").unwrap();
        assert_eq!(lid.chain(false, Power::Ac), Chain::main(Power::Ac));
        assert_eq!(lid.chain(true, Power::Ac), Chain::lid(Power::Ac));
        assert!(lid.sections(Chain::lid(Power::Ac)).is_empty());
        assert_eq!(lid.sections(Chain::lid(Power::Battery)).len(), 1);
        assert!(main.sections(Chain::lid(Power::Battery)).is_empty());
    }

    #[test]
    fn the_verbosity_is_info_unless_the_file_sets_it() {
        assert_eq!(parse("").unwrap().verbosity(), Level::Info);
        assert_eq!(
            parse("verbosity: debug\n").unwrap().verbosity(),
            Level::Debug
        );
        let error = parse("verbosity: loud\n").unwrap_err().to_string();
        assert!(error.contains("loud"), "{error}");
    }

    #[test]
    fn no_file_is_empty() {
        let path = std::env::temp_dir().join(format!("drowse-no-file-{}", std::process::id()));
        assert_eq!(load(&path).unwrap(), Settings::default());
    }

    #[test]
    fn package_settings_are_valid_and_keep_the_main_chain() {
        let settings = parse(include_str!("../../etc/drowse.yaml")).unwrap();
        let battery = settings.sections(Chain::main(Power::Battery));
        assert_eq!(battery.len(), 2);
        assert_eq!(battery[0].after, Span::Time(Duration::from_secs(600)));
        assert_eq!(battery[0].countdown, Span::Never);
        assert_eq!(battery[0].hint.as_deref(), Some("power save"));
        assert!(matches!(
            &battery[0].kind,
            SectionKind::Command {
                enter: Some(command),
                leave: Some(_),
            } if command.line() == "/usr/libexec/drowse/drowsed-tuned enter cm5-powersave"
        ));
        assert_eq!(battery[1].after, Span::Time(Duration::from_secs(38 * 60)));
        assert_eq!(battery[1].countdown, Span::Time(Duration::from_secs(120)));
        assert!(matches!(
            battery[1].kind,
            SectionKind::Action(LogindAction::Poweroff)
        ));
        assert_eq!(settings.sections(Chain::main(Power::Ac)).len(), 0);
        assert!(settings.counts_class("greeter"));
        assert!(!settings.counts_class("background"));
        assert_eq!(settings.verbosity(), Level::Info);

        let file = yaml::parse(include_str!("../../etc/drowse.yaml")).unwrap();
        assert_eq!(file.sections.len(), 1);
        assert!(file.lid.is_none());
    }
}
