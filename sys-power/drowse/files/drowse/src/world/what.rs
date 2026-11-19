//! The kinds an inhibitor of logind holds, of those drowse looks at:
//! `idle`, `sleep`, `shutdown`. logind gives `what` as the kinds joined
//! by `:`, in an order of its own (`shutdown:sleep`, never
//! `sleep:shutdown`), so `what` is read as a set, never compared as text.

/// A kind of an inhibitor that holds an entry of drowsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// `idle`: holds a section with a command.
    Idle,
    /// `sleep`: holds a sleep of logind.
    Sleep,
    /// `shutdown`: holds a power off of logind.
    Shutdown,
}

/// The kinds of a `what`, as a set. Kinds drowse does not look at
/// (`handle-lid-switch`, ...) are left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Holds {
    /// `idle` is in it.
    idle: bool,
    /// `sleep` is in it.
    sleep: bool,
    /// `shutdown` is in it.
    shutdown: bool,
}

impl Holds {
    /// `idle`.
    pub const IDLE: Self = Self {
        idle: true,
        sleep: false,
        shutdown: false,
    };

    /// `sleep` and `shutdown`: every action of logind drowsed does.
    pub const POWER: Self = Self {
        idle: false,
        sleep: true,
        shutdown: true,
    };

    /// The kinds of a `what` of logind.
    pub fn parse(what: &str) -> Self {
        let mut holds = Self::default();
        for kind in what.split(':') {
            match kind {
                "idle" => holds.idle = true,
                "sleep" => holds.sleep = true,
                "shutdown" => holds.shutdown = true,
                _ => {}
            }
        }
        holds
    }

    /// Whether this kind is in the set.
    pub fn contains(self, hold: Hold) -> bool {
        match hold {
            Hold::Idle => self.idle,
            Hold::Sleep => self.sleep,
            Hold::Shutdown => self.shutdown,
        }
    }

    /// Whether a kind is in both sets.
    pub fn meets(self, other: Self) -> bool {
        (self.idle && other.idle)
            || (self.sleep && other.sleep)
            || (self.shutdown && other.shutdown)
    }

    /// Whether no kind drowse looks at is in the set.
    pub fn is_empty(self) -> bool {
        !(self.idle || self.sleep || self.shutdown)
    }

    /// The `what` to give to `Inhibit` of logind, in the order of logind.
    pub fn what(self) -> String {
        [
            (self.shutdown, "shutdown"),
            (self.sleep, "sleep"),
            (self.idle, "idle"),
        ]
        .into_iter()
        .filter_map(|(is, name)| is.then_some(name))
        .collect::<Vec<_>>()
        .join(":")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_of_what_does_not_count() {
        assert_eq!(Holds::parse("shutdown:sleep"), Holds::POWER);
        assert_eq!(Holds::parse("sleep:shutdown"), Holds::POWER);
        assert_eq!(Holds::parse("idle"), Holds::IDLE);
        for what in [
            "idle:sleep:shutdown",
            "shutdown:idle:sleep",
            "sleep:shutdown:idle",
        ] {
            assert_eq!(Holds::parse(what), Holds::parse("shutdown:sleep:idle"));
        }
    }

    #[test]
    fn a_kind_twice_and_empty_parts_count_as_once_and_nothing() {
        assert_eq!(Holds::parse("sleep:sleep:shutdown"), Holds::POWER);
        assert_eq!(Holds::parse("sleep::shutdown"), Holds::POWER);
        assert_eq!(Holds::parse(":shutdown:sleep:"), Holds::POWER);
    }

    #[test]
    fn other_kinds_are_left_out() {
        assert_eq!(Holds::parse("handle-lid-switch:idle"), Holds::IDLE);
        assert!(Holds::parse("handle-power-key").is_empty());
        assert!(Holds::parse("").is_empty());
    }

    #[test]
    fn what_is_in_the_order_of_logind() {
        assert_eq!(Holds::POWER.what(), "shutdown:sleep");
        assert_eq!(Holds::IDLE.what(), "idle");
        assert_eq!(
            Holds::parse("idle:sleep:shutdown").what(),
            "shutdown:sleep:idle"
        );
    }

    #[test]
    fn contains_and_meets() {
        let sleep = Holds::parse("sleep");
        assert!(sleep.contains(Hold::Sleep));
        assert!(!sleep.contains(Hold::Shutdown));
        assert!(sleep.meets(Holds::POWER));
        assert!(!sleep.meets(Holds::IDLE));
    }
}
