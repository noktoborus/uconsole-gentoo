//! Which logind session classes count in the idle state.

use std::collections::BTreeSet;

/// The name of the section.
pub const SECTION: &str = "Sessions";

/// The key of the classes that count.
pub const CLASSES: &str = "Classes";

/// The keys of the section.
pub const KEYS: [&str; 1] = [CLASSES];

/// The classes that count by default: those logind counts
/// (`SESSION_CLASS_CAN_IDLE`).
pub const DEFAULT: [&str; 5] = [
    "user",
    "user-early",
    "user-light",
    "user-early-light",
    "greeter",
];

/// The default classes, as a set.
pub fn default_classes() -> BTreeSet<String> {
    DEFAULT.into_iter().map(str::to_owned).collect()
}

/// Reads the classes: names split by spaces; empty is none. Any name is
/// taken, so a class of a newer logind works too; a name is compared with
/// the class of a session as it is.
pub fn parse(value: &str) -> Option<BTreeSet<String>> {
    Some(value.split_whitespace().map(str::to_owned).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_by_spaces() {
        let classes = parse("user  greeter user").unwrap();
        assert_eq!(classes, ["greeter", "user"].map(str::to_owned).into());
    }

    #[test]
    fn empty_is_none_counted() {
        assert_eq!(parse(""), Some(BTreeSet::new()));
    }

    #[test]
    fn any_name_is_taken() {
        assert_eq!(parse("user-new"), Some(["user-new".to_owned()].into()));
    }
}
