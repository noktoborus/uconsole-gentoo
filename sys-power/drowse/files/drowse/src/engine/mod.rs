//! The state of the service: the idle state, the stages, the announce and
//! the actions. The engine is pure: it takes what happens, with the time of
//! `CLOCK_MONOTONIC` in microseconds, and gives what to do. The driver
//! does it in the world.

mod units;

use std::time::Duration;

pub use units::{Job, unit_name};

use crate::settings::{Chain, Group, LogindAction, Power, Section, SectionKind, Settings, Span};
use crate::world::what::Hold;
use crate::world::{Inhibitor, PowerSource, Session, SessionState};
use units::{Ended, Units};

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
    /// Not idle: a counted session is in use.
    Active,
    /// Idle; the time runs, or no action comes.
    Idle,
    /// An announce runs.
    Announced,
    /// A due section waits for an inhibitor.
    Held,
}

impl State {
    /// The name on the bus.
    pub fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Announced => "announced",
            Self::Held => "held",
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
    /// Whether the lid is closed, as logind says.
    pub lid_closed: bool,
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
    /// still runs (the `leave` of the section before), and entry waits
    /// for its end.
    Waiting {
        stage: usize,
        at: u64,
        announced: bool,
    },
    /// The jobs ended; the entry into `stage` comes on a fresh read of
    /// logind.
    Ready {
        stage: usize,
        at: u64,
        announced: bool,
    },
    /// Entry waits for an inhibitor of its kind.
    Held {
        stage: usize,
        at: u64,
        announced: bool,
        logged: bool,
    },
    /// Every stage is done.
    Done,
}

/// The engine.
#[derive(Debug)]
pub struct Engine {
    settings: Settings,
    power: PowerSource,
    lid_closed: bool,
    snapshot: Snapshot,
    phase: Phase,
    run_start: u64,
    activity: u64,
    units: Units,
    pending_logind: Option<(usize, bool)>,
}

impl Engine {
    /// The engine at the start of the service; the units of the stages left
    /// up from before are stopped.
    pub fn new(
        settings: Settings,
        power: PowerSource,
        lid_closed: bool,
        now: u64,
        left_up: Vec<String>,
    ) -> (Self, Vec<Command>) {
        let mut engine = Self {
            settings,
            power,
            lid_closed,
            snapshot: Snapshot::default(),
            phase: Phase::Active,
            run_start: now,
            activity: now,
            units: Units::default(),
            pending_logind: None,
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
        self.snapshot = snapshot;
        self.evaluate(now, true, &mut out);
        out
    }

    /// The power source changed.
    pub fn on_power(&mut self, now: u64, power: PowerSource) -> Vec<Command> {
        let mut out = Vec::new();
        if power != self.power && power == PowerSource::Unknown {
            out.push(warning(UPOWER_GONE.to_owned()));
        } else if power != self.power && self.power == PowerSource::Unknown {
            out.push(info("upower: back on the bus".to_owned()));
        }
        let before = self.chain();
        self.power = power;
        let what = format!("power: {}", power_name(power));
        self.change_chain(now, before, &what, &mut out);
        self.evaluate(now, false, &mut out);
        out
    }

    /// The lid was closed or opened, as logind says.
    pub fn on_lid(&mut self, now: u64, closed: bool) -> Vec<Command> {
        let mut out = Vec::new();
        if closed == self.lid_closed {
            return out;
        }
        let before = self.chain();
        self.lid_closed = closed;
        let what = format!("lid: {}", lid_name(closed));
        self.change_chain(now, before, &what, &mut out);
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

    /// Starts a due announce without acting before a fresh read.
    pub fn on_announce_due(&mut self, now: u64) -> Vec<Command> {
        let mut out = Vec::new();
        self.evaluate(now, false, &mut out);
        out
    }

    /// Records that logind accepted the action just sent.
    pub fn on_action_finished(&mut self) {
        self.pending_logind = None;
    }

    /// Records that logind refused an action because it was inhibited.
    pub fn on_action_blocked(&mut self, now: u64, inhibitors: Vec<Inhibitor>) -> Vec<Command> {
        let Some((stage, announced)) = self.pending_logind.take() else {
            return vec![warning(
                "logind refused an action with no pending entry".to_owned(),
            )];
        };
        self.snapshot.inhibitors = inhibitors;
        let Some(section) = self.sections().get(stage) else {
            return vec![warning(
                "logind refused an action with no section".to_owned(),
            )];
        };
        if held_by(section, &self.snapshot.inhibitors).is_empty() {
            return vec![warning(
                "logind refused an action but no matching inhibitor was listed".to_owned(),
            )];
        }
        self.phase = Phase::Held {
            stage,
            at: 0,
            announced,
            logged: false,
        };
        let mut out = Vec::new();
        self.evaluate(now, true, &mut out);
        out
    }

    /// The job of a unit of a stage ended.
    pub fn on_job_ended(&mut self, now: u64, unit: &str, done: bool) -> Vec<Command> {
        let (jobs, ended) = self.units.job_ended(unit, done, now);
        let mut out: Vec<Command> = jobs.into_iter().map(Command::Job).collect();
        if let Phase::Waiting {
            stage, announced, ..
        } = self.phase
            && self.units.all_down()
        {
            self.phase = Phase::Ready {
                stage,
                at: now,
                announced,
            };
        }
        match ended {
            Ended::Quiet => {}
            Ended::StartFailed => out.push(warning(format!(
                "{unit}: the command failed or was killed; no leave"
            ))),
            Ended::ResumeLate => out.push(warning(format!(
                "{unit}: the command ended; its leave runs late"
            ))),
            Ended::StopFailed => out.push(warning(format!(
                "{unit}: the leave failed or was killed; the next section comes"
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
                let stage = self.sections().get(stage)?;
                let Span::Time(idle) = stage.after else {
                    return None;
                };
                let due = match stage.countdown {
                    Span::Never => Due::Action,
                    Span::Time(_) => Due::Announce,
                };
                Some((base.saturating_add(micros(idle)), due))
            }
            Phase::Announced { at, .. } | Phase::Ready { at, .. } => Some((at, Due::Action)),
            Phase::Waiting { .. } | Phase::Held { .. } | Phase::Active | Phase::Done => None,
        }
    }

    /// What the service publishes now.
    pub fn view(&self) -> View {
        let (state, sessions, inhibitors) = match self.phase {
            Phase::Active => (State::Active, self.sessions_in_use(), Vec::new()),
            Phase::Announced { .. }
            | Phase::Waiting {
                announced: true, ..
            }
            | Phase::Ready {
                announced: true, ..
            }
            | Phase::Held {
                announced: true, ..
            } => (State::Announced, Vec::new(), self.held_inhibitors()),
            Phase::Held {
                stage,
                announced: false,
                ..
            } => (
                State::Held,
                Vec::new(),
                self.sections().get(stage).map_or_else(Vec::new, |section| {
                    held_by(section, &self.snapshot.inhibitors)
                }),
            ),
            Phase::Counting { .. }
            | Phase::Waiting {
                announced: false, ..
            }
            | Phase::Ready {
                announced: false, ..
            }
            | Phase::Done => (State::Idle, Vec::new(), Vec::new()),
        };
        View {
            state,
            scheduled: self.scheduled(),
            sessions,
            inhibitors,
            power: self.power,
            lid_closed: self.lid_closed,
        }
    }

    fn held_inhibitors(&self) -> Vec<Inhibitor> {
        let Phase::Held { stage, .. } = self.phase else {
            return Vec::new();
        };
        self.sections().get(stage).map_or_else(Vec::new, |section| {
            held_by(section, &self.snapshot.inhibitors)
        })
    }

    /// The action of the next stage and its time.
    fn scheduled(&self) -> (String, u64) {
        let none = (String::new(), 0);
        match self.phase {
            Phase::Counting { stage, base } => {
                let Some(stage) = self.sections().get(stage) else {
                    return none;
                };
                let Span::Time(idle) = stage.after else {
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
            | Phase::Waiting { stage, at, .. }
            | Phase::Ready { stage, at, .. }
            | Phase::Held { stage, at, .. } => self
                .sections()
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
        if let Phase::Held {
            stage,
            at,
            announced,
            logged,
        } = self.phase
        {
            let Some(section) = self.sections().get(stage) else {
                return;
            };
            let inhibitors = held_by(section, &self.snapshot.inhibitors);
            if inhibitors.is_empty() {
                if logged {
                    out.push(info(format!("released: {} now", self.named(stage))));
                }
                self.phase = if announced && at > now {
                    Phase::Announced { stage, at }
                } else {
                    Phase::Ready {
                        stage,
                        at: now,
                        announced,
                    }
                };
            } else {
                if !logged {
                    out.push(held_line(&self.named(stage), &inhibitors[0]));
                    self.phase = Phase::Held {
                        stage,
                        at,
                        announced,
                        logged: true,
                    };
                }
                return;
            }
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
                | (Phase::Announced { stage, .. }, Due::Action)
                | (Phase::Ready { stage, .. }, Due::Action) => self.act(stage, now, out),
                _ => return,
            }
        }
    }

    /// Starts the announce of a section: entry comes `countdown` from now.
    fn announce(&mut self, stage: usize, now: u64, out: &mut Vec<Command>) {
        let Some(this) = self.sections().get(stage) else {
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

    /// The time of entry into a section came. One section is entered at a
    /// time and one job runs at a time: the section before is left
    /// first (its `leave`), and the entry waits until no job runs.
    fn act(&mut self, stage: usize, now: u64, out: &mut Vec<Command>) {
        let announced = self.was_announced();
        if self.wait_for_hold(stage, announced, out) {
            return;
        }
        if self.units.all_down() {
            self.enter(stage, now, out);
            return;
        }
        out.push(info(format!(
            "section {}: waits for the leave of the section before",
            self.named(stage)
        )));
        self.stop_units(now, out);
        self.phase = Phase::Waiting {
            stage,
            at: now,
            announced,
        };
    }

    /// Enters a stage: its action, if any; then counts the next stage from
    /// now. A command section has a unit: its start runs `enter`, its stop
    /// runs `leave`.
    fn enter(&mut self, stage: usize, now: u64, out: &mut Vec<Command>) {
        let announced = self.was_announced();
        if self.wait_for_hold(stage, announced, out) {
            return;
        }
        let Some(this) = self.sections().get(stage).cloned() else {
            return;
        };
        match &this.kind {
            SectionKind::Action(action) => {
                out.push(info(format!("action: {action}")));
                out.push(Command::Logind(*action));
                self.pending_logind = Some((stage, announced));
            }
            SectionKind::Command {
                enter: None,
                leave: None,
            } => {
                out.push(info(format!(
                    "section {}: entered, no action",
                    self.named(stage)
                )));
            }
            SectionKind::Command { .. } => {
                let unit = unit_name(self.chain(), stage);
                out.push(info(format!("section {}: started", self.named(stage))));
                out.extend(self.units.want_up(&unit, now).into_iter().map(Command::Job));
            }
        }
        self.phase = self.after_entry(stage, now);
    }

    /// The phase after the entry into a stage at `now`: the count of the
    /// next stage, or the end of the chain.
    fn after_entry(&self, stage: usize, now: u64) -> Phase {
        let next = stage + 1;
        if next < self.sections().len() {
            Phase::Counting {
                stage: next,
                base: now,
            }
        } else {
            Phase::Done
        }
    }

    /// The chain in use changed from `before`. When the first section of
    /// the new chain runs the same commands as the section entered, that
    /// section becomes the first one of the new chain, with no command run,
    /// and the next one counts from now. Else the change is activity.
    fn change_chain(&mut self, now: u64, before: Chain, what: &str, out: &mut Vec<Command>) {
        let chain = self.chain();
        if chain == before {
            return;
        }
        if let Some((stage, unit)) = self.carried_over(before) {
            out.push(info(format!(
                "{what}; section {} -> {}: the same commands, none run; {unit} runs on",
                self.named_in(before, stage),
                self.named(0)
            )));
            self.phase = self.after_entry(0, now);
            return;
        }
        self.activity = now;
        out.push(info(format!("{what}; the times moved")));
    }

    /// The stage of `before` that is entered and the unit that runs it,
    /// when its commands are those of the first section of the chain now.
    fn carried_over(&self, before: Chain) -> Option<(usize, String)> {
        let unit = self.units.running()?.to_owned();
        let stage = self.entered(before)?;
        let entered = self.settings.sections(before).get(stage)?;
        let first = self.sections().first()?;
        entered.same_commands(first).then_some((stage, unit))
    }

    /// The stage of this chain entered last, by the phase.
    fn entered(&self, chain: Chain) -> Option<usize> {
        match self.phase {
            Phase::Counting { stage, .. }
            | Phase::Announced { stage, .. }
            | Phase::Waiting { stage, .. }
            | Phase::Ready { stage, .. }
            | Phase::Held { stage, .. } => stage.checked_sub(1),
            Phase::Done => self.settings.sections(chain).len().checked_sub(1),
            Phase::Active => None,
        }
    }

    /// Wants every unit down; a unit whose command still runs is a warning.
    fn stop_units(&mut self, now: u64, out: &mut Vec<Command>) {
        let (jobs, waiting) = self.units.want_all_down(now);
        for (unit, ran) in waiting {
            out.push(warning(format!(
                "{unit}: the command still runs after {}; its leave waits",
                human(Duration::from_micros(ran))
            )));
        }
        out.extend(jobs.into_iter().map(Command::Job));
    }

    /// A section for the journal: its name as in the file, and what it
    /// does: `main.battery.1 (power save)`.
    fn named(&self, stage: usize) -> String {
        self.named_in(self.chain(), stage)
    }

    /// A section of a chain for the journal.
    fn named_in(&self, chain: Chain, stage: usize) -> String {
        let name = chain.section_name(stage);
        match self
            .settings
            .sections(chain)
            .get(stage)
            .map(Section::shown_name)
        {
            Some(shown) if !shown.is_empty() => format!("{name} ({shown})"),
            _ => name,
        }
    }

    /// The chain in use: by the lid and the power source.
    fn chain(&self) -> Chain {
        self.settings.chain(self.lid_closed, section(self.power))
    }

    /// The sections of the chain in use.
    fn sections(&self) -> &[Section] {
        self.settings.sections(self.chain())
    }

    fn was_announced(&self) -> bool {
        matches!(
            self.phase,
            Phase::Announced { .. }
                | Phase::Waiting {
                    announced: true,
                    ..
                }
                | Phase::Ready {
                    announced: true,
                    ..
                }
                | Phase::Held {
                    announced: true,
                    ..
                }
        )
    }

    fn wait_for_hold(&mut self, stage: usize, announced: bool, out: &mut Vec<Command>) -> bool {
        let Some(section) = self.sections().get(stage) else {
            return false;
        };
        let inhibitors = held_by(section, &self.snapshot.inhibitors);
        if inhibitors.is_empty() {
            return false;
        }
        out.push(held_line(&self.named(stage), &inhibitors[0]));
        self.phase = Phase::Held {
            stage,
            at: 0,
            announced,
            logged: true,
        };
        true
    }

    /// The sessions that count in the idle state: not `closing`, of a
    /// class in `sessions.classes` of the settings.
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

    /// Whether every session that counts is idle.
    fn is_idle(&self) -> bool {
        self.counted(&self.snapshot).all(|session| session.idle)
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
        "activity".to_owned()
    }

    /// The line of the journal when the system becomes idle.
    fn idle_line(&self, now: u64) -> String {
        let (name, at) = self.scheduled();
        let on = match self.chain().group {
            Group::Main => power_name(self.power).to_owned(),
            Group::Lid => format!("{}, the lid closed", power_name(self.power)),
        };
        if name.is_empty() {
            return format!("idle, on {on}: no action");
        }
        format!(
            "idle, on {on}: {name} in {}",
            human(Duration::from_micros(at.saturating_sub(now)))
        )
    }
}

/// The inhibitors that hold entry into this section: of the kind its
/// entry needs, whatever else they hold and in whatever order logind
/// gives the kinds.
fn held_by(section: &Section, inhibitors: &[Inhibitor]) -> Vec<Inhibitor> {
    let Some(kind) = hold_of(section) else {
        return Vec::new();
    };
    inhibitors
        .iter()
        .filter(|inhibitor| MODES_THAT_HOLD.contains(&inhibitor.mode.as_str()))
        .filter(|inhibitor| inhibitor.holds().contains(kind))
        .cloned()
        .collect()
}

/// The kind of inhibitor that holds the entry into a section: `idle` for a
/// command, and for an action of logind the kind logind itself checks.
fn hold_of(section: &Section) -> Option<Hold> {
    match &section.kind {
        SectionKind::Command { .. } => Some(Hold::Idle),
        SectionKind::Action(
            LogindAction::Suspend
            | LogindAction::Hibernate
            | LogindAction::HybridSleep
            | LogindAction::SuspendThenHibernate
            | LogindAction::Sleep,
        ) => Some(Hold::Sleep),
        SectionKind::Action(LogindAction::Poweroff | LogindAction::Halt) => Some(Hold::Shutdown),
        SectionKind::Action(_) => None,
    }
}

/// The journal line for an entry that waits for an inhibitor.
fn held_line(named: &str, inhibitor: &Inhibitor) -> Command {
    Command::Journal(
        Level::Info,
        format!(
            "held: {named} by {} (uid {}): \"{}\"",
            inhibitor.who, inhibitor.uid, inhibitor.why
        ),
    )
}

/// The section of the settings for a power source: not known is AC.
fn section(power: PowerSource) -> Power {
    match power {
        PowerSource::Battery => Power::Battery,
        PowerSource::Ac | PowerSource::Unknown => Power::Ac,
    }
}

/// The state of the lid in the journal and on the bus.
pub fn lid_name(closed: bool) -> &'static str {
    if closed { "closed" } else { "open" }
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
