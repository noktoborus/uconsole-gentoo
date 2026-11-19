//! The chains of the settings and the names of their sections. A chain is
//! a group of the file (`main`, `lid`) and a power source; a section is
//! named by its chain and its number from 1: `lid.battery.2`. The
//! journal, the units and the errors of the settings give this name.

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

    /// The name in the file: `battery`, `ac`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Ac => "ac",
        }
    }
}

/// A group of chains of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// `main`: the lid open, or the lid not looked at.
    Main,
    /// `lid`: the lid closed.
    Lid,
}

impl Group {
    /// Both groups.
    pub const ALL: [Self; 2] = [Self::Main, Self::Lid];

    /// The name in the file: `main`, `lid`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Lid => "lid",
        }
    }
}

/// A chain: a group and a power source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chain {
    /// The group.
    pub group: Group,
    /// The power source.
    pub power: Power,
}

impl Chain {
    /// The chain of the group `main` for a power source.
    pub fn main(power: Power) -> Self {
        Self {
            group: Group::Main,
            power,
        }
    }

    /// The chain of the group `lid` for a power source.
    pub fn lid(power: Power) -> Self {
        Self {
            group: Group::Lid,
            power,
        }
    }

    /// Its name: `main.battery`.
    pub fn name(self) -> String {
        format!("{}.{}", self.group.name(), self.power.name())
    }

    /// The name of its section of this index: `main.battery.1`.
    pub fn section_name(self, index: usize) -> String {
        format!("{}.{}", self.name(), index + 1)
    }

    /// The chain and the index of a section, from its name.
    pub fn parse_section_name(name: &str) -> Option<(Self, usize)> {
        let mut parts = name.split('.');
        let group = parts.next()?;
        let power = parts.next()?;
        let number = parts.next()?;
        if parts.next().is_some() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let chain = Self {
            group: Group::ALL.into_iter().find(|g| g.name() == group)?,
            power: Power::ALL.into_iter().find(|p| p.name() == power)?,
        };
        let index = number.parse::<usize>().ok()?.checked_sub(1)?;
        Some((chain, index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_reads_back() {
        assert_eq!(
            Chain::main(Power::Battery).section_name(0),
            "main.battery.1"
        );
        assert_eq!(Chain::lid(Power::Ac).section_name(2), "lid.ac.3");
        for group in Group::ALL {
            for power in Power::ALL {
                let chain = Chain { group, power };
                for index in [0, 1, 9] {
                    let name = chain.section_name(index);
                    assert_eq!(Chain::parse_section_name(&name), Some((chain, index)));
                }
            }
        }
    }

    #[test]
    fn wrong_names() {
        for wrong in [
            "main.battery",
            "main.battery.",
            "main.battery.0",
            "main.battery.x",
            "main.dc.1",
            "top.ac.1",
            "battery-0",
            "main.ac.-1",
            "main.ac.1.2",
        ] {
            assert_eq!(Chain::parse_section_name(wrong), None, "{wrong}");
        }
    }
}
