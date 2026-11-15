//! The real world: the system bus, logind, UPower, systemd and the monitor
//! of sd-login. `connect` gives the world and the stream of its changes.

use futures_util::StreamExt;
use tokio::sync::mpsc;
use zbus::message::Type as MessageType;
use zbus::{Connection, MatchRule, MessageStream, fdo};

use super::logind::{ManagerProxy as LogindProxy, SessionProxy};
use super::systemd::ManagerProxy as SystemdProxy;
use super::upower::UPowerProxy;
use super::{Event, Inhibitor, Login, Power, PowerSource, Refusal, Session, SessionState, Units};
use crate::error::{Error, Result};
use crate::login::SessionMonitor;
use crate::settings::LogindAction;

/// The name of logind on the bus.
const LOGIND: &str = "org.freedesktop.login1";
/// The name of UPower on the bus.
const UPOWER: &str = "org.freedesktop.UPower";
/// The error of logind when an inhibitor holds the action.
const BLOCKED: &str = "org.freedesktop.login1.BlockedByInhibitorLock";
/// The flag of logind: root keeps to the inhibitors of mode `block-weak`.
const ROOT_CHECK_INHIBITORS: u64 = 1;
/// The flag of logind: `reboot` by kexec.
const REBOOT_VIA_KEXEC: u64 = 1 << 1;
/// The flag of logind: `reboot` as a soft reboot.
const SOFT_REBOOT: u64 = 1 << 2;
/// The start of the names of the units of the stages.
pub const STAGE_UNIT_PREFIX: &str = "drowsed-stage@";
/// The pattern of the units of the stages.
const STAGE_UNIT_PATTERN: &str = "drowsed-stage@*.service";

/// The world on the system bus.
pub struct System {
    conn: Connection,
    logind: LogindProxy<'static>,
    upower: UPowerProxy<'static>,
    systemd: SystemdProxy<'static>,
}

/// Wraps an error of the bus with what was done.
fn bus(what: &'static str) -> impl FnOnce(zbus::Error) -> Error {
    move |source| Error::Bus { what, source }
}

/// Connects to the system bus and starts to follow the world. Every change
/// comes on the receiver.
pub async fn connect() -> Result<(System, mpsc::UnboundedReceiver<Event>)> {
    let conn = Connection::system()
        .await
        .map_err(bus("connect to the system bus"))?;
    let logind = LogindProxy::new(&conn)
        .await
        .map_err(bus("make the proxy of logind"))?;
    let upower = UPowerProxy::builder(&conn)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(bus("make the proxy of UPower"))?;
    let systemd = SystemdProxy::new(&conn)
        .await
        .map_err(bus("make the proxy of systemd"))?;
    systemd
        .subscribe()
        .await
        .map_err(bus("subscribe to the jobs of systemd"))?;
    let monitor = SessionMonitor::new().map_err(|source| Error::SessionMonitor { source })?;
    let (send, receive) = mpsc::unbounded_channel();
    follow_logind(&conn, send.clone()).await?;
    follow_names(&conn, upower.clone(), send.clone()).await?;
    follow_upower(&conn, upower.clone(), send.clone()).await?;
    follow_jobs(&systemd, send.clone()).await?;
    follow_monitor(monitor, send);
    let system = System {
        conn,
        logind,
        upower,
        systemd,
    };
    Ok((system, receive))
}

/// Any signal of logind: the sessions or the inhibitors may have changed.
async fn follow_logind(conn: &Connection, send: mpsc::UnboundedSender<Event>) -> Result<()> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(LOGIND)
        .build()
        .map_err(bus("match the signals of logind"))?;
    let mut stream = MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(bus("follow the signals of logind"))?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if send.send(Event::Login).is_err() {
                return;
            }
        }
    });
    Ok(())
}

/// logind leaving the bus, and UPower coming or going.
async fn follow_names(
    conn: &Connection,
    upower: UPowerProxy<'static>,
    send: mpsc::UnboundedSender<Event>,
) -> Result<()> {
    let dbus = fdo::DBusProxy::new(conn)
        .await
        .map_err(bus("make the proxy of the bus"))?;
    let mut stream = dbus
        .receive_name_owner_changed()
        .await
        .map_err(bus("follow the names on the bus"))?;
    tokio::spawn(async move {
        while let Some(signal) = stream.next().await {
            let Ok(args) = signal.args() else {
                continue;
            };
            let event = match args.name().as_str() {
                LOGIND if args.new_owner().is_none() => Event::LoginGone,
                UPOWER => Event::Power(read_power(&upower).await),
                _ => continue,
            };
            if send.send(event).is_err() {
                return;
            }
        }
    });
    Ok(())
}

/// The signals of UPower: the power source may have changed.
async fn follow_upower(
    conn: &Connection,
    upower: UPowerProxy<'static>,
    send: mpsc::UnboundedSender<Event>,
) -> Result<()> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(UPOWER)
        .path("/org/freedesktop/UPower")
        .build()
        .map_err(bus("match the signals of UPower"))?;
    let mut stream = MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(bus("follow the signals of UPower"))?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if send.send(Event::Power(read_power(&upower).await)).is_err() {
                return;
            }
        }
    });
    Ok(())
}

/// The end of the jobs of the units of the stages.
async fn follow_jobs(
    systemd: &SystemdProxy<'static>,
    send: mpsc::UnboundedSender<Event>,
) -> Result<()> {
    let mut stream = systemd
        .receive_job_removed()
        .await
        .map_err(bus("follow the jobs of systemd"))?;
    tokio::spawn(async move {
        while let Some(signal) = stream.next().await {
            let Ok(args) = signal.args() else {
                continue;
            };
            if !args.unit().starts_with(STAGE_UNIT_PREFIX) {
                continue;
            }
            let event = Event::JobEnded {
                unit: args.unit().clone(),
                done: args.result() == "done",
            };
            if send.send(event).is_err() {
                return;
            }
        }
    });
    Ok(())
}

/// The monitor of sd-login: a session may have gone `closing`.
fn follow_monitor(mut monitor: SessionMonitor, send: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        while monitor.changed().await.is_ok() {
            if send.send(Event::Login).is_err() {
                return;
            }
        }
    });
}

/// Reads the power source; no answer of UPower is `Unknown`.
async fn read_power(upower: &UPowerProxy<'static>) -> PowerSource {
    match upower.on_battery().await {
        Ok(true) => PowerSource::Battery,
        Ok(false) => PowerSource::Ac,
        Err(_) => PowerSource::Unknown,
    }
}

impl System {
    /// The connection to the system bus.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Reads the properties of a session that `ListSessionsEx()` lacks.
    async fn session_details(
        &self,
        path: &zbus::OwnedObjectPath,
    ) -> zbus::Result<(String, String, String, String)> {
        let session: SessionProxy<'_> = SessionProxy::builder(&self.conn)
            .path(path.clone())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        Ok((
            session.state().await?,
            session.kind().await?,
            session.tty().await?,
            session.remote_host().await?,
        ))
    }
}

impl Login for System {
    async fn sessions(&self) -> Result<Vec<Session>> {
        let rows = self
            .logind
            .list_sessions_ex()
            .await
            .map_err(bus("list the sessions of logind"))?;
        let mut sessions = Vec::with_capacity(rows.len());
        for (id, uid, user, _seat, _leader, class, tty, idle, idle_since, path) in rows {
            let Ok((state, kind, _tty, remote_host)) = self.session_details(&path).await else {
                continue;
            };
            sessions.push(Session {
                id,
                uid,
                user,
                class,
                kind,
                tty,
                remote_host,
                idle,
                idle_since,
                state: SessionState::from_name(&state),
            });
        }
        Ok(sessions)
    }

    async fn inhibitors(&self) -> Result<Vec<Inhibitor>> {
        let rows = self
            .logind
            .list_inhibitors()
            .await
            .map_err(bus("list the inhibitors of logind"))?;
        Ok(rows
            .into_iter()
            .map(|(what, who, why, mode, uid, pid)| Inhibitor {
                what,
                who,
                why,
                mode,
                uid,
                pid,
            })
            .collect())
    }

    async fn act(&self, action: LogindAction) -> std::result::Result<(), Refusal> {
        let flags = ROOT_CHECK_INHIBITORS;
        let logind = &self.logind;
        let done = match action {
            LogindAction::Poweroff => logind.power_off_with_flags(flags).await,
            LogindAction::Halt => logind.halt_with_flags(flags).await,
            LogindAction::Reboot => logind.reboot_with_flags(flags).await,
            LogindAction::Kexec => logind.reboot_with_flags(flags | REBOOT_VIA_KEXEC).await,
            LogindAction::SoftReboot => logind.reboot_with_flags(flags | SOFT_REBOOT).await,
            LogindAction::Suspend => logind.suspend_with_flags(flags).await,
            LogindAction::Hibernate => logind.hibernate_with_flags(flags).await,
            LogindAction::HybridSleep => logind.hybrid_sleep_with_flags(flags).await,
            LogindAction::SuspendThenHibernate => {
                logind.suspend_then_hibernate_with_flags(flags).await
            }
            LogindAction::Sleep => logind.sleep_with_flags(flags).await,
            LogindAction::Lock => logind.lock_sessions().await,
            LogindAction::Ignore => Ok(()),
        };
        done.map_err(refusal)
    }
}

/// What a refusal of logind says.
fn refusal(error: zbus::Error) -> Refusal {
    match error {
        zbus::Error::MethodError(name, _, _) if name.as_str() == BLOCKED => Refusal::Inhibited,
        zbus::Error::MethodError(name, text, _) => {
            Refusal::Other(format!("{}: {}", name.as_str(), text.unwrap_or_default()))
        }
        other => Refusal::Other(other.to_string()),
    }
}

impl Power for System {
    async fn source(&self) -> PowerSource {
        read_power(&self.upower).await
    }
}

impl Units for System {
    async fn start(&self, unit: &str) -> std::result::Result<(), String> {
        self.systemd
            .start_unit(unit, "replace")
            .await
            .map(drop)
            .map_err(|error| error.to_string())
    }

    async fn stop(&self, unit: &str) -> std::result::Result<(), String> {
        self.systemd
            .stop_unit(unit, "replace")
            .await
            .map(drop)
            .map_err(|error| error.to_string())
    }

    async fn stage_units_up(&self) -> Vec<String> {
        let states = [
            "active",
            "activating",
            "deactivating",
            "reloading",
            "failed",
        ];
        self.systemd
            .list_units_by_patterns(&states, &[STAGE_UNIT_PATTERN])
            .await
            .map(|rows| rows.into_iter().map(|row| row.0).collect())
            .unwrap_or_default()
    }
}
