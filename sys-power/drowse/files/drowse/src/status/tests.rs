//! Tests of what drowse-status shows.

use super::*;

const MINUTE: u64 = 60_000_000;

fn service(state: &str, at: u64) -> ServiceState {
    ServiceState {
        state: state.to_owned(),
        scheduled: (if at > 0 { "poweroff" } else { "" }.to_owned(), at),
        power: "battery".to_owned(),
    }
}

fn inhibitor(who: &str, why: &str, uid: u32) -> Inhibitor {
    Inhibitor {
        what: "sleep:shutdown".to_owned(),
        who: who.to_owned(),
        why: why.to_owned(),
        mode: "block-weak".to_owned(),
        uid,
        pid: 7,
    }
}

fn session(user: &str, uid: u32, tty: &str, host: &str) -> SessionRow {
    (
        "1".to_owned(),
        uid,
        user.to_owned(),
        "wayland".to_owned(),
        tty.to_owned(),
        host.to_owned(),
    )
}

fn observed(state: Option<ServiceState>) -> Observed {
    Observed {
        service: state,
        sessions: Some(Vec::new()),
        inhibitors: Vec::new(),
        uid: 1000,
    }
}

fn name(uid: u32) -> String {
    match uid {
        1000 => "alice".to_owned(),
        1001 => "bob".to_owned(),
        _ => uid.to_string(),
    }
}

/// The language is global: the tests that set it take this lock.
static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn language(code: &str) -> std::sync::MutexGuard<'static, ()> {
    let guard = LANGUAGE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    rust_i18n::set_locale(code);
    guard
}

fn english() -> std::sync::MutexGuard<'static, ()> {
    language("en")
}

#[test]
fn times() {
    let _language = english();
    assert_eq!(time_left(Duration::from_secs(80 * 60)), "1 h 20 min");
    assert_eq!(time_left(Duration::from_secs(13 * 60 + 1)), "14 min");
    assert_eq!(time_left(Duration::from_secs(14 * 60)), "14 min");
    assert_eq!(time_left(Duration::from_secs(167)), "2:47");
    assert_eq!(time_left(Duration::from_secs(5)), "0:05");
}

#[test]
fn not_running() {
    let _language = english();
    let shown = show(&observed(None), 0, &name);
    assert_eq!(shown.alt, "unknown");
    assert_eq!(shown.class, ["unknown", "none", "ac"]);
    assert_eq!(shown.tooltip, ["drowsed is not running"]);
}

#[test]
fn planned() {
    let _language = english();
    let shown = show(
        &observed(Some(service("idle", 17 * MINUTE))),
        3 * MINUTE,
        &name,
    );
    assert_eq!(shown.text, "14 min");
    assert_eq!(shown.alt, "idle");
    assert_eq!(shown.class, ["idle", "none", "battery"]);
    assert_eq!(shown.tooltip, ["planned poweroff in 14 min", "On battery"]);
    assert_eq!(shown.again_in, Some(Duration::from_secs(60)));
}

#[test]
fn announced_counts_the_seconds() {
    let _language = english();
    let shown = show(
        &observed(Some(service("announced", 17 * MINUTE))),
        15 * MINUTE + 13_000_000,
        &name,
    );
    assert_eq!(shown.text, "1:47");
    assert_eq!(shown.alt, "announced");
    assert_eq!(shown.tooltip[0], "poweroff in 1:47: no activity");
    assert_eq!(shown.again_in, Some(Duration::from_secs(1)));
}

#[test]
fn idle_with_no_action() {
    let _language = english();
    let shown = show(&observed(Some(service("idle", 0))), 0, &name);
    assert_eq!(shown.text, "");
    assert_eq!(shown.tooltip, ["Idle", "On battery"]);
    assert_eq!(shown.again_in, None);
}

#[test]
fn awake_held_by_many() {
    let _language = english();
    let mut seen = observed(Some(service("active", 0)));
    seen.sessions = Some(vec![
        session("alice", 1000, "tty1", ""),
        session("bob", 1001, "", "192.168.1.5"),
    ]);
    seen.inhibitors = vec![inhibitor("systemd-inhibit", "backup", 1001)];
    let shown = show(&seen, 0, &name);
    assert_eq!(shown.alt, "other");
    assert_eq!(shown.class, ["active", "other", "battery"]);
    assert_eq!(
        shown.tooltip,
        [
            "awake, held by:",
            " - alice on tty1 (wayland)",
            " - bob by ssh from 192.168.1.5",
            " - backup by bob",
            "On battery"
        ]
    );
}

#[test]
fn the_holds() {
    let _language = english();
    let mut seen = observed(Some(service("active", 0)));
    seen.inhibitors = vec![inhibitor(WHO, "emerge", 1000), inhibitor("x", "y", 1001)];
    assert_eq!(show(&seen, 0, &name).alt, "own");
    seen.inhibitors.clear();
    seen.sessions = Some(vec![session("bob", 1001, "tty2", "")]);
    let shown = show(&seen, 0, &name);
    assert_eq!(
        (shown.alt, shown.class[1]),
        ("session-other", "session-other")
    );
    seen.sessions = Some(vec![session("alice", 1000, "tty1", "")]);
    let shown = show(&seen, 0, &name);
    assert_eq!((shown.alt, shown.class[1]), ("active", "session-own"));
    seen.sessions = None;
    let shown = show(&seen, 0, &name);
    assert_eq!((shown.alt, shown.class[1]), ("active", "hidden"));
    assert_eq!(shown.tooltip, ["awake", "On battery"]);
}

#[test]
fn a_long_why_is_cut_and_no_why_is_who() {
    let _language = english();
    let long = "a".repeat(50);
    assert_eq!(
        inhibitor_line(&inhibitor("x", &long, 1000), &name),
        format!("{}… by alice", "a".repeat(39))
    );
    assert_eq!(
        inhibitor_line(&inhibitor("drowse-inhibit", "", 1000), &name),
        "drowse-inhibit by alice"
    );
}

#[test]
fn json_for_waybar() {
    let _language = english();
    let mut seen = observed(Some(service("active", 0)));
    seen.inhibitors = vec![inhibitor("x", "<b>\"a\" & b</b>", 1001)];
    let line = show(&seen, 0, &name).waybar();
    assert_eq!(
        line,
        "{\"text\":\"\",\"alt\":\"other\",\"class\":[\"active\",\"other\",\"battery\"],\"tooltip\":\"awake, held by:\\n - &lt;b&gt;\\\"a\\\" &amp; b&lt;/b&gt; by bob\\nOn battery\"}"
    );
}

#[test]
fn russian() {
    let _language = language("ru");
    let shown = show(
        &observed(Some(service("idle", 17 * MINUTE))),
        3 * MINUTE,
        &name,
    );
    assert_eq!(shown.text, "14 мин");
    assert_eq!(shown.tooltip[1], "От батареи");
}
