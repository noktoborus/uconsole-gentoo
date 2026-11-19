//! The power and the lid through the driver: every change of the chain
//! in use, one at a time, with a chain of seconds and the clock of tokio
//! paused. The same commands carry over; other commands leave and enter.

use super::script::Script;
use super::*;

/// The same commands in `main.battery.1`, `main.ac.1` and
/// `lid.battery.1`; others in `lid.ac.1`.
const CHAINS: &str = "\
main:
  battery:
    - {after: 2s, enter: save, leave: unsave}
    - {after: 20s, countdown: 1s, action: poweroff}
  ac:
    - {after: 2s, enter: save, leave: unsave}
lid:
  battery:
    - {after: 0, enter: save, leave: unsave}
    - {after: 10s, countdown: 1s, action: poweroff}
  ac:
    - {after: 0, enter: dim, leave: undim}
";

/// A start of the unit of a section.
fn start(section: &str) -> String {
    format!("start drowsed-stage@{section}.service")
}

/// A stop of the unit of a section.
fn stop(section: &str) -> String {
    format!("stop drowsed-stage@{section}.service")
}

/// Microseconds of the clock of the service.
const fn secs(n: u64) -> u64 {
    n * 1_000_000
}

#[tokio::test(start_paused = true)]
async fn the_same_commands_carry_over_through_the_power_and_the_lid() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAINS, async {
        let entered = [start("main.battery.1")];
        s.at(2_500).await;
        assert_eq!(s.jobs(), entered);
        s.at(3_000).await;
        s.power(PowerSource::Ac);
        s.at(3_200).await;
        assert_eq!(s.jobs(), entered, "main.battery -> main.ac");
        assert_eq!(s.view().scheduled, (String::new(), 0));
        s.at(4_000).await;
        s.power(PowerSource::Battery);
        s.at(4_200).await;
        assert_eq!(s.jobs(), entered, "main.ac -> main.battery");
        assert_eq!(s.view().scheduled.1, secs(4 + 21));
        s.at(5_000).await;
        s.lid(true);
        s.at(5_200).await;
        assert_eq!(s.jobs(), entered, "main.battery -> lid.battery");
        assert!(s.view().lid_closed);
        assert_eq!(s.view().scheduled.1, secs(5 + 11));
        s.at(6_000).await;
        s.lid(false);
        s.at(6_200).await;
        assert_eq!(s.jobs(), entered, "lid.battery -> main.battery");
        assert_eq!(s.view().scheduled.1, secs(6 + 21));
        assert_eq!(s.view().state, State::Idle);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn other_commands_leave_and_enter() {
    let s = Script::new(PowerSource::Ac);
    s.run(CHAINS, async {
        let mut jobs = vec![start("main.ac.1")];
        s.at(2_500).await;
        assert_eq!(s.jobs(), jobs);
        s.lid(true);
        s.at(3_500).await;
        jobs.extend([stop("main.ac.1"), start("lid.ac.1")]);
        assert_eq!(s.jobs(), jobs, "main.ac -> lid.ac");
        s.lid(false);
        s.at(4_500).await;
        jobs.push(stop("lid.ac.1"));
        assert_eq!(s.jobs(), jobs, "lid.ac -> main.ac: a new count");
        s.at(6_500).await;
        jobs.push(start("main.ac.1"));
        assert_eq!(s.jobs(), jobs);
        s.lid(true);
        s.at(7_500).await;
        jobs.extend([stop("main.ac.1"), start("lid.ac.1")]);
        assert_eq!(s.jobs(), jobs);
        s.power(PowerSource::Battery);
        s.at(8_500).await;
        jobs.extend([stop("lid.ac.1"), start("lid.battery.1")]);
        assert_eq!(s.jobs(), jobs, "lid.ac -> lid.battery");
        s.power(PowerSource::Ac);
        s.at(9_500).await;
        jobs.extend([stop("lid.battery.1"), start("lid.ac.1")]);
        assert_eq!(s.jobs(), jobs, "lid.battery -> lid.ac");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn the_lid_chain_powers_off_after_a_carry_over() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAINS, async {
        s.at(3_000).await;
        s.lid(true);
        s.at(12_900).await;
        assert!(s.acted().is_empty());
        s.at(13_500).await;
        assert_eq!(s.view().state, State::Announced);
        s.at(14_500).await;
        assert_eq!(s.acted(), [LogindAction::Poweroff]);
        assert_eq!(s.jobs(), [start("main.battery.1"), stop("main.battery.1")]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn activity_after_a_carry_over_leaves_the_unit_that_runs() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAINS, async {
        s.at(3_000).await;
        s.lid(true);
        s.at(4_000).await;
        s.idle(false, s.now());
        s.at(4_500).await;
        assert_eq!(s.jobs(), [start("main.battery.1"), stop("main.battery.1")]);
        assert_eq!(s.view().state, State::Active);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn the_lid_closed_in_use_waits_for_idle() {
    let s = Script::new(PowerSource::Battery);
    s.idle(false, 0);
    s.run(CHAINS, async {
        s.at(1_000).await;
        s.lid(true);
        s.at(2_000).await;
        assert!(s.jobs().is_empty());
        assert_eq!(s.view().state, State::Active);
        s.idle(true, s.now());
        s.at(2_500).await;
        assert_eq!(s.jobs(), [start("lid.battery.1")]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_lid_chain_not_set_is_empty() {
    let chains = "\
main:
  ac: [{after: 2s, enter: save, leave: unsave}]
lid:
  battery: [{after: 0, enter: save, leave: unsave}]
";
    let s = Script::new(PowerSource::Ac);
    s.run(chains, async {
        s.at(3_000).await;
        s.lid(true);
        s.at(10_000).await;
        assert_eq!(s.jobs(), [start("main.ac.1"), stop("main.ac.1")]);
        assert_eq!(s.view().state, State::Idle);
        assert_eq!(s.view().scheduled, (String::new(), 0));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn with_no_lid_in_the_file_the_lid_is_not_looked_at() {
    let chains = "main:\n  battery: [{after: 2s, enter: save, leave: unsave}]\n";
    let s = Script::new(PowerSource::Battery);
    s.run(chains, async {
        s.at(1_000).await;
        s.lid(true);
        s.at(2_500).await;
        assert_eq!(s.jobs(), [start("main.battery.1")]);
        s.lid(false);
        s.at(3_000).await;
        assert_eq!(s.jobs(), [start("main.battery.1")]);
    })
    .await;
}
