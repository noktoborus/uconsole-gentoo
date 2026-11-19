//! Tests of the engine: the world is given by hand, the time too.

mod chain;

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
const TIMES: &str = "\
main:
  battery: [{after: 15m, action: poweroff}]
  ac: [{after: 60m, action: poweroff}]
";

fn engine(text: &str, power: PowerSource) -> Engine {
    let settings = settings::parse(text).unwrap();
    Engine::new(settings, power, false, 0, Vec::new()).0
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
main:
  battery:
    - after: 0
      enter: tuned-adm profile powersave
      leave: tuned-adm auto_profile
      hint: power save
    - after: 5m
      enter: nmcli radio all off
      leave: nmcli radio all on
      hint: radio off
    - after: 30m
      countdown: 2m
      action: poweroff
  ac: [{}]
";

const POWER_SAVE: &str = "drowsed-stage@main.battery.1.service";
const RADIO_OFF: &str = "drowsed-stage@main.battery.2.service";

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
        settings::parse(TIMES).unwrap(),
        PowerSource::Battery,
        false,
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
    let mut engine = engine(
        "main:\n  ac: [{after: 1m, countdown: never, action: poweroff}]\n",
        PowerSource::Ac,
    );
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_power(mins(2), PowerSource::Unknown);
    assert!(logind_actions(&out).is_empty());
    let out = engine.on_due(mins(2));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
}

#[test]
fn inhibitors_do_not_count_as_activity_or_hold_the_wrong_section() {
    let text = "main:\n  battery: [{after: 1m, countdown: never, enter: foo}, {after: 1m, countdown: never, action: suspend}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    let held = Snapshot {
        sessions: vec![session("1", true, 0)],
        inhibitors: vec![inhibitor("sleep", "block-weak")],
    };
    engine.on_snapshot(0, held);
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.view().inhibitors.len(), 0);
    let out = engine.on_due(mins(1));
    assert!(jobs(&out).contains(&Job::Start(POWER_SAVE.to_owned())));
    assert_eq!(engine.view().state, State::Idle);
}

#[test]
fn an_idle_inhibitor_holds_a_command_and_release_enters_it() {
    let text = "main:\n  battery: [{after: 1m, countdown: never, enter: foo}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    engine.on_snapshot(
        0,
        Snapshot {
            sessions: vec![session("1", true, 0)],
            inhibitors: vec![inhibitor("idle", "block-weak")],
        },
    );
    let out = engine.on_due(mins(1));
    assert!(has_line(
        &out,
        Level::Info,
        "held: main.battery.1 (foo) by drowse-inhibit"
    ));
    assert!(jobs(&out).is_empty());
    assert_eq!(engine.view().state, State::Held);
    assert_eq!(engine.view().scheduled.1, 0);
    assert_eq!(engine.view().inhibitors.len(), 1);
    assert_eq!(engine.next_due(), None);
    let out = engine.on_snapshot(mins(2), idle_world(0));
    assert!(has_line(
        &out,
        Level::Info,
        "released: main.battery.1 (foo) now"
    ));
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
}

#[test]
fn shutdown_and_sleep_hold_only_their_logind_actions() {
    for (kind, action, held) in [
        ("shutdown", "poweroff", true),
        ("shutdown", "suspend", false),
        ("sleep", "poweroff", false),
        ("sleep", "suspend", true),
    ] {
        let text =
            format!("main:\n  battery: [{{after: 1m, countdown: never, action: {action}}}]\n");
        let mut engine = engine(&text, PowerSource::Battery);
        engine.on_snapshot(
            0,
            Snapshot {
                sessions: vec![session("1", true, 0)],
                inhibitors: vec![inhibitor(kind, "block")],
            },
        );
        let out = engine.on_due(mins(1));
        assert_eq!(!logind_actions(&out).is_empty(), !held, "{kind} / {action}");
        assert_eq!(
            has_line(&out, Level::Info, "held:"),
            held,
            "{kind} / {action}"
        );
    }
}

#[test]
fn an_inhibitor_during_announce_does_not_cancel_it() {
    let text = "main:\n  battery: [{after: 1m, countdown: 1m, enter: foo}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_due(mins(1));
    let held = Snapshot {
        sessions: vec![session("1", true, 0)],
        inhibitors: vec![inhibitor("idle", "block-weak")],
    };
    let out = engine.on_snapshot(mins(1) + secs(1), held);
    assert!(out.is_empty());
    assert_eq!(engine.view().state, State::Announced);
    let out = engine.on_due(mins(2));
    assert!(has_line(&out, Level::Info, "held: main.battery.1 (foo) by"));
    assert_eq!(engine.view().state, State::Announced);
    let out = engine.on_snapshot(mins(3), idle_world(0));
    assert!(has_line(
        &out,
        Level::Info,
        "released: main.battery.1 (foo) now"
    ));
    assert!(jobs(&out).contains(&Job::Start(POWER_SAVE.to_owned())));
}

#[test]
fn a_logind_inhibition_waits_for_release() {
    let text = "main:\n  battery: [{after: 1m, countdown: never, action: poweroff}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_due(mins(1));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
    let out = engine.on_action_blocked(mins(1), vec![inhibitor("shutdown", "block-weak")]);
    assert!(has_line(
        &out,
        Level::Info,
        "held: main.battery.1 (poweroff) by"
    ));
    assert_eq!(engine.next_due(), None);
    let out = engine.on_snapshot(mins(2), idle_world(0));
    assert!(has_line(
        &out,
        Level::Info,
        "released: main.battery.1 (poweroff) now"
    ));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
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
        "sessions:\n  classes: [user]\nmain:\n  battery: [{after: 15m}]\n",
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
    assert!(has_line(&out, Level::Info, "idle, on AC"));
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.view().scheduled, (String::new(), 0));
    assert_eq!(engine.next_due(), None);
}

/// Ends the `leave` of a unit at `at`, and gives the entry that waited
/// for it, on a fresh read of an idle world.
fn resume_ended(engine: &mut Engine, at: u64, unit: &str) -> Vec<Command> {
    engine.on_job_ended(at, unit, true);
    assert_eq!(engine.next_due(), Some((at, Due::Action)));
    engine.on_snapshot(at, idle_world(0))
}

#[test]
fn the_stages_one_after_another() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    let out = engine.on_snapshot(0, idle_world(0));
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
    assert_eq!(engine.view().scheduled, ("radio off".to_owned(), mins(5)));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    let out = engine.on_due(mins(5));
    assert_eq!(jobs(&out), [Job::Stop(POWER_SAVE.to_owned())]);
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.next_due(), None);
    let out = resume_ended(&mut engine, mins(5) + secs(2), POWER_SAVE);
    assert_eq!(jobs(&out), [Job::Start(RADIO_OFF.to_owned())]);
    let entered = mins(5) + secs(2);
    assert_eq!(
        engine.view().scheduled,
        ("poweroff".to_owned(), entered + mins(32))
    );
    engine.on_job_ended(entered + secs(1), RADIO_OFF, true);
    engine.on_due(entered + mins(30));
    assert_eq!(engine.view().state, State::Announced);
    let out = engine.on_due(entered + mins(32));
    assert_eq!(jobs(&out), [Job::Stop(RADIO_OFF.to_owned())]);
    assert!(logind_actions(&out).is_empty());
    let out = resume_ended(&mut engine, entered + mins(32) + secs(1), RADIO_OFF);
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
}

#[test]
fn activity_leaves_the_one_stage_entered() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    engine.on_due(mins(5));
    resume_ended(&mut engine, mins(5) + secs(1), POWER_SAVE);
    engine.on_job_ended(mins(5) + secs(2), RADIO_OFF, true);
    let out = engine.on_snapshot(mins(10), busy_world(mins(10)));
    assert_eq!(jobs(&out), [Job::Stop(RADIO_OFF.to_owned())]);
    assert!(!engine.settled());
    engine.on_job_ended(mins(10) + secs(1), RADIO_OFF, true);
    assert!(engine.settled());
}

#[test]
fn activity_while_the_resume_runs_cancels_the_entry() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    engine.on_due(mins(5));
    let out = engine.on_snapshot(mins(5) + secs(1), busy_world(mins(5) + secs(1)));
    assert!(jobs(&out).is_empty());
    engine.on_job_ended(mins(5) + secs(2), POWER_SAVE, true);
    assert_eq!(engine.view().state, State::Active);
    assert_eq!(engine.next_due(), None);
    assert!(engine.settled());
}

#[test]
fn activity_that_ended_during_the_resume_counts_again() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    engine.on_due(mins(5));
    let back = mins(5) + secs(1);
    engine.on_job_ended(back, POWER_SAVE, true);
    let out = engine.on_snapshot(back, idle_world(back));
    assert!(!jobs(&out).contains(&Job::Start(RADIO_OFF.to_owned())));
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
}

#[test]
fn a_failed_resume_is_a_warning_and_the_entry_comes() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    engine.on_due(mins(5));
    let out = engine.on_job_ended(mins(5) + secs(5), POWER_SAVE, false);
    assert!(has_line(&out, Level::Warning, "the leave failed"));
    let out = engine.on_snapshot(mins(5) + secs(5), idle_world(0));
    assert_eq!(jobs(&out), [Job::Start(RADIO_OFF.to_owned())]);
}

#[test]
fn a_new_count_waits_for_the_resume() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_job_ended(secs(1), POWER_SAVE, true);
    let out = engine.on_snapshot(mins(1), busy_world(mins(1)));
    assert_eq!(jobs(&out), [Job::Stop(POWER_SAVE.to_owned())]);
    let out = engine.on_snapshot(mins(1) + secs(1), idle_world(mins(1) + secs(1)));
    assert!(jobs(&out).is_empty());
    let out = resume_ended(&mut engine, mins(1) + secs(2), POWER_SAVE);
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
}

#[test]
fn a_stage_with_no_action_runs_its_resume_when_left() {
    let text = "main:\n  battery: [{after: 0, leave: foo}]\n  ac: [{}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    let out = engine.on_snapshot(0, idle_world(0));
    assert_eq!(jobs(&out), [Job::Start(POWER_SAVE.to_owned())]);
    engine.on_job_ended(1, POWER_SAVE, true);
    let out = engine.on_snapshot(secs(1), busy_world(secs(1)));
    assert_eq!(jobs(&out), [Job::Stop(POWER_SAVE.to_owned())]);
}

#[test]
fn a_stage_with_nothing_is_an_entry_that_does_nothing() {
    let text = "main:\n  battery: [{after: 0}, {after: 1m, countdown: never, action: poweroff}]\n  ac: [{}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    let out = engine.on_snapshot(0, idle_world(0));
    assert!(jobs(&out).is_empty());
    assert!(has_line(&out, Level::Info, "entered, no action"));
    let out = engine.on_due(mins(1));
    assert_eq!(logind_actions(&out), [LogindAction::Poweroff]);
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
        false,
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
    assert!(has_line(&out, Level::Warning, "no leave"));
    assert_eq!(engine.view().scheduled, ("radio off".to_owned(), mins(5)));
}

#[test]
fn upower_gone_and_back() {
    let (_, out) = Engine::new(
        settings::Settings::default(),
        PowerSource::Unknown,
        false,
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

#[test]
fn the_holds_of_the_next_section_are_shown_while_the_time_runs() {
    let text = "main:\n  battery: [{after: 1m, countdown: never, enter: foo}, {after: 1m, countdown: never, action: poweroff}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    engine.on_snapshot(
        0,
        Snapshot {
            sessions: vec![session("1", true, 0)],
            inhibitors: vec![
                inhibitor("idle", "block-weak"),
                inhibitor("sleep:shutdown", "block-weak"),
            ],
        },
    );
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(
        engine.view().inhibitors,
        [inhibitor("idle", "block-weak")],
        "only the hold of the kind of the next section"
    );
    engine.on_snapshot(1, idle_world(0));
    assert_eq!(engine.view().state, State::Idle);
    assert!(engine.view().inhibitors.is_empty());
}

#[test]
fn no_hold_is_shown_while_the_system_is_in_use() {
    let text = "main:\n  battery: [{after: 1m, countdown: never, enter: foo}]\n";
    let mut engine = engine(text, PowerSource::Battery);
    engine.on_snapshot(
        0,
        Snapshot {
            sessions: vec![session("1", false, 0)],
            inhibitors: vec![inhibitor("idle", "block-weak")],
        },
    );
    assert_eq!(engine.view().state, State::Active);
    assert!(engine.view().inhibitors.is_empty());
}
