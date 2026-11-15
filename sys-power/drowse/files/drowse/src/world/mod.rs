//! The world outside the service: logind, sd-login, UPower and the units
//! of systemd. The service sees it only through the traits here, so the
//! tests can give it mocks.

pub mod system;

pub(crate) mod logind;
mod systemd;
mod upower;

use std::future::Future;

use crate::settings::LogindAction;

/// The state of a session, as logind gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Logged in, in the foreground of its seat.
    Active,
    /// Logged in, not in the foreground.
    Online,
    /// Logged out; processes of it may live on.
    Closing,
}

impl SessionState {
    /// Reads the state as logind writes it; an unknown one is `Online`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "active" => Self::Active,
            "closing" => Self::Closing,
            _ => Self::Online,
        }
    }
}

/// A session of logind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The id of the session.
    pub id: String,
    /// The uid of its user.
    pub uid: u32,
    /// The name of its user.
    pub user: String,
    /// Its class: `user`, `greeter`, `background`, …
    pub class: String,
    /// Its type: `wayland`, `tty`, …
    pub kind: String,
    /// Its tty, empty when it has none.
    pub tty: String,
    /// Its remote host, empty when it has none.
    pub remote_host: String,
    /// Whether logind takes it as idle.
    pub idle: bool,
    /// Since when it is idle or in use, in microseconds of
    /// `CLOCK_MONOTONIC`.
    pub idle_since: u64,
    /// Its state.
    pub state: SessionState,
}

/// An inhibitor of logind, in the form of `ListInhibitors()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inhibitor {
    /// What it holds, the kinds joined by `:`.
    pub what: String,
    /// Who holds it.
    pub who: String,
    /// Why.
    pub why: String,
    /// Its mode: `block`, `block-weak` or `delay`.
    pub mode: String,
    /// The uid of the one that holds it.
    pub uid: u32,
    /// The pid of the one that holds it.
    pub pid: u32,
}

/// The power source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    /// On battery.
    Battery,
    /// On AC.
    Ac,
    /// UPower does not answer; the power is taken as AC.
    Unknown,
}

/// Why logind did not do an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// An inhibitor was taken just before the call.
    Inhibited,
    /// Any other answer: its name and its text.
    Other(String),
}

/// Something in the world that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The sessions or the inhibitors may have changed: read them again.
    Login,
    /// The power source changed.
    Power(PowerSource),
    /// The job of a unit ended.
    JobEnded {
        /// The name of the unit.
        unit: String,
        /// Whether the job was done (`done`), not failed or cut.
        done: bool,
    },
    /// logind left the bus.
    LoginGone,
}

/// logind: the sessions, the inhibitors and the actions of power.
pub trait Login {
    /// The sessions, with their state, type, tty and remote host.
    fn sessions(&self) -> impl Future<Output = crate::Result<Vec<Session>>>;
    /// The inhibitors of every user.
    fn inhibitors(&self) -> impl Future<Output = crate::Result<Vec<Inhibitor>>>;
    /// Does an action of logind; logind keeps to the inhibitors.
    fn act(&self, action: LogindAction) -> impl Future<Output = Result<(), Refusal>>;
}

/// UPower: the power source.
pub trait Power {
    /// The power source now.
    fn source(&self) -> impl Future<Output = PowerSource>;
}

/// systemd: the units of the stages.
pub trait Units {
    /// Starts a unit; its end comes as `Event::JobEnded`.
    fn start(&self, unit: &str) -> impl Future<Output = Result<(), String>>;
    /// Stops a unit; its end comes as `Event::JobEnded`.
    fn stop(&self, unit: &str) -> impl Future<Output = Result<(), String>>;
    /// The units of the stages that are not inactive, by their names.
    fn stage_units_up(&self) -> impl Future<Output = Vec<String>>;
}
