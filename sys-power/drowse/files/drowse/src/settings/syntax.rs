//! The form of the files of systemd: sections, `Key=value`, comments and
//! lines that go on after a backslash (`systemd.syntax(7)`).

use super::warning::Warning;

/// One `Key=value` of the file, with its section and its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name of the section, as written between the brackets.
    pub section: String,
    /// The key.
    pub key: String,
    /// The value, with the spaces at its ends dropped.
    pub value: String,
    /// The number of the line where the entry starts, from 1.
    pub line: usize,
}

/// A line of the file that holds a whole entry, a section or nothing.
struct Logical {
    text: String,
    line: usize,
}

/// Reads the text of a settings file: its entries in order, and a warning
/// for each line that is no section, no entry and no comment.
pub fn parse(text: &str) -> (Vec<Entry>, Vec<Warning>) {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let mut section: Option<String> = None;
    for Logical { text, line } in logical_lines(text) {
        if let Some(name) = section_name(&text) {
            section = Some(name.to_owned());
        } else if let Some((key, value)) = text.split_once('=') {
            match &section {
                Some(name) => entries.push(Entry {
                    section: name.clone(),
                    key: key.trim().to_owned(),
                    value: value.trim().to_owned(),
                    line,
                }),
                None => warnings.push(Warning::KeyOutsideSection { line }),
            }
        } else {
            warnings.push(Warning::NotAnEntry { line });
        }
    }
    (entries, warnings)
}

/// Joins the lines that end with a backslash to the next one, the
/// backslash made a space, and leaves out the empty lines and the comments.
fn logical_lines(text: &str) -> Vec<Logical> {
    let mut out = Vec::new();
    let mut open: Option<Logical> = None;
    for (index, raw) in text.lines().enumerate() {
        let trimmed = raw.trim();
        if is_comment(trimmed) {
            continue;
        }
        let (body, goes_on) = match trimmed.strip_suffix('\\') {
            Some(body) => (body, true),
            None => (trimmed, false),
        };
        let mut current = open.take().unwrap_or(Logical {
            text: String::new(),
            line: index + 1,
        });
        if !current.text.is_empty() {
            current.text.push(' ');
        }
        current.text.push_str(body.trim_end());
        if goes_on {
            open = Some(current);
        } else if !current.text.trim().is_empty() {
            out.push(current);
        }
    }
    if let Some(last) = open.filter(|last| !last.text.trim().is_empty()) {
        out.push(last);
    }
    out
}

/// Whether a trimmed line is empty or a comment (`#`, `;`).
fn is_comment(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';')
}

/// The name of a section line `[Name]`, if the line is one.
fn section_name(text: &str) -> Option<&str> {
    text.trim()
        .strip_prefix('[')?
        .strip_suffix(']')
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(section: &str, key: &str, value: &str, line: usize) -> Entry {
        Entry {
            section: section.to_owned(),
            key: key.to_owned(),
            value: value.to_owned(),
            line,
        }
    }

    #[test]
    fn sections_and_keys() {
        let text = "# a comment\n\n[IdleBattery]\nIdleTime = 15m\n; another\nAction=poweroff\n[IdleAC]\nIdleTime=Never\n";
        let (entries, warnings) = parse(text);
        assert_eq!(
            entries,
            [
                entry("IdleBattery", "IdleTime", "15m", 4),
                entry("IdleBattery", "Action", "poweroff", 6),
                entry("IdleAC", "IdleTime", "Never", 8),
            ]
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_line_goes_on() {
        let text = "[IdleBattery]\nAction=foo \\\n  --bar \\\n  baz\nIdleTime=1m\n";
        let (entries, _) = parse(text);
        assert_eq!(
            entries,
            [
                entry("IdleBattery", "Action", "foo --bar baz", 2),
                entry("IdleBattery", "IdleTime", "1m", 5),
            ]
        );
    }

    #[test]
    fn a_comment_inside_goes_on() {
        let text = "[IdleBattery]\nAction=foo \\\n# not here\n  bar\n";
        let (entries, _) = parse(text);
        assert_eq!(entries, [entry("IdleBattery", "Action", "foo bar", 2)]);
    }

    #[test]
    fn value_with_equals() {
        let (entries, _) = parse("[IdleAC]\nAction=env A=1 foo\n");
        assert_eq!(entries, [entry("IdleAC", "Action", "env A=1 foo", 2)]);
    }

    #[test]
    fn wrong_lines() {
        let (entries, warnings) = parse("IdleTime=1m\n[IdleAC]\njust words\n");
        assert!(entries.is_empty());
        assert_eq!(
            warnings,
            [
                Warning::KeyOutsideSection { line: 1 },
                Warning::NotAnEntry { line: 3 },
            ]
        );
    }
}
