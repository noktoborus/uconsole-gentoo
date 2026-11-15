//! drowse-status: what the tool shows, worked out from what it read. This
//! part is pure; `client` reads the bus.

pub mod client;

use std::time::Duration;

use rust_i18n::t;

use crate::bus::SessionRow;
use crate::inhibit::WHO;
use crate::world::Inhibitor;

/// The longest `why` shown, in characters.
const WHY_MAX: usize = 40;

/// Below this time left, the seconds are shown and counted.
const SECONDS_BELOW: Duration = Duration::from_secs(3 * 60);

/// The state of the service as read on the bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceState {
    /// `active`, `idle` or `announced`.
    pub state: String,
    /// The action of the next stage and when it comes.
    pub scheduled: (String, u64),
    /// `battery`, `ac` or `unknown`.
    pub power: String,
}

/// What the tool read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// The state of the service; none when it is not on the bus.
    pub service: Option<ServiceState>,
    /// The sessions that hold the system up; none when the bus gave no
    /// answer (a user not in `wheel`).
    pub sessions: Option<Vec<SessionRow>>,
    /// The inhibitors of logind that hold the system up.
    pub inhibitors: Vec<Inhibitor>,
    /// The uid of the user of the tool.
    pub uid: u32,
}

/// One line for waybar, and the time of the next line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The time left, or empty.
    pub text: String,
    /// What matters most now.
    pub alt: &'static str,
    /// The state, the hold, the power.
    pub class: [&'static str; 3],
    /// The lines of the tooltip.
    pub tooltip: Vec<String>,
    /// When the line must be written again though nothing changed.
    pub again_in: Option<Duration>,
}

impl Observed {
    /// Whether an inhibitor of the tool of this user holds.
    fn own_held(&self) -> bool {
        self.inhibitors
            .iter()
            .any(|inhibitor| inhibitor.who == WHO && inhibitor.uid == self.uid)
    }

    /// Whether an inhibitor of another holds.
    fn other_held(&self) -> bool {
        self.inhibitors
            .iter()
            .any(|inhibitor| inhibitor.who != WHO || inhibitor.uid != self.uid)
    }

    /// The hold: `own`, `other`, `session-other`, `session-own`, `none`,
    /// `hidden`.
    fn hold(&self) -> &'static str {
        if self.own_held() {
            return "own";
        }
        if self.other_held() {
            return "other";
        }
        match &self.sessions {
            None => "hidden",
            Some(sessions) if sessions.iter().any(|row| row.1 != self.uid) => "session-other",
            Some(sessions) if !sessions.is_empty() => "session-own",
            Some(_) => "none",
        }
    }
}

/// Works out what to show at the time `now` of `CLOCK_MONOTONIC`, in
/// microseconds, with `user_of` giving the name of a uid.
pub fn show(observed: &Observed, now: u64, user_of: &dyn Fn(u32) -> String) -> Shown {
    let Some(service) = &observed.service else {
        return Shown {
            text: String::new(),
            alt: "unknown",
            class: ["unknown", observed.hold(), "ac"],
            tooltip: vec![t!("status.not_running").into_owned()],
            again_in: None,
        };
    };
    let state = match service.state.as_str() {
        "idle" => "idle",
        "announced" => "announced",
        _ => "active",
    };
    let hold = observed.hold();
    let alt = match (state, hold) {
        ("announced", _) => "announced",
        (_, "own") => "own",
        (_, "other") => "other",
        (_, "session-other") => "session-other",
        ("idle", _) => "idle",
        _ => "active",
    };
    let power = if service.power == "battery" {
        "battery"
    } else {
        "ac"
    };
    let (action, at) = &service.scheduled;
    let left =
        (state != "active" && *at > 0).then(|| Duration::from_micros(at.saturating_sub(now)));
    let text = left.map(time_left).unwrap_or_default();
    let mut tooltip = match (state, left) {
        ("announced", Some(left)) => {
            vec![t!("status.announced", action = action, left = time_left(left)).into_owned()]
        }
        ("idle", Some(left)) => {
            vec![t!("status.planned", action = action, left = time_left(left)).into_owned()]
        }
        ("idle", None) | ("announced", None) => vec![t!("status.idle").into_owned()],
        _ => holders(observed, user_of),
    };
    tooltip.push(power_line(&service.power));
    Shown {
        text,
        alt,
        class: [state, hold, power],
        tooltip,
        again_in: left.map(again_in),
    }
}

/// The lines of what holds the system up.
fn holders(observed: &Observed, user_of: &dyn Fn(u32) -> String) -> Vec<String> {
    let mut items: Vec<String> = observed
        .sessions
        .iter()
        .flatten()
        .map(session_line)
        .collect();
    items.extend(
        observed
            .inhibitors
            .iter()
            .map(|inhibitor| inhibitor_line(inhibitor, user_of)),
    );
    if items.is_empty() {
        return vec![t!("status.awake").into_owned()];
    }
    let mut lines = vec![t!("status.awake_held").into_owned()];
    lines.extend(items.into_iter().map(|item| format!(" - {item}")));
    lines
}

/// A session: the user, then where.
fn session_line(row: &SessionRow) -> String {
    let (_, _, user, kind, tty, host) = row;
    if !host.is_empty() {
        return t!("status.session_remote", user = user, host = host).into_owned();
    }
    if !tty.is_empty() {
        return t!("status.session_tty", user = user, tty = tty, kind = kind).into_owned();
    }
    t!("status.session_other", user = user, kind = kind).into_owned()
}

/// An inhibitor: `why` by the user; with no `why`, `who`.
fn inhibitor_line(inhibitor: &Inhibitor, user_of: &dyn Fn(u32) -> String) -> String {
    let why = if inhibitor.why.is_empty() {
        &inhibitor.who
    } else {
        &inhibitor.why
    };
    t!(
        "status.inhibitor",
        why = cut(why),
        user = user_of(inhibitor.uid)
    )
    .into_owned()
}

/// A text cut to `WHY_MAX` characters, with `…`.
fn cut(text: &str) -> String {
    if text.chars().count() <= WHY_MAX {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(WHY_MAX - 1).collect();
    short.push('…');
    short
}

/// The line of the power source.
fn power_line(power: &str) -> String {
    match power {
        "battery" => t!("status.on_battery").into_owned(),
        "ac" => t!("status.on_ac").into_owned(),
        _ => t!("status.power_unknown").into_owned(),
    }
}

/// The time left: `1 h 20 min`, `14 min` (rounded up), `2:47`.
pub fn time_left(left: Duration) -> String {
    let seconds = left.as_secs();
    if left < SECONDS_BELOW {
        return format!("{}:{:02}", seconds / 60, seconds % 60);
    }
    let minutes = seconds.div_ceil(60);
    if minutes < 60 {
        return t!("time.minutes", minutes = minutes).into_owned();
    }
    t!(
        "time.hours_minutes",
        hours = minutes / 60,
        minutes = minutes % 60
    )
    .into_owned()
}

/// When the time left must be written again: each second below 3 min,
/// else at the next change of the minute shown.
fn again_in(left: Duration) -> Duration {
    if left < SECONDS_BELOW {
        return Duration::from_secs(1);
    }
    let into_minute = left.as_secs() % 60;
    Duration::from_secs(if into_minute == 0 { 60 } else { into_minute })
}

/// A text for waybar: markup of Pango made plain, then a string of JSON.
fn json_text(text: &str) -> String {
    let plain = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    json_string(&plain)
}

/// A string of JSON, its quotes and control characters escaped.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl Shown {
    /// The line of JSON for waybar.
    pub fn waybar(&self) -> String {
        let class: Vec<String> = self.class.iter().map(|name| json_string(name)).collect();
        format!(
            "{{\"text\":{},\"alt\":{},\"class\":[{}],\"tooltip\":{}}}",
            json_text(&self.text),
            json_string(self.alt),
            class.join(","),
            json_text(&self.tooltip.join("\n"))
        )
    }

    /// The text for a terminal.
    pub fn plain(&self) -> String {
        self.tooltip.join("\n")
    }
}

#[cfg(test)]
mod tests;
