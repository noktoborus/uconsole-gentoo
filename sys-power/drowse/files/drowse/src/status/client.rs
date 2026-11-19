//! drowse-status on the bus. It reads the state of the service and the
//! inhibitors of logind once; then it keeps the values that the signals of
//! the service carry. It reads again only what a signal does not carry:
//! the service that came back on the bus, a property invalidated, the
//! inhibitors of logind changed.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::stream::{self, BoxStream};
use futures_util::{FutureExt, StreamExt};
use rustix::process::getuid;
use zbus::names::InterfaceName;
use zbus::{Connection, Message, OwnedVariant, fdo};

use super::{Observed, ServiceState, show};
use crate::bus::{HELD_PATH, InhibitorRow, NAME, PATH, SessionRow};
use crate::changes::{self, Changed};
use crate::error::{Error, Result};
use crate::service::{Clock, Monotonic};
use crate::world::Inhibitor;
use crate::world::logind::ManagerProxy;

/// The name of logind on the bus.
const LOGIND: &str = "org.freedesktop.login1";
/// The path of the manager of logind.
const LOGIND_PATH: &str = "/org/freedesktop/login1";
/// The interface of the manager of logind.
const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";
/// The properties of the manager of logind sent at each inhibitor of mode
/// `block` or `block-weak` taken or let go (notes/logind-interface.md).
const BLOCK_PROPERTIES: [&str; 2] = ["BlockInhibited", "BlockWeakInhibited"];

/// The properties of one object, by name.
type Properties = HashMap<String, OwnedVariant>;

/// Wraps an error of the bus with what was done.
fn bus(what: &'static str) -> impl FnOnce(zbus::Error) -> Error {
    move |source| Error::Bus { what, source }
}

/// A change the tool follows.
enum Change {
    /// A signal `PropertiesChanged` of the service.
    Service(Message),
    /// The service came on the bus (true) or left it (false).
    Owner(bool),
    /// An inhibitor of logind of mode `block` or `block-weak` was taken or
    /// let go.
    Inhibitors,
}

/// What the tool keeps between two changes.
#[derive(Default)]
struct Kept {
    /// The properties of the state; none when the service is not there.
    state: Option<Properties>,
    /// The properties of what holds the system up; none when the service
    /// is not there or this user may not read them.
    held: Option<Properties>,
    /// The inhibitors of logind that hold the system up.
    inhibitors: Vec<Inhibitor>,
}

impl Kept {
    /// What to show, as the tool read it.
    fn observed(&self) -> Observed {
        let service = self.state.as_ref().and_then(service_state);
        let (sessions, held_inhibitors) = match (&service, &self.held) {
            (None, _) => (Some(Vec::new()), Vec::new()),
            (Some(_), None) => (None, Vec::new()),
            (Some(_), Some(held)) => (
                value::<Vec<SessionRow>>(held, "HeldBySessions"),
                value::<Vec<InhibitorRow>>(held, "HeldByInhibitors")
                    .unwrap_or_default()
                    .into_iter()
                    .map(inhibitor_of)
                    .collect(),
            ),
        };
        Observed {
            service,
            sessions,
            inhibitors: self.inhibitors.clone(),
            held_inhibitors,
            uid: getuid().as_raw(),
        }
    }

    /// The properties of the object at this path.
    fn object(&mut self, path: &str) -> Option<&mut Option<Properties>> {
        match path {
            PATH => Some(&mut self.state),
            HELD_PATH => Some(&mut self.held),
            _ => None,
        }
    }
}

/// One property, of the type it has on the bus.
fn value<T: TryFrom<OwnedVariant>>(properties: &Properties, name: &str) -> Option<T> {
    T::try_from(properties.get(name)?.try_clone().ok()?).ok()
}

/// The state of the service, from the properties of its object.
fn service_state(properties: &Properties) -> Option<ServiceState> {
    Some(ServiceState {
        state: value(properties, "State")?,
        scheduled: value(properties, "ScheduledAction")?,
        next_held: value(properties, "NextHeld")?,
        power: value(properties, "PowerSource")?,
        lid: value(properties, "Lid")?,
    })
}

/// An inhibitor, from a row of logind or of the service.
fn inhibitor_of((what, who, why, mode, uid, pid): InhibitorRow) -> Inhibitor {
    Inhibitor {
        what,
        who,
        why,
        mode,
        uid,
        pid,
    }
}

/// The connection and the proxies of the tool.
struct Client {
    conn: Connection,
    logind: ManagerProxy<'static>,
}

impl Client {
    async fn connect() -> Result<Self> {
        let conn = Connection::system()
            .await
            .map_err(bus("connect to the system bus"))?;
        let logind = ManagerProxy::builder(&conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(bus("make the proxy of logind"))?;
        Ok(Self { conn, logind })
    }

    /// Every property of an object of the service; none when the service is
    /// not there or this user may not read it.
    async fn properties(&self, path: &'static str) -> Option<Properties> {
        let proxy = fdo::PropertiesProxy::builder(&self.conn)
            .destination(NAME)
            .path(path)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .ok()?;
        proxy
            .get_all(InterfaceName::from_static_str_unchecked(NAME))
            .await
            .ok()
    }

    /// The inhibitors of logind of mode `block` or `block-weak` that hold a
    /// kind drowse looks at.
    async fn inhibitors(&self) -> Result<Vec<Inhibitor>> {
        let rows = self
            .logind
            .list_inhibitors()
            .await
            .map_err(bus("list the inhibitors of logind"))?;
        Ok(rows
            .into_iter()
            .map(inhibitor_of)
            .filter(|inhibitor| {
                matches!(inhibitor.mode.as_str(), "block" | "block-weak")
                    && !inhibitor.holds().is_empty()
            })
            .collect())
    }

    /// Reads all the tool shows. The service not there, or `HeldBy` not
    /// open to this user, is no error.
    async fn read(&self) -> Result<Kept> {
        Ok(Kept {
            state: self.properties(PATH).await,
            held: self.properties(HELD_PATH).await,
            inhibitors: self.inhibitors().await?,
        })
    }

    /// Takes a change into what the tool keeps.
    async fn apply(&self, kept: &mut Kept, change: Change) -> Result<()> {
        match change {
            Change::Service(message) => self.apply_service(kept, &message).await,
            Change::Owner(true) => {
                kept.state = self.properties(PATH).await;
                kept.held = self.properties(HELD_PATH).await;
            }
            Change::Owner(false) => {
                kept.state = None;
                kept.held = None;
            }
            Change::Inhibitors => kept.inhibitors = self.inhibitors().await?,
        }
        Ok(())
    }

    /// Takes the values of a signal of the service; reads the object again
    /// when the signal does not carry them all.
    async fn apply_service(&self, kept: &mut Kept, message: &Message) {
        let header = message.header();
        let Some(path) = header.path().map(|path| path.as_str().to_owned()) else {
            return;
        };
        let Some(path) = [PATH, HELD_PATH].into_iter().find(|known| *known == path) else {
            return;
        };
        let changed = changes::read(message).unwrap_or_default();
        let Some(object) = kept.object(path) else {
            return;
        };
        match object {
            Some(properties) if changed.invalidated.is_empty() => {
                properties.extend(changed.values);
            }
            _ => *object = self.properties(path).await,
        }
    }

    /// Every change the tool follows. The bus sends only these: the changes
    /// of the service, its name coming or going, and the changes of the
    /// manager of logind, of which only those of its inhibitors count.
    async fn changes(&self) -> Result<BoxStream<'static, Change>> {
        let service = changes::follow(&self.conn, NAME, None, NAME).await?;
        let logind = changes::follow(&self.conn, LOGIND, Some(LOGIND_PATH), LOGIND_MANAGER).await?;
        let names = fdo::DBusProxy::new(&self.conn)
            .await
            .map_err(bus("make the proxy of the bus"))?
            .receive_name_owner_changed_with_args(&[(0, NAME)])
            .await
            .map_err(bus("follow the name of the service"))?;
        Ok(stream::select_all([
            service
                .filter_map(|message| std::future::ready(message.ok().map(Change::Service)))
                .boxed(),
            logind
                .filter_map(|message| {
                    let ours = message
                        .ok()
                        .and_then(|message| changes::read(&message))
                        .is_some_and(|changed| block_changed(&changed));
                    std::future::ready(ours.then_some(Change::Inhibitors))
                })
                .boxed(),
            names
                .filter_map(|signal| {
                    let owner = signal.args().ok().map(|args| args.new_owner().is_some());
                    std::future::ready(owner.map(Change::Owner))
                })
                .boxed(),
        ])
        .boxed())
    }
}

/// Whether a change of the manager of logind is one of its inhibitors.
fn block_changed(changed: &Changed) -> bool {
    changed.names().any(|name| BLOCK_PROPERTIES.contains(&name))
}

/// The name of a user, by the databases of the system (NSS: `/etc/passwd`,
/// systemd-homed, LDAP); its uid when not found.
pub fn user_name(uid: u32) -> String {
    uzers::get_user_by_uid(uid)
        .map(|user| user.name().to_string_lossy().into_owned())
        .unwrap_or_else(|| uid.to_string())
}

/// How the tool shows the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Text for a terminal.
    Plain,
    /// One line of JSON for waybar.
    Waybar,
}

/// Shows the state once, or, with `follow`, at each change and as the
/// time left runs.
pub async fn run(form: Form, follow: bool) -> Result<()> {
    let client = Client::connect().await?;
    let mut changes = if follow {
        Some(client.changes().await?)
    } else {
        None
    };
    let mut kept = client.read().await?;
    let mut last: Option<String> = None;
    loop {
        let shown = show(&kept.observed(), Monotonic.now(), &user_name);
        let line = match form {
            Form::Plain => shown.plain(),
            Form::Waybar => shown.waybar(),
        };
        if last.as_ref() != Some(&line) {
            println!("{line}");
            last = Some(line);
        }
        let Some(changes) = changes.as_mut() else {
            return Ok(());
        };
        let tick = match shown.again_in {
            Some(wait) => tokio::time::sleep(wait).boxed(),
            None => std::future::pending().boxed(),
        };
        tokio::select! {
            change = changes.next() => {
                let Some(change) = change else {
                    return Ok(());
                };
                client.apply(&mut kept, change).await?;
                settle(&client, &mut kept, changes).await?;
            }
            () = tick => {}
        }
    }
}

/// Takes the changes that follow within `SETTLE`, so a burst of signals
/// gives one line.
async fn settle(
    client: &Client,
    kept: &mut Kept,
    changes: &mut BoxStream<'static, Change>,
) -> Result<()> {
    let deadline = tokio::time::sleep(SETTLE);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            change = changes.next() => match change {
                Some(change) => client.apply(kept, change).await?,
                None => return Ok(()),
            },
            () = &mut deadline => return Ok(()),
        }
    }
}

/// A short wait after a change, so a burst of signals gives one line.
const SETTLE: Duration = Duration::from_millis(50);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_is_named_by_the_system_and_an_unknown_uid_by_its_number() {
        assert_eq!(user_name(0), "root");
        assert_eq!(user_name(4_000_000_000), "4000000000");
    }
}
