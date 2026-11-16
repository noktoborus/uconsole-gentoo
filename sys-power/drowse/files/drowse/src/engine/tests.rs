//! Tests of the engine: the world is given by hand, the time too.

use super::*;
use crate::settings;

const fn secs(n: u64) -> u64 {
    n * 1_000_000
}

const fn mins(n: u64) -> u64 {
    secs(n * 60)
}

fn session(id: &str, idle: bool, idle_since: u64) -> Session {
    Session {
        id: id.to_owned(),
        uid: 1000,
        user: "alice".to_owned(),
        class: "user".to_owned(),
        kind: "wayland".to_owned(),
        tty: "tty1".to_owned(),
        remote_host: String::new(),
        idle,
        idle_since,
        state: SessionState::Active,
    }
}

fn inhibitor(what: &str, mode: &str) -> Inhibitor {
    Inhibitor {
        what: what.to_owned(),
        who: "drowse-inhibit".to_owned(),
        why: "emerge".to_owned(),
        mode: mode.to_owned(),
        uid: 1000,
        pid: 42,
    }
}

fn idle_world(since: u64) -> Snapshot {
    Snapshot {
        sessions: vec![session("1", true, since)],
        inhibitors: Vec::new(),
    }
}

fn busy_world(since: u64) -> Snapshot {
    Snapshot {
        sessions: vec![session("1", false, since)],
        inhibitors: Vec::new(),
    }
}

/// The times of the tests: one stage a power source, `poweroff`.
const TIMES: &str = "[IdleBattery]\nIdleTime=15m\n[IdleAC]\nIdleTime=60m\n";

fn engine(text: &str, power: PowerSource) -> Engine {
    let (settings, warnings) = settings::parse(text);
    assert!(warnings.is_empty(), "{warnings:?}");
    Engine::new(settings, power, 0, Vec::new()).0
}

fn logind_actions(out: &[Command]) -> Vec<LogindAction> {
    out.iter()
        .filter_map(|command| match command {
            Command::Logind(action) => Some(*action),
            _ => None,
        })
        .collect()
}

fn jobs(out: &[Command]) -> Vec<Job> {
    out.iter()
        .filter_map(|command| match command {
            Command::Job(job) => Some(job.clone()),
            _ => None,
        })
        .collect()
}

fn has_line(out: &[Command], level: Level, part: &str) -> bool {
    out.iter().any(|command| {
        matches!(command, Command::Journal(l, text) if *l == level && text.contains(part))
    })
}

const STAGES: &str = "\
[IdleBattery]
IdleTime=0
Countdown=Never
Action=tuned-adm profile powersave
ActionHint=power save
Resume=tuned-adm auto_profile

[IdleBattery-1]
IdleTime=5m
Countdown=Never
Action=nmcli radio all off
ActionHint=radio off
Resume=nmcli radio all on

[IdleBattery-2]
IdleTime=30m
Countdown=2m
Action=poweroff

[IdleAC]
IdleTime=Never
";

const POWER_SAVE: &str = "drowsed-stage@battery-0.service";
const RADIO_OFF: &str = "drowsed-stage@battery-1.service";

#[test]
fn an_announce_then_power_off() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.view().scheduled, ("poweroff".to_owned(), mins(17)));
    assert_eq!(engine.next_due(), Some((mins(15), Due::Announce)));
    let out = engine.on_due(mins(15));
    assert!(has_line(&out, Level::Info, "announce: poweroff in 2 min"));
    assert_eq!(engine.view().state, State::Announced);
    assert_eq!(engine.next_due(), Some((mins(17), Due::Action)));
    let out = engine.on_due(mins(17));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.view().scheduled, (String::new(), 0));
    assert_eq!(engine.next_due(), None);
}

#[test]
fn the_count_starts_no_sooner_than_the_start() {
    let mut engine = Engine::new(
        settings::parse(TIMES).0,
        PowerSource::Battery,
        mins(100),
        Vec::new(),
    )
    .0;
    engine.on_snapshot(mins(100), idle_world(0));
    assert_eq!(engine.next_due(), Some((mins(115), Due::Announce)));
}

#[test]
fn activity_cancels_the_announce() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_due(mins(15));
    let out = engine.on_snapshot(mins(16), busy_world(mins(16)));
    assert!(has_line(
        &out,
        Level::Notice,
        "cancel: session 1 (alice) in use"
    ));
    let view = engine.view();
    assert_eq!(view.state, State::Active);
    assert_eq!(view.sessions.len(), 1);
    assert_eq!(view.scheduled, (String::new(), 0));
    engine.on_snapshot(mins(20), idle_world(mins(20)));
    assert_eq!(engine.next_due(), Some((mins(35), Due::Announce)));
}

#[test]
fn a_late_announce_runs_in_full() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_due(mins(20));
    assert!(logind_actions(&out).is_empty());
    assert_eq!(engine.next_due(), Some((mins(22), Due::Action)));
}

#[test]
fn an_action_waits_for_a_fresh_read() {
    let mut engine = engine("[IdleAC]\nIdleTime=1m\nCountdown=Never\n", PowerSource::Ac);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_power(mins(2), PowerSource::Unknown);
    assert!(logind_actions(&out).is_empty());
    let out = engine.on_due(mins(2));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
}

#[test]
fn any_inhibitor_holds_and_its_end_is_activity() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    let held = Snapshot {
        sessions: vec![session("1", true, 0)],
        inhibitors: vec![inhibitor("sleep", "block-weak")],
    };
    engine.on_snapshot(0, held);
    let view = engine.view();
    assert_eq!(view.state, State::Active);
    assert_eq!(view.inhibitors.len(), 1);
    let out = engine.on_snapshot(mins(40), idle_world(0));
    assert!(has_line(&out, Level::Info, "inhibitors let go"));
    assert_eq!(engine.next_due(), Some((mins(55), Due::Announce)));
}

#[test]
fn a_delay_inhibitor_does_not_hold() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    let snapshot = Snapshot {
        sessions: vec![session("1", true, 0)],
        inhibitors: vec![
            inhibitor("shutdown", "delay"),
            inhibitor("handle-lid-switch", "block"),
        ],
    };
    engine.on_snapshot(0, snapshot);
    assert_eq!(engine.view().state, State::Idle);
}

#[test]
fn closing_and_background_sessions_do_not_count() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    let mut closing = session("2", false, 0);
    closing.state = SessionState::Closing;
    let mut background = session("3", false, 0);
    background.class = "background".to_owned();
    let snapshot = Snapshot {
        sessions: vec![session("1", true, 0), closing, background],
        inhibitors: Vec::new(),
    };
    engine.on_snapshot(0, snapshot);
    assert_eq!(engine.view().state, State::Idle);
}

#[test]
fn with_no_settings_nothing_comes() {
    let mut engine = engine("", PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.next_due(), None);
}

#[test]
fn a_greeter_counts_unless_left_out() {
    let mut greeter = session("2", false, 0);
    greeter.class = "greeter".to_owned();
    let snapshot = Snapshot {
        sessions: vec![session("1", true, 0), greeter],
        inhibitors: Vec::new(),
    };
    let mut by_default = engine(TIMES, PowerSource::Battery);
    by_default.on_snapshot(0, snapshot.clone());
    assert_eq!(by_default.view().state, State::Active);
    let mut no_greeter = engine(
        "[Sessions]\nClasses=user\n[IdleBattery]\nIdleTime=15m\n",
        PowerSource::Battery,
    );
    no_greeter.on_snapshot(0, snapshot);
    assert_eq!(no_greeter.view().state, State::Idle);
}

#[test]
fn a_session_in_use_that_leaves_is_activity() {
    let mut engine = engine(TIMES, PowerSource::Battery);
    let two = Snapshot {
        sessions: vec![session("1", true, 0), session("2", false, 0)],
        inhibitors: Vec::new(),
    };
    engine.on_snapshot(0, two);
    assert_eq!(engine.view().state, State::Active);
    engine.on_snapshot(mins(30), idle_world(0));
    assert_eq!(engine.next_due(), Some((mins(45), Due::Announce)));
}

#[test]
fn no_session_is_idle() {
    let mut engine = engine(TIMES, PowerSource::Ac);
    engine.on_snapshot(0, Snapshot::default());
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.next_due(), Some((mins(60), Due::Announce)));
}

#[test]
fn never_is_idle_with_no_action() {
    let mut engine = engine(STAGES, PowerSource::Ac);
    let out = engine.on_snapshot(0, idle_world(0));
    assert!(has_line(&out, Level::Info, "no action"));
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.view().scheduled, (String::new(), 0));
    assert_eq!(engine.next_due(), None);
}

#[test]
fn the_stages_one_after_another() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    let out = engine.on_snapshot(0, idle_world(0));
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
    assert_eq!(engine.view().scheduled, ("radio off".to_owned(), mins(5)));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    let out = engine.on_due(mins(5));
    assert_eq!(jobs(&out), [Job::Start(RADIO_OFF.to_owned())]);
    assert_eq!(engine.view().scheduled, ("poweroff".to_owned(), mins(37)));
    engine.on_job_ended(mins(5) + secs(1), RADIO_OFF, true);
    engine.on_due(mins(35));
    assert_eq!(engine.view().state, State::Announced);
    let out = engine.on_due(mins(37));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
}

#[test]
fn activity_runs_the_resume_last_stage_first() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    engine.on_due(mins(5));
    engine.on_job_ended(mins(5) + secs(1), RADIO_OFF, true);
    let out = engine.on_snapshot(mins(10), busy_world(mins(10)));
    assert_eq!(
        jobs(&out),
        [
            Job::Stop(RADIO_OFF.to_owned()),
            Job::Stop(POWER_SAVE.to_owned())
        ]
    );
    assert!(!engine.settled());
    engine.on_job_ended(mins(10) + secs(1), RADIO_OFF, true);
    engine.on_job_ended(mins(10) + secs(1), POWER_SAVE, true);
    assert!(engine.settled());
}

#[test]
fn a_stop_waits_for_a_command_that_runs() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_snapshot(secs(4), busy_world(secs(4)));
    assert!(jobs(&out).is_empty());
    assert!(has_line(&out, Level::Warning, "still runs after 4 s"));
    let out = engine.on_job_ended(secs(5), POWER_SAVE, true);
    assert_eq!(jobs(&out), [Job::Stop(POWER_SAVE.to_owned())]);
    assert!(has_line(&out, Level::Warning, "runs late"));
}

#[test]
fn a_change_of_power_is_a_new_count() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    let out = engine.on_power(mins(3), PowerSource::Ac);
    assert!(has_line(&out, Level::Info, "power: AC; the times moved"));
    assert_eq!(jobs(&out), [Job::Stop(POWER_SAVE.to_owned())]);
    assert_eq!(engine.view().scheduled, (String::new(), 0));
    engine.on_job_ended(mins(3) + secs(1), POWER_SAVE, true);
    let out = engine.on_power(mins(4), PowerSource::Battery);
    assert!(jobs(&out).is_empty());
    assert_eq!(engine.next_due(), Some((mins(4), Due::Action)));
    let out = engine.on_due(mins(4));
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
    assert_eq!(engine.view().scheduled, ("radio off".to_owned(), mins(9)));
}

#[test]
fn unknown_power_is_ac_with_no_new_count() {
    let mut engine = engine(TIMES, PowerSource::Ac);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_power(mins(10), PowerSource::Unknown);
    assert!(!has_line(&out, Level::Info, "the times moved"));
    assert_eq!(engine.next_due(), Some((mins(60), Due::Announce)));
}

#[test]
fn the_stop_of_the_service_runs_the_resume() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    let out = engine.stop(mins(1));
    assert_eq!(jobs(&out), [Job::Stop(POWER_SAVE.to_owned())]);
    assert!(!engine.settled());
    engine.on_job_ended(mins(1) + secs(1), POWER_SAVE, true);
    assert!(engine.settled());
}

#[test]
fn units_left_from_before_are_stopped_at_the_start() {
    let (engine, out) = Engine::new(
        settings::Settings::default(),
        PowerSource::Ac,
        0,
        vec![RADIO_OFF.to_owned()],
    );
    assert_eq!(jobs(&out), [Job::Stop(RADIO_OFF.to_owned())]);
    assert!(!engine.settled());
}

#[test]
fn a_failed_command_is_a_warning_and_the_stages_go_on() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_job_ended(secs(5), POWER_SAVE, false);
    assert!(has_line(&out, Level::Warning, "no Resume"));
    assert_eq!(engine.view().scheduled, ("radio off".to_owned(), mins(5)));
}

#[test]
fn upower_gone_and_back() {
    let (_, out) = Engine::new(
        settings::Settings::default(),
        PowerSource::Unknown,
        0,
        Vec::new(),
    );
    assert!(has_line(&out, Level::Warning, "upower: no answer"));
    let mut engine = engine(TIMES, PowerSource::Ac);
    let out = engine.on_power(0, PowerSource::Unknown);
    assert!(has_line(&out, Level::Warning, "upower: no answer"));
    let out = engine.on_power(1, PowerSource::Ac);
    assert!(has_line(&out, Level::Info, "upower: back"));
}
