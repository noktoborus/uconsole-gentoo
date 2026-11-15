//! The state of the service on the system bus: two objects, for reading
//! only. Anyone reads the state; only `wheel` reads what holds the system
//! up, by the policy of D-Bus of the package. One change sends one
//! `PropertiesChanged` an object.

use std::borrow::Cow;
use std::collections::HashMap;

use zbus::object_server::SignalEmitter;
use zbus::{Connection, Variant, interface};

use crate::engine::View;
use crate::error::{Error, Result};
use crate::world::{Inhibitor, PowerSource, Session};

/// The name of the service on the bus, and of its interface.
pub const NAME: &str = "io.github.noktoborus.Drowse";
/// The object of the state.
pub const PATH: &str = "/io/github/noktoborus/Drowse";
/// The object of what holds the system up.
pub const HELD_PATH: &str = "/io/github/noktoborus/Drowse/HeldBy";

/// A session on the bus: id, uid, user, type, tty, remote host.
pub type SessionRow = (String, u32, String, String, String, String);
/// An inhibitor on the bus: what, who, why, mode, uid, pid.
pub type InhibitorRow = (String, String, String, String, u32, u32);

/// The object of the state: `State`, `ScheduledAction`, `PowerSource`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateObject {
    state: String,
    scheduled: (String, u64),
    power: String,
}

#[interface(name = "io.github.noktoborus.Drowse")]
impl StateObject {
    /// `active`, `idle` or `announced`.
    #[zbus(property)]
    fn state(&self) -> String {
        self.state.clone()
    }

    /// The action of the next stage and when it comes, in microseconds of
    /// `CLOCK_MONOTONIC`; `("", 0)` when none comes.
    #[zbus(property)]
    fn scheduled_action(&self) -> (String, u64) {
        self.scheduled.clone()
    }

    /// `battery`, `ac`, or `unknown` when UPower does not answer.
    #[zbus(property)]
    fn power_source(&self) -> String {
        self.power.clone()
    }
}

/// The object of what holds the system up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldObject {
    sessions: Vec<SessionRow>,
    inhibitors: Vec<InhibitorRow>,
}

#[interface(name = "io.github.noktoborus.Drowse")]
impl HeldObject {
    /// The sessions that hold the system up.
    #[zbus(property)]
    fn held_by_sessions(&self) -> Vec<SessionRow> {
        self.sessions.clone()
    }

    /// The inhibitors that hold the system up.
    #[zbus(property)]
    fn held_by_inhibitors(&self) -> Vec<InhibitorRow> {
        self.inhibitors.clone()
    }
}

impl From<&View> for StateObject {
    fn from(view: &View) -> Self {
        Self {
            state: view.state.name().to_owned(),
            scheduled: view.scheduled.clone(),
            power: power_name(view.power).to_owned(),
        }
    }
}

impl From<&View> for HeldObject {
    fn from(view: &View) -> Self {
        Self {
            sessions: view.sessions.iter().map(session_row).collect(),
            inhibitors: view.inhibitors.iter().map(inhibitor_row).collect(),
        }
    }
}

/// A session as the bus gives it.
fn session_row(session: &Session) -> SessionRow {
    (
        session.id.clone(),
        session.uid,
        session.user.clone(),
        session.kind.clone(),
        session.tty.clone(),
        session.remote_host.clone(),
    )
}

/// An inhibitor as the bus gives it.
fn inhibitor_row(inhibitor: &Inhibitor) -> InhibitorRow {
    (
        inhibitor.what.clone(),
        inhibitor.who.clone(),
        inhibitor.why.clone(),
        inhibitor.mode.clone(),
        inhibitor.uid,
        inhibitor.pid,
    )
}

/// The name of a power source on the bus.
fn power_name(power: PowerSource) -> &'static str {
    match power {
        PowerSource::Battery => "battery",
        PowerSource::Ac => "ac",
        PowerSource::Unknown => "unknown",
    }
}

/// Wraps an error of the bus with what was done.
fn bus(what: &'static str) -> impl FnOnce(zbus::Error) -> Error {
    move |source| Error::Bus { what, source }
}

/// Puts the two objects on a connection with the first view.
pub async fn serve(conn: &Connection, view: &View) -> Result<()> {
    let server = conn.object_server();
    server
        .at(PATH, StateObject::from(view))
        .await
        .map_err(bus("put the object of the state on the bus"))?;
    server
        .at(HELD_PATH, HeldObject::from(view))
        .await
        .map_err(bus("put the object of what holds on the bus"))?;
    Ok(())
}

/// Takes the name of the service on the bus.
pub async fn take_name(conn: &Connection) -> Result<()> {
    conn.request_name(NAME)
        .await
        .map_err(bus("take the name on the bus"))
}

/// Publishes a new view: each object that changed gets its new values and
/// one `PropertiesChanged` with every property that changed.
pub async fn publish(conn: &Connection, view: &View) -> Result<()> {
    let server = conn.object_server();
    let state = server
        .interface::<_, StateObject>(PATH)
        .await
        .map_err(bus("find the object of the state"))?;
    let new_state = StateObject::from(view);
    let old_state = state.get().await.clone();
    if old_state != new_state {
        let mut changed: HashMap<&str, Variant<'_>> = HashMap::new();
        if old_state.state != new_state.state {
            changed.insert("State", Variant::from(new_state.state.clone()));
        }
        if old_state.scheduled != new_state.scheduled {
            changed.insert(
                "ScheduledAction",
                Variant::from(new_state.scheduled.clone()),
            );
        }
        if old_state.power != new_state.power {
            changed.insert("PowerSource", Variant::from(new_state.power.clone()));
        }
        *state.get_mut().await = new_state;
        emit(state.signal_emitter(), changed).await?;
    }
    let held = server
        .interface::<_, HeldObject>(HELD_PATH)
        .await
        .map_err(bus("find the object of what holds"))?;
    let new_held = HeldObject::from(view);
    let old_held = held.get().await.clone();
    if old_held != new_held {
        let mut changed: HashMap<&str, Variant<'_>> = HashMap::new();
        if old_held.sessions != new_held.sessions {
            changed.insert("HeldBySessions", Variant::from(new_held.sessions.clone()));
        }
        if old_held.inhibitors != new_held.inhibitors {
            changed.insert(
                "HeldByInhibitors",
                Variant::from(new_held.inhibitors.clone()),
            );
        }
        *held.get_mut().await = new_held;
        emit(held.signal_emitter(), changed).await?;
    }
    Ok(())
}

/// Sends one `PropertiesChanged` of the interface of the service.
async fn emit(emitter: &SignalEmitter<'_>, changed: HashMap<&str, Variant<'_>>) -> Result<()> {
    let interface = zbus::names::InterfaceName::from_static_str_unchecked(NAME);
    zbus::fdo::Properties::properties_changed(emitter, interface, changed, Cow::Borrowed(&[]))
        .await
        .map_err(bus("send the change of the properties"))
}

#[cfg(test)]
mod tests;
