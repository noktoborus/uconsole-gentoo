//! The chain in use changes: by the power source or by the lid. A first
//! section with the same commands as the section entered takes its place
//! with no command run; any other change is activity.

use super::*;

/// The four chains: the same commands in `main.battery.1`,
/// `main.ac.1` and `lid.battery.1`; others in `lid.ac.1`.
const CHAINS: &str = "\
main:
  battery:
    - {after: 1m, enter: save, leave: unsave, hint: power save}
    - {after: 5m, countdown: never, action: poweroff}
  ac:
    - {after: 2m, enter: save, leave: unsave, hint: save on AC}
lid:
  battery:
    - {after: 0, enter: save, leave: unsave, hint: lid save}
    - {after: 10m, countdown: never, action: poweroff}
  ac:
    - {after: 0, enter: dim, leave: undim}
";

/// The unit of `main.battery.1`.
const SAVE: &str = "drowsed-stage@main.battery.1.service";

/// The engine on battery with `main.battery.1` entered at 1 min.
fn entered_on_battery() -> Engine {
    let mut engine = engine(CHAINS, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_due(mins(1));
    assert_eq!(jobs(&out), [Job::Start(SAVE.to_owned())]);
    engine.on_job_ended(mins(1), SAVE, true);
    engine
}

#[test]
fn a_change_of_power_carries_the_same_section_over() {
    let mut engine = entered_on_battery();
    let out = engine.on_power(mins(2), PowerSource::Ac);
    assert!(jobs(&out).is_empty(), "{out:?}");
    assert!(has_line(
        &out,
        Level::Info,
        "power: AC; section main.battery.1 (power save) -> main.ac.1 (save on AC): \
         the same commands, none run; drowsed-stage@main.battery.1.service runs on"
    ));
    assert_eq!(engine.view().state, State::Idle);
    assert_eq!(engine.view().scheduled, (String::new(), 0));
    let out = engine.on_snapshot(mins(3), busy_world(mins(3)));
    assert_eq!(jobs(&out), [Job::Stop(SAVE.to_owned())]);
}

#[test]
fn the_lid_carries_the_same_section_over_and_counts_the_next_from_then() {
    let mut engine = entered_on_battery();
    let out = engine.on_lid(mins(2), true);
    assert!(jobs(&out).is_empty(), "{out:?}");
    assert!(has_line(
        &out,
        Level::Info,
        "lid: closed; section main.battery.1 (power save) -> lid.battery.1 (lid save)"
    ));
    assert!(engine.view().lid_closed);
    assert_eq!(engine.view().scheduled, ("poweroff".to_owned(), mins(12)));
    let out = engine.on_lid(mins(3), false);
    assert!(jobs(&out).is_empty(), "{out:?}");
    assert!(has_line(
        &out,
        Level::Info,
        "lid: open; section lid.battery.1 (lid save) -> main.battery.1 (power save)"
    ));
    assert_eq!(engine.view().scheduled, ("poweroff".to_owned(), mins(8)));
}

#[test]
fn other_commands_are_activity() {
    let main_ac = "drowsed-stage@main.ac.1.service";
    let lid_ac = "drowsed-stage@lid.ac.1.service";
    let mut engine = engine(CHAINS, PowerSource::Ac);
    engine.on_snapshot(0, idle_world(0));
    engine.on_due(mins(2));
    engine.on_job_ended(mins(2), main_ac, true);
    let out = engine.on_lid(mins(3), true);
    assert!(has_line(&out, Level::Info, "lid: closed; the times moved"));
    assert_eq!(jobs(&out), [Job::Stop(main_ac.to_owned())]);
    engine.on_job_ended(mins(3), main_ac, true);
    let out = engine.on_due(mins(3));
    assert_eq!(jobs(&out), [Job::Start(lid_ac.to_owned())]);
}

#[test]
fn with_no_lid_in_the_file_the_lid_changes_nothing() {
    let mut engine = engine(STAGES, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_due(0);
    engine.on_job_ended(0, POWER_SAVE, true);
    let before = engine.view();
    let out = engine.on_lid(mins(1), true);
    assert!(out.is_empty(), "{out:?}");
    assert!(engine.view().lid_closed);
    assert_eq!(engine.view().scheduled, before.scheduled);
}

#[test]
fn a_change_with_nothing_entered_is_a_new_count() {
    let mut engine = engine(CHAINS, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    let out = engine.on_lid(secs(30), true);
    assert!(has_line(&out, Level::Info, "lid: closed; the times moved"));
    assert!(jobs(&out).is_empty());
    assert_eq!(engine.view().scheduled, ("lid save".to_owned(), secs(30)));
}

#[test]
fn a_hold_does_not_hold_the_carry_over() {
    let mut engine = entered_on_battery();
    let held = Snapshot {
        sessions: vec![session("1", true, 0)],
        inhibitors: vec![inhibitor("idle", "block-weak")],
    };
    engine.on_snapshot(mins(1) + secs(1), held);
    let out = engine.on_lid(mins(2), true);
    assert!(has_line(&out, Level::Info, "the same commands, none run"));
    assert_eq!(engine.view().state, State::Idle);
}

#[test]
fn a_section_whose_enter_still_runs_carries_over() {
    let mut engine = engine(CHAINS, PowerSource::Battery);
    engine.on_snapshot(0, idle_world(0));
    engine.on_due(mins(1));
    let out = engine.on_power(mins(1) + secs(1), PowerSource::Ac);
    assert!(has_line(&out, Level::Info, "the same commands, none run"));
    assert!(jobs(&out).is_empty());
    let out = engine.on_job_ended(mins(1) + secs(2), SAVE, true);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn the_idle_line_says_the_lid_is_closed() {
    let settings = settings::parse(CHAINS).unwrap();
    let (mut engine, _) = Engine::new(settings, PowerSource::Battery, true, 0, Vec::new());
    let out = engine.on_snapshot(0, idle_world(0));
    assert!(has_line(
        &out,
        Level::Debug,
        "idle, on battery, the lid closed: lid save in 0 s"
    ));
}
