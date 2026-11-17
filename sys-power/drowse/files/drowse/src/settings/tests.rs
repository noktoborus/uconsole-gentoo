//! Tests of the settings.

use super::*;

fn minutes(n: u64) -> Span {
    Span::Time(Duration::from_secs(n * 60))
}

fn command(line: &str) -> Command {
    Command::parse(line).unwrap()
}

#[test]
fn no_file_is_the_defaults() {
    let path = std::env::temp_dir().join(format!("drowse-no-file-{}", std::process::id()));
    let (settings, warnings) = load(&path).unwrap();
    assert_eq!(settings, Settings::default());
    assert!(warnings.is_empty());
}

#[test]
fn defaults_do_nothing() {
    let (settings, warnings) = parse("");
    assert!(warnings.is_empty());
    let battery = settings.stages(Power::Battery);
    assert_eq!(battery.len(), 1);
    assert_eq!(battery[0].idle_time, Span::Never);
    assert_eq!(battery[0].countdown, minutes(2));
    assert_eq!(battery[0].action, None);
    assert_eq!(settings.stages(Power::Ac)[0].idle_time, Span::Never);
}

#[test]
fn an_idle_time_alone_has_no_action() {
    let (settings, warnings) = parse("[IdleBattery]\nIdleTime=20m\n");
    assert!(warnings.is_empty());
    let first = &settings.stages(Power::Battery)[0];
    assert_eq!(first.idle_time, minutes(20));
    assert_eq!(first.countdown, minutes(2));
    assert_eq!(first.action, None);
    assert_eq!(first.shown_name(), "");
}

#[test]
fn the_example_of_the_requirements() {
    let text = "\
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
    let (settings, warnings) = parse(text);
    assert!(warnings.is_empty(), "{warnings:?}");
    let battery = settings.stages(Power::Battery);
    assert_eq!(battery.len(), 3);
    assert_eq!(battery[0].idle_time, Span::Time(Duration::ZERO));
    assert_eq!(battery[0].countdown, Span::Never);
    assert_eq!(
        battery[0].action,
        Some(Action::Command(command("tuned-adm profile powersave")))
    );
    assert_eq!(battery[0].shown_name(), "power save");
    assert_eq!(battery[0].resume, Some(command("tuned-adm auto_profile")));
    assert_eq!(battery[1].idle_time, minutes(5));
    assert_eq!(battery[1].shown_name(), "radio off");
    assert_eq!(
        battery[2].action,
        Some(Action::Logind(LogindAction::Poweroff))
    );
    assert_eq!(battery[2].shown_name(), "poweroff");
    let ac = settings.stages(Power::Ac);
    assert_eq!(ac.len(), 1);
    assert_eq!(ac[0].idle_time, Span::Never);
}

#[test]
fn a_command_with_no_hint_shows_its_line() {
    let (settings, _) = parse("[IdleAC]\nAction=/usr/local/bin/backup --quiet\n");
    assert_eq!(
        settings.stages(Power::Ac)[0].shown_name(),
        "/usr/local/bin/backup --quiet"
    );
}

#[test]
fn a_wrong_value_takes_its_default() {
    let (settings, warnings) = parse("[IdleBattery]\nIdleTime=soon\nCountdown=1x\n");
    let first = &settings.stages(Power::Battery)[0];
    assert_eq!(first.idle_time, Span::Never);
    assert_eq!(first.countdown, minutes(2));
    assert_eq!(warnings.len(), 2);
    assert!(matches!(&warnings[0], Warning::WrongValue { key, .. } if key == "IdleTime"));
}

#[test]
fn a_wrong_action_is_no_action() {
    let (settings, warnings) = parse("[IdleBattery]\nAction=\"not closed\n");
    assert_eq!(settings.stages(Power::Battery)[0].action, None);
    assert!(matches!(&warnings[0], Warning::WrongValue { key, .. } if key == "Action"));
}

#[test]
fn unknown_keys_and_sections() {
    let (_, warnings) = parse("[IdleBattery]\nTimeout=5m\n[Other]\nA=1\nB=2\n");
    assert_eq!(
        warnings,
        [
            Warning::UnknownKey {
                section: "IdleBattery".to_owned(),
                key: "Timeout".to_owned(),
                line: 2,
            },
            Warning::UnknownSection {
                name: "Other".to_owned(),
                line: 4,
            },
        ]
    );
}

#[test]
fn the_first_stage_written_twice() {
    let text = "[IdleBattery]\nIdleTime=1m\nAction=suspend\n[IdleBattery-0]\nIdleTime=3m\n";
    let (settings, warnings) = parse(text);
    let first = &settings.stages(Power::Battery)[0];
    assert_eq!(first.idle_time, minutes(3));
    assert_eq!(first.action, Some(Action::Logind(LogindAction::Suspend)));
    assert_eq!(
        warnings,
        [Warning::FirstStageTwice {
            power: Power::Battery
        }]
    );
}

#[test]
fn a_gap_ends_the_stages() {
    let text =
        "[IdleAC-1]\nAction=lock\n[IdleAC-3]\nAction=poweroff\n[IdleAC-4]\nAction=poweroff\n";
    let (settings, warnings) = parse(text);
    assert_eq!(settings.stages(Power::Ac).len(), 2);
    assert_eq!(
        warnings,
        [Warning::GapInStages {
            power: Power::Ac,
            missing: 2,
            passed_over: vec!["IdleAC-3".to_owned(), "IdleAC-4".to_owned()],
        }]
    );
}

#[test]
fn a_stage_needs_no_action() {
    let text = "[IdleBattery-1]\nIdleTime=5m\nResume=foo\n[IdleBattery-2]\nAction=poweroff\n";
    let (settings, warnings) = parse(text);
    let battery = settings.stages(Power::Battery);
    assert_eq!(battery.len(), 3);
    assert_eq!(battery[1].action, None);
    assert_eq!(battery[1].resume, Some(command("foo")));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn resume_of_logind_is_passed_over() {
    let (settings, warnings) = parse("[IdleBattery]\nAction=suspend\nResume=foo\n");
    assert_eq!(settings.stages(Power::Battery)[0].resume, None);
    assert_eq!(
        warnings,
        [Warning::ResumeOfLogind {
            section: "IdleBattery".to_owned(),
            line: 3,
        }]
    );
}

#[test]
fn a_later_key_wins() {
    let (settings, warnings) = parse("[IdleAC]\nIdleTime=1m\nIdleTime=2m\n");
    assert_eq!(settings.stages(Power::Ac)[0].idle_time, minutes(2));
    assert!(warnings.is_empty());
}

#[test]
fn sections_that_are_not_of_a_stage() {
    for name in [
        "IdleBattery-",
        "IdleBattery-x",
        "IdleBattery--1",
        "IdleBattery-+1",
        "idlebattery",
    ] {
        assert!(stage_of_section(name).is_none(), "{name}");
    }
}

#[test]
fn the_classes_of_the_sessions() {
    assert!(Settings::default().counts_class("user"));
    assert!(Settings::default().counts_class("greeter"));
    assert!(!Settings::default().counts_class("background"));
    let (settings, warnings) = parse("[Sessions]\nClasses=user background\nIdle=1\n");
    assert!(settings.counts_class("background"));
    assert!(!settings.counts_class("greeter"));
    assert_eq!(
        warnings,
        [Warning::UnknownKey {
            section: "Sessions".to_owned(),
            key: "Idle".to_owned(),
            line: 3,
        }]
    );
}

#[test]
fn the_file_of_the_package() {
    let (settings, warnings) = parse(include_str!("../../etc/drowse.conf"));
    assert!(warnings.is_empty(), "{warnings:?}");
    let battery = settings.stages(Power::Battery);
    assert_eq!(battery.len(), 2);
    assert_eq!(battery[0].idle_time, minutes(10));
    assert_eq!(battery[0].countdown, Span::Never);
    assert_eq!(
        battery[0].action,
        Some(Action::Command(command("tuned-adm profile cm5-powersave")))
    );
    assert_eq!(battery[0].shown_name(), "power save");
    assert_eq!(
        battery[0].resume,
        Some(command("systemctl try-restart tuned-ppd.service"))
    );
    assert_eq!(battery[1].idle_time, minutes(38));
    assert_eq!(battery[1].countdown, minutes(2));
    assert_eq!(
        battery[1].action,
        Some(Action::Logind(LogindAction::Poweroff))
    );
    let ac = settings.stages(Power::Ac);
    assert_eq!(ac.len(), 1);
    assert_eq!(ac[0].idle_time, Span::Never);
    assert!(settings.counts_class("greeter"));
}
