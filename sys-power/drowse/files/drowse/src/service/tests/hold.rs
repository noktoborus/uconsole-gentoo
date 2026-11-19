//! The hold of an inhibitor, through the driver: a mock of logind gives
//! the inhibitors, the times of the chain are seconds, the clock of tokio
//! is paused. A script takes and lets go inhibitors at set times and
//! checks the state, the jobs of systemd and the actions of logind.

use super::script::Script;
use super::*;

/// `main.battery.1`, a command, at 2 s of idle; `main.battery.2`, a power
/// off, 2 s after it, with an announce of 1 s.
const CHAIN: &str = "\
main:
  battery:
    - after: 2s
      enter: tuned-adm profile powersave
      leave: tuned-adm auto_profile
      hint: power save
    - after: 2s
      countdown: 1s
      action: poweroff
";

/// A power off at 2 s of idle, with an announce of 1 s.
const POWEROFF: &str = "main:\n  battery: [{after: 2s, countdown: 1s, action: poweroff}]\n";

/// Two commands, at 2 s of idle and 2 s after.
const TWO_COMMANDS: &str = "\
main:
  battery:
    - {after: 2s, enter: tuned-adm profile powersave, leave: tuned-adm auto_profile}
    - {after: 2s, enter: nmcli radio all off, leave: nmcli radio all on}
";

/// The start of the unit of `main.battery.2`.
const START_1: &str = "start drowsed-stage@main.battery.2.service";

/// The start of the unit of `main.battery.1`.
const START_0: &str = "start drowsed-stage@main.battery.1.service";

/// The stop of the unit of `main.battery.1`.
const STOP_0: &str = "stop drowsed-stage@main.battery.1.service";

/// `what` of `drowse-inhibit on --idle`.
const IDLE: &str = "idle";

/// `what` of `drowse-inhibit on --power`.
const POWER: &str = "shutdown:sleep";

/// A suspend at 2 s of idle, with an announce of 1 s.
const SUSPEND: &str = "main:\n  battery: [{after: 2s, countdown: 1s, action: suspend}]\n";

/// The ways a `what` with `sleep` and `shutdown` can be written: the order
/// of logind, the other order, with other kinds among them.
const POWER_SPELLINGS: [&str; 6] = [
    "shutdown:sleep",
    "sleep:shutdown",
    "idle:sleep:shutdown",
    "sleep:idle:shutdown",
    "handle-lid-switch:shutdown:sleep",
    "sleep:handle-power-key:shutdown:idle",
];

/// An inhibitor of `drowse-inhibit`, of this kind and mode.
fn inhibitor(what: &str, mode: &str) -> Inhibitor {
    Inhibitor {
        what: what.to_owned(),
        who: "drowse-inhibit".to_owned(),
        why: "held by hand".to_owned(),
        mode: mode.to_owned(),
        uid: 1000,
        pid: 4242,
    }
}

#[tokio::test(start_paused = true)]
async fn an_idle_hold_keeps_a_command_section_out_until_let_go() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "block-weak"), true);
        s.at(1_000).await;
        assert_eq!(s.view().state, State::Idle);
        s.at(3_000).await;
        assert!(s.jobs().is_empty(), "entered under a hold: {:?}", s.jobs());
        assert_eq!(s.view().state, State::Held);
        assert_eq!(s.view().inhibitors, [inhibitor(IDLE, "block-weak")]);
        s.at(6_000).await;
        assert!(s.jobs().is_empty(), "entered under a hold: {:?}", s.jobs());
        s.let_go();
        s.at(6_500).await;
        assert_eq!(s.jobs(), [START_0]);
        assert_eq!(s.view().state, State::Idle);
        assert!(s.view().inhibitors.is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn the_read_before_entry_finds_a_hold_no_signal_told_of() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "block-weak"), false);
        s.at(3_000).await;
        assert!(s.jobs().is_empty(), "entered under a hold: {:?}", s.jobs());
        assert_eq!(s.view().state, State::Held);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_block_hold_keeps_a_command_section_out() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "block"), true);
        s.at(3_000).await;
        assert!(s.jobs().is_empty(), "entered under a hold: {:?}", s.jobs());
        assert_eq!(s.view().state, State::Held);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_let_go_before_the_time_keeps_the_time() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "block-weak"), true);
        s.at(1_000).await;
        s.let_go();
        s.at(1_900).await;
        assert!(s.jobs().is_empty());
        s.at(2_100).await;
        assert_eq!(s.jobs(), [START_0]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_delay_inhibitor_holds_nothing() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "delay"), true);
        s.at(2_100).await;
        assert_eq!(s.jobs(), [START_0]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_idle_hold_lets_poweroff_in() {
    let s = Script::new(PowerSource::Battery);
    s.run(POWEROFF, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "block-weak"), true);
        s.at(3_100).await;
        assert_eq!(s.acted(), [LogindAction::Poweroff]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_idle_hold_after_entry_neither_leaves_the_section_nor_holds_poweroff() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(2_500).await;
        assert_eq!(s.jobs(), [START_0]);
        s.take(inhibitor(IDLE, "block-weak"), true);
        s.at(4_500).await;
        assert_eq!(s.jobs(), [START_0], "a hold left the section entered");
        assert_eq!(s.view().state, State::Announced);
        s.at(5_100).await;
        assert_eq!(s.jobs(), [START_0, STOP_0]);
        assert_eq!(s.acted(), [LogindAction::Poweroff]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn both_holds_keep_every_section_out() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(IDLE, "block-weak"), true);
        s.take(inhibitor(POWER, "block-weak"), true);
        s.at(10_000).await;
        assert!(s.jobs().is_empty(), "entered under a hold: {:?}", s.jobs());
        assert!(s.acted().is_empty(), "power off under a hold");
        assert_eq!(s.view().state, State::Held);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_power_hold_lets_a_command_section_in_and_holds_poweroff() {
    let s = Script::new(PowerSource::Battery);
    s.run(CHAIN, async {
        s.at(500).await;
        s.take(inhibitor(POWER, "block-weak"), true);
        s.at(2_100).await;
        assert_eq!(s.jobs(), [START_0]);
        s.at(4_500).await;
        assert_eq!(s.view().state, State::Announced);
        s.at(8_000).await;
        assert!(s.acted().is_empty(), "power off under a hold");
        assert_eq!(s.jobs(), [START_0], "a hold left the section entered");
        assert_eq!(s.view().state, State::Announced);
        assert_eq!(s.view().inhibitors, [inhibitor(POWER, "block-weak")]);
        s.let_go();
        s.at(8_500).await;
        assert_eq!(s.jobs(), [START_0, STOP_0]);
        assert_eq!(s.acted(), [LogindAction::Poweroff]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn the_section_before_is_left_before_a_held_entry_comes_in() {
    let s = Script::new(PowerSource::Battery);
    s.run(TWO_COMMANDS, async {
        s.at(2_500).await;
        assert_eq!(s.jobs(), [START_0]);
        s.take(inhibitor(IDLE, "block-weak"), true);
        s.at(6_000).await;
        assert_eq!(s.jobs(), [START_0], "a hold left the section entered");
        assert_eq!(s.view().state, State::Held);
        s.let_go();
        s.at(6_500).await;
        assert_eq!(s.jobs(), [START_0, STOP_0, START_1]);
    })
    .await;
}

/// A section of this chain is held by an inhibitor of this `what`, and
/// entered once it is let go.
async fn held_until_let_go(chain: &str, what: &str, action: LogindAction) {
    let s = Script::new(PowerSource::Battery);
    s.run(chain, async {
        s.at(500).await;
        s.take(inhibitor(what, "block-weak"), true);
        s.at(6_000).await;
        assert!(s.acted().is_empty(), "{action:?} under a hold of {what}");
        assert_eq!(
            s.view().inhibitors,
            [inhibitor(what, "block-weak")],
            "{what}"
        );
        s.let_go();
        s.at(6_500).await;
        assert_eq!(s.acted(), [action], "{what}");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_power_hold_holds_poweroff_in_any_order_of_what() {
    for what in POWER_SPELLINGS {
        held_until_let_go(POWEROFF, what, LogindAction::Poweroff).await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_power_hold_holds_suspend_in_any_order_of_what() {
    for what in POWER_SPELLINGS {
        held_until_let_go(SUSPEND, what, LogindAction::Suspend).await;
    }
}

#[tokio::test(start_paused = true)]
async fn an_idle_hold_holds_a_command_section_in_any_order_of_what() {
    for what in ["idle", "sleep:idle", "idle:shutdown", "shutdown:sleep:idle"] {
        let s = Script::new(PowerSource::Battery);
        s.run(TWO_COMMANDS, async {
            s.at(500).await;
            s.take(inhibitor(what, "block-weak"), true);
            s.at(6_000).await;
            assert!(s.jobs().is_empty(), "entered under a hold of {what}");
            s.let_go();
            s.at(6_500).await;
            assert_eq!(s.jobs(), [START_0], "{what}");
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_hold_of_one_kind_lets_the_other_in() {
    let s = Script::new(PowerSource::Battery);
    s.run(POWEROFF, async {
        s.at(500).await;
        s.take(inhibitor("sleep", "block"), true);
        s.at(3_100).await;
        assert_eq!(s.acted(), [LogindAction::Poweroff]);
    })
    .await;
    let s = Script::new(PowerSource::Battery);
    s.run(SUSPEND, async {
        s.at(500).await;
        s.take(inhibitor("shutdown", "block"), true);
        s.at(3_100).await;
        assert_eq!(s.acted(), [LogindAction::Suspend]);
    })
    .await;
}
