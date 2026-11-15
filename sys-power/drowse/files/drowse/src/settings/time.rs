//! A time in the settings: a time span in the form of systemd, or `Never`.

use std::time::Duration;

/// A time of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    /// A time span.
    Time(Duration),
    /// No time: the thing does not come.
    Never,
}

/// The word for no time.
const NEVER: &str = "Never";

/// The units of a time span and their length in seconds.
const UNITS: &[(&str, u64)] = &[
    ("sec", 1),
    ("s", 1),
    ("min", 60),
    ("m", 60),
    ("hr", 3600),
    ("h", 3600),
];

/// Reads a time: `Never`, or one or more parts of a number and a unit,
/// added. A number with no unit is seconds. Spaces between the parts and
/// between a number and its unit may be there or not.
pub fn parse(text: &str) -> Option<Span> {
    let text = text.trim();
    if text == NEVER {
        return Some(Span::Never);
    }
    parse_parts(text).map(Span::Time)
}

/// Reads the parts of a time span and adds them.
fn parse_parts(text: &str) -> Option<Duration> {
    if text.is_empty() {
        return None;
    }
    let mut rest = text;
    let mut total: u64 = 0;
    while !rest.is_empty() {
        let digits = leading_len(rest, |c| c.is_ascii_digit());
        if digits == 0 {
            return None;
        }
        let number: u64 = rest[..digits].parse().ok()?;
        rest = rest[digits..].trim_start();
        let (unit_seconds, unit_len) = read_unit(rest)?;
        rest = rest[unit_len..].trim_start();
        total = total.checked_add(number.checked_mul(unit_seconds)?)?;
    }
    Some(Duration::from_secs(total))
}

/// Reads the unit at the start of `text`: its length in seconds and its
/// length in bytes. No unit, as before a digit or at the end, is seconds.
fn read_unit(text: &str) -> Option<(u64, usize)> {
    let word_len = leading_len(text, |c| c.is_ascii_alphabetic());
    if word_len == 0 {
        return Some((1, 0));
    }
    let word = &text[..word_len];
    UNITS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, seconds)| (*seconds, word_len))
}

/// The length in bytes of the start of `text` whose characters all match.
fn leading_len(text: &str, matches: impl Fn(char) -> bool) -> usize {
    text.len() - text.trim_start_matches(matches).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Option<Span> {
        Some(Span::Time(Duration::from_secs(n)))
    }

    #[test]
    fn never() {
        assert_eq!(parse("Never"), Some(Span::Never));
        assert_eq!(parse(" Never "), Some(Span::Never));
        assert_eq!(parse("never"), None);
    }

    #[test]
    fn units() {
        assert_eq!(parse("2h"), secs(7200));
        assert_eq!(parse("2hr"), secs(7200));
        assert_eq!(parse("15m"), secs(900));
        assert_eq!(parse("15min"), secs(900));
        assert_eq!(parse("120s"), secs(120));
        assert_eq!(parse("120sec"), secs(120));
        assert_eq!(parse("90"), secs(90));
        assert_eq!(parse("0"), secs(0));
    }

    #[test]
    fn parts_add() {
        assert_eq!(parse("1m 120s"), secs(180));
        assert_eq!(parse("1m120s"), secs(180));
        assert_eq!(parse("1 h 20 min"), secs(4800));
        assert_eq!(parse("1m 30"), secs(90));
    }

    #[test]
    fn wrong() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("m"), None);
        assert_eq!(parse("5d"), None);
        assert_eq!(parse("1.5h"), None);
        assert_eq!(parse("-5m"), None);
        assert_eq!(parse("5 minutes"), None);
    }
}
