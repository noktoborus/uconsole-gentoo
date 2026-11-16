//! The state of the service: the idle state, the stages, the announce and
//! the actions. The engine is pure: it takes what happens, with the time of
//! `CLOCK_MONOTONIC` in microseconds, and gives what to do. The driver
//! does it in the world.

mod units;

use std::time::Duration;

pub use units::{Job, unit_name};

use crate::settings::{Action, LogindAction, Power, Settings, Span, Stage};
use crate::world::{Inhibitor, PowerSource, Session, SessionState};
use units::{Ended, Units};

/// The kinds of an inhibitor that hold the system up, whatever the action.
const KINDS_THAT_HOLD: [&str; 3] = ["idle", "sleep", "shutdown"];

/// The line of the journal when UPower does not answer.
const UPOWER_GONE: &str = "upower: no answer; the power is taken as AC";

/// The modes of an inhibitor that hold.
const MODES_THAT_HOLD: [&str; 2] = ["block", "block-weak"];

/// The sessions and the inhibitors, as read from logind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Every session.
    pub sessions: Vec<Session>,
    /// Every inhibitor.
    pub inhibitors: Vec<Inhibitor>,
}

/// The state on the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not idle: a session in use or an inhibitor.
    Active,
    /// Idle; the time runs, or no action comes.
    Idle,
    /// An announce runs.
    Announced,
}

impl State {
    /// The name on the bus.
    pub fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Announced => "announced",
        }
    }
}

/// What the service publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// The state.
    pub state: State,
    /// The action of the next stage and when it comes, in microseconds of
    /// `CLOCK_MONOTONIC`; `("", 0)` when none comes.
    pub scheduled: (String, u64),
    /// The sessions that hold the system up.
    pub sessions: Vec<Session>,
    /// The inhibitors that hold the system up.
    pub inhibitors: Vec<Inhibitor>,
    /// The power source.
    pub power: PowerSource,
}

/// The level of a line of the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// A change of state.
    Info,
    /// A cancel.
    Notice,
    /// A source not there, a refusal, a command that waits or is killed.
    Warning,
}

/// What the driver does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Calls an action of logind.
    Logind(LogindAction),
    /// Asks a job of systemd.
    Job(Job),
    /// Writes a line to the journal.
    Journal(Level, String),
}

/// What comes at the next time of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// The announce starts.
    Announce,
    /// The action comes: the driver reads the world again first.
    Action,
}

/// Where the count is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Not idle.
    Active,
    /// The idle time of a stage runs from `base`.
    Counting { stage: usize, base: u64 },
    /// The announce of a stage runs; its action comes at `at`.
    Announced { stage: usize, at: u64 },
    /// The time of the entry into a stage came at `at`; a job of a unit
    /// still runs (the `Resume` of the stage before), and the entry waits
    /// for its end.
    Waiting { stage: usize, at: u64 },
    /// The jobs ended; the entry into `stage` comes on a fresh read of
    /// logind.
    Ready { stage: usize, at: u64 },
    /// Every stage is done.
    Done,
}

/// The engine.
#[derive(Debug)]
pub struct Engine {
    settings: Settings,
    power: PowerSource,
    snapshot: Snapshot,
    phase: Phase,
    run_start: u64,
    activity: u64,
    units: Units,
}

impl Engine {
    /// The engine at the start of the service; the units of the stages left
    /// up from before are stopped.
    pub fn new(
        settings: Settings,
        power: PowerSource,
        now: u64,
        left_up: Vec<String>,
    ) -> (Self, Vec<Command>) {
        let mut engine = Self {
            settings,
            power,
            snapshot: Snapshot::default(),
            phase: Phase::Active,
            run_start: now,
            activity: now,
            units: Units::default(),
        };
        let mut out: Vec<Command> = engine
            .units
            .adopt_left_up(left_up)
            .into_iter()
            .map(Command::Job)
            .collect();
        if power == PowerSource::Unknown {
            out.push(warning(UPOWER_GONE.to_owned()));
        }
        (engine, out)
    }

    /// The sessions and the inhibitors as read now, fresh from logind.
    pub fn on_snapshot(&mut self, now: u64, snapshot: Snapshot) -> Vec<Command> {
        let mut out = Vec::new();
        if self.a_session_in_use_left(&snapshot) {
            self.activity = now;
        }
        let was_held = !holding(&self.snapshot.inhibitors).is_empty();
        let is_held = !holding(&snapshot.inhibitors).is_empty();
        if was_held && !is_held {
            self.activity = now;
            out.push(info("inhibitors let go: a new count".to_owned()));
        }
        self.snapshot = snapshot;
        self.evaluate(now, true, &mut out);
        out
    }

    /// The power source changed.
    pub fn on_power(&mut self, now: u64, power: PowerSource) -> Vec<Command> {
        let mut out = Vec::new();
        let changed = section(power) != section(self.power);
        if power != self.power && power == PowerSource::Unknown {
            out.push(warning(UPOWER_GONE.to_owned()));
        } else if power != self.power && self.power == PowerSource::Unknown {
            out.push(info("upower: back on the bus".to_owned()));
        }
        self.power = power;
        if changed {
            self.activity = now;
            out.push(info(format!(
                "power: {}; the times moved",
                power_name(power)
            )));
        }
        self.evaluate(now, false, &mut out);
        out
    }

    /// The next time comes; for `Due::Action` the driver gave a fresh
    /// snapshot just before.
    pub fn on_due(&mut self, now: u64) -> Vec<Command> {
        let mut out = Vec::new();
        self.evaluate(now, true, &mut out);
        out
    }

    /// The job of a unit of a stage ended.
    pub fn on_job_ended(&mut self, now: u64, unit: &str, done: bool) -> Vec<Command> {
        let (jobs, ended) = self.units.job_ended(unit, done, now);
        let mut out: Vec<Command> = jobs.into_iter().map(Command::Job).collect();
        if let Phase::Waiting { stage, .. } = self.phase
            && self.units.all_down()
        {
            self.phase = Phase::Ready { stage, at: now };
        }
        match ended {
            Ended::Quiet => {}
            Ended::StartFailed => out.push(warning(format!(
                "{unit}: the command failed or was killed; no Resume"
            ))),
            Ended::ResumeLate => out.push(warning(format!(
                "{unit}: the command ended; its Resume runs late"
            ))),
            Ended::StopFailed => out.push(warning(format!(
                "{unit}: the Resume failed or was killed; the next stage comes"
            ))),
        }
        out
    }

    /// The service stops: every unit of a stage that ran is stopped; the
    /// state stays as it is until the service leaves the bus.
    pub fn stop(&mut self, now: u64) -> Vec<Command> {
        let mut out = Vec::new();
        self.stop_units(now, &mut out);
        out
    }

    /// Whether no job runs and no unit of a stage is up.
    pub fn settled(&self) -> bool {
        self.units.all_down()
    }

    /// The next time of the engine, if any.
    pub fn next_due(&self) -> Option<(u64, Due)> {
        match self.phase {
            Phase::Counting { stage, base } => {
                let stage = self.stages().get(stage)?;
                let Span::Time(idle) = stage.idle_time else {
                    return None;
                };
                let due = match stage.countdown {
                    Span::Never => Due::Action,
                    Span::Time(_) => Due::Announce,
                };
                Some((base.saturating_add(micros(idle)), due))
            }
            Phase::Announced { at, .. } | Phase::Ready { at, .. } => Some((at, Due::Action)),
            Phase::Waiting { .. } | Phase::Active | Phase::Done => None,
        }
    }

    /// What the service publishes now.
    pub fn view(&self) -> View {
        let (state, sessions, inhibitors) = match self.phase {
            Phase::Active => (
                State::Active,
                self.sessions_in_use(),
                holding(&self.snapshot.inhibitors),
            ),
            Phase::Announced { .. } | Phase::Waiting { .. } | Phase::Ready { .. } => {
                (State::Announced, Vec::new(), Vec::new())
            }
            Phase::Counting { .. } | Phase::Done => (State::Idle, Vec::new(), Vec::new()),
        };
        View {
            state,
            scheduled: self.scheduled(),
            sessions,
            inhibitors,
            power: self.power,
        }
    }

    /// The action of the next stage and its time.
    fn scheduled(&self) -> (String, u64) {
        let none = (String::new(), 0);
        match self.phase {
            Phase::Counting { stage, base } => {
                let Some(stage) = self.stages().get(stage) else {
                    return none;
                };
                let Span::Time(idle) = stage.idle_time else {
                    return none;
                };
                let countdown = match stage.countdown {
                    Span::Time(countdown) => micros(countdown),
                    Span::Never => 0,
                };
                let at = base.saturating_add(micros(idle)).saturating_add(countdown);
                (stage.shown_name().to_owned(), at)
            }
            Phase::Announced { stage, at }
            | Phase::Waiting { stage, at }
            | Phase::Ready { stage, at } => self
                .stages()
                .get(stage)
                .map_or(none, |stage| (stage.shown_name().to_owned(), at)),
            Phase::Active | Phase::Done => none,
        }
    }

    /// Moves the state to what the world says now. `fresh`: the snapshot
    /// was read just now, so an action may come.
    fn evaluate(&mut self, now: u64, fresh: bool, out: &mut Vec<Command>) {
        if !self.is_idle() {
            if self.phase != Phase::Active {
                out.push(Command::Journal(
                    Level::Notice,
                    format!("cancel: {}", self.what_holds()),
                ));
                self.stop_units(now, out);
                self.phase = Phase::Active;
            }
            return;
        }
        let start = self.idle_start();
        if self.phase == Phase::Active || start > self.run_start {
            if self.phase != Phase::Active {
                self.stop_units(now, out);
            }
            self.run_start = start;
            self.phase = Phase::Counting {
                stage: 0,
                base: start,
            };
            out.push(info(self.idle_line(now)));
        }
        self.advance(now, fresh, out);
    }

    /// Fires every time that has come.
    fn advance(&mut self, now: u64, fresh: bool, out: &mut Vec<Command>) {
        while let Some((at, due)) = self.next_due() {
            if at > now || (due == Due::Action && !fresh) {
                return;
            }
            match (self.phase, due) {
                (Phase::Counting { stage, .. }, Due::Announce) => self.announce(stage, now, out),
                (Phase::Counting { stage, .. }, Due::Action)
                | (Phase::Announced { stage, .. }, Due::Action) => self.act(stage, now, out),
                (Phase::Ready { stage, .. }, Due::Action) => self.enter(stage, now, out),
                _ => return,
            }
        }
    }

    /// Starts the announce of a stage: its action comes `Countdown` from now.
    fn announce(&mut self, stage: usize, now: u64, out: &mut Vec<Command>) {
        let Some(this) = self.stages().get(stage) else {
            return;
        };
        let countdown = match this.countdown {
            Span::Time(countdown) => countdown,
            Span::Never => Duration::ZERO,
        };
        out.push(info(format!(
            "announce: {} in {}",
            this.shown_name(),
            human(countdown)
        )));
        self.phase = Phase::Announced {
            stage,
            at: now.saturating_add(micros(countdown)),
        };
    }

    /// The time of the entry into a stage came. One stage is entered at a
    /// time and one job runs at a time: the stage entered before is left
    /// first (its `Resume`), and the entry waits until no job runs.
    fn act(&mut self, stage: usize, now: u64, out: &mut Vec<Command>) {
        if self.units.all_down() {
            self.enter(stage, now, out);
            return;
        }
        out.push(info(format!(
            "stage {stage}: waits for the Resume of the stage before"
        )));
        self.stop_units(now, out);
        self.phase = Phase::Waiting { stage, at: now };
    }

    /// Enters a stage: its action, if any; then counts the next stage from
    /// now. A stage with a command or a `Resume` has a unit: its start runs
    /// the command, its stop the `Resume`.
    fn enter(&mut self, stage: usize, now: u64, out: &mut Vec<Command>) {
        let Some(this) = self.stages().get(stage).cloned() else {
            return;
        };
        match (&this.action, &this.resume) {
            (Some(Action::Logind(action)), _) => {
                out.push(info(format!("action: {action}")));
                out.push(Command::Logind(*action));
            }
            (None, None) => out.push(info(format!("stage {stage}: entered, no action"))),
            (Some(Action::Command(_)), _) | (None, Some(_)) => {
                let unit = unit_name(section(self.power), stage);
                out.push(info(format!(
                    "stage {stage} ({}): started",
                    this.shown_name()
                )));
                out.extend(self.units.want_up(&unit, now).into_iter().map(Command::Job));
            }
        }
        let next = stage + 1;
        self.phase = if next < self.stages().len() {
            Phase::Counting {
                stage: next,
                base: now,
            }
        } else {
            Phase::Done
        };
    }

    /// Wants every unit down; a unit whose command still runs is a warning.
    fn stop_units(&mut self, now: u64, out: &mut Vec<Command>) {
        let (jobs, waiting) = self.units.want_all_down(now);
        for (unit, ran) in waiting {
            out.push(warning(format!(
                "{unit}: the command still runs after {}; its Resume waits",
                human(Duration::from_micros(ran))
            )));
        }
        out.extend(jobs.into_iter().map(Command::Job));
    }

    /// The stages of the power source now.
    fn stages(&self) -> &[Stage] {
        self.settings.stages(section(self.power))
    }

    /// The sessions that count in the idle state: not `closing`, of a
    /// class of `Classes=` of `[Sessions]`.
    fn counted<'a>(&'a self, snapshot: &'a Snapshot) -> impl Iterator<Item = &'a Session> {
        snapshot.sessions.iter().filter(|session| {
            session.state != SessionState::Closing && self.settings.counts_class(&session.class)
        })
    }

    /// The sessions that count and are in use.
    fn sessions_in_use(&self) -> Vec<Session> {
        self.counted(&self.snapshot)
            .filter(|session| !session.idle)
            .cloned()
            .collect()
    }

    /// Whether a session in use is gone from the count since the last
    /// snapshot: it counts as activity.
    fn a_session_in_use_left(&self, next: &Snapshot) -> bool {
        self.counted(&self.snapshot)
            .filter(|session| !session.idle)
            .any(|session| !self.counted(next).any(|kept| kept.id == session.id))
    }

    /// Whether the system is idle: every session that counts is idle and no
    /// inhibitor holds.
    fn is_idle(&self) -> bool {
        self.counted(&self.snapshot).all(|session| session.idle)
            && holding(&self.snapshot.inhibitors).is_empty()
    }

    /// The start of the idle state: the latest start of idle of the
    /// sessions, no sooner than the last activity the service saw.
    fn idle_start(&self) -> u64 {
        self.counted(&self.snapshot)
            .map(|session| session.idle_since)
            .fold(self.activity, u64::max)
    }

    /// What holds the system up, in words.
    fn what_holds(&self) -> String {
        if let Some(session) = self.sessions_in_use().first() {
            return format!("session {} ({}) in use", session.id, session.user);
        }
        if let Some(inhibitor) = holding(&self.snapshot.inhibitors).first() {
            return format!(
                "inhibitor of {} by \"{}\" (uid {}): \"{}\"",
                inhibitor.what, inhibitor.who, inhibitor.uid, inhibitor.why
            );
        }
        "activity".to_owned()
    }

    /// The line of the journal when the system becomes idle.
    fn idle_line(&self, now: u64) -> String {
        let (name, at) = self.scheduled();
        if name.is_empty() {
            return format!("idle, on {}: no action", power_name(self.power));
        }
        format!(
            "idle, on {}: {name} in {}",
            power_name(self.power),
            human(Duration::from_micros(at.saturating_sub(now)))
        )
    }
}

/// The inhibitors that hold the system up.
pub(crate) fn holding(inhibitors: &[Inhibitor]) -> Vec<Inhibitor> {
    inhibitors
        .iter()
        .filter(|inhibitor| MODES_THAT_HOLD.contains(&inhibitor.mode.as_str()))
        .filter(|inhibitor| {
            inhibitor
                .what
                .split(':')
                .any(|kind| KINDS_THAT_HOLD.contains(&kind))
        })
        .cloned()
        .collect()
}

/// The section of the settings for a power source: not known is AC.
fn section(power: PowerSource) -> Power {
    match power {
        PowerSource::Battery => Power::Battery,
        PowerSource::Ac | PowerSource::Unknown => Power::Ac,
    }
}

/// The name of a power source in the journal.
fn power_name(power: PowerSource) -> &'static str {
    match power {
        PowerSource::Battery => "battery",
        PowerSource::Ac => "AC",
        PowerSource::Unknown => "AC (UPower does not answer)",
    }
}

/// A duration in microseconds.
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// A duration for the journal: `2 min`, `1 h 20 min`, `45 s`.
fn human(duration: Duration) -> String {
    let seconds = duration.as_secs();
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds.div_ceil(60)),
        _ => format!("{} h {} min", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// A line of the journal at the level `info`.
fn info(text: String) -> Command {
    Command::Journal(Level::Info, text)
}

/// A line of the journal at the level `warning`.
fn warning(text: String) -> Command {
    Command::Journal(Level::Warning, text)
}

#[cfg(test)]
mod tests;
