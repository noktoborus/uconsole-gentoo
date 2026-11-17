//! drowse-status on the bus: it reads the state of the service and the
//! inhibitors of logind, and follows their changes.

use std::time::Duration;

use futures_util::stream::{self, BoxStream};
use futures_util::{FutureExt, StreamExt};
use rustix::process::getuid;
use zbus::message::Type as MessageType;
use zbus::{Connection, MatchRule, MessageStream, fdo, proxy};

use super::{Observed, ServiceState, show};
use crate::bus::{NAME, SessionRow};
use crate::error::{Error, Result};
use crate::service::{Clock, Monotonic};
use crate::world::Inhibitor;

/// The object of the state of the service.
#[proxy(
    interface = "io.github.noktoborus.Drowse",
    default_service = "io.github.noktoborus.Drowse",
    default_path = "/io/github/noktoborus/Drowse"
)]
trait State {
    /// The state.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<String>;

    /// The action of the next stage and when it comes.
    #[zbus(property)]
    fn scheduled_action(&self) -> zbus::Result<(String, u64)>;

    /// The power source.
    #[zbus(property)]
    fn power_source(&self) -> zbus::Result<String>;
}

/// The object of what holds the system up.
#[proxy(
    interface = "io.github.noktoborus.Drowse",
    default_service = "io.github.noktoborus.Drowse",
    default_path = "/io/github/noktoborus/Drowse/HeldBy"
)]
trait Held {
    /// The sessions that hold the system up.
    #[zbus(property)]
    fn held_by_sessions(&self) -> zbus::Result<Vec<SessionRow>>;

    /// The inhibitors that hold a due entry.
    #[zbus(property)]
    fn held_by_inhibitors(&self) -> zbus::Result<Vec<(String, String, String, String, u32, u32)>>;
}

/// Wraps an error of the bus with what was done.
fn bus(what: &'static str) -> impl FnOnce(zbus::Error) -> Error {
    move |source| Error::Bus { what, source }
}

/// The tool on the system bus.
struct Client {
    conn: Connection,
    state: StateProxy<'static>,
    held: HeldProxy<'static>,
}

impl Client {
    /// Connects to the system bus.
    async fn connect() -> Result<Self> {
        let conn = Connection::system()
            .await
            .map_err(bus("connect to the system bus"))?;
        let state = StateProxy::builder(&conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(bus("make the proxy of the service"))?;
        let held = HeldProxy::builder(&conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(bus("make the proxy of the service"))?;
        Ok(Self { conn, state, held })
    }

    /// Reads all the tool shows. The service not there, or `HeldBy` not
    /// open to this user, is no error.
    async fn observe(&self) -> Result<Observed> {
        let service = self.service().await.ok();
        let sessions = if service.is_some() {
            self.held.held_by_sessions().await.ok()
        } else {
            Some(Vec::new())
        };
        let rows = if service.is_some() {
            self.held.held_by_inhibitors().await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let inhibitors: Vec<Inhibitor> = rows
            .into_iter()
            .map(|(what, who, why, mode, uid, pid)| Inhibitor {
                what,
                who,
                why,
                mode,
                uid,
                pid,
            })
            .collect();
        Ok(Observed {
            service,
            sessions,
            inhibitors,
            uid: getuid().as_raw(),
        })
    }

    /// The state of the service.
    async fn service(&self) -> zbus::Result<ServiceState> {
        Ok(ServiceState {
            state: self.state.state().await?,
            scheduled: self.state.scheduled_action().await?,
            power: self.state.power_source().await?,
        })
    }

    /// Every change to read again for: a signal of logind, a signal of the
    /// service, the service coming or going on the bus.
    async fn changes(&self) -> Result<BoxStream<'static, ()>> {
        let logind = signals_of(&self.conn, "org.freedesktop.login1").await?;
        let service = signals_of(&self.conn, NAME).await?;
        let names = fdo::DBusProxy::new(&self.conn)
            .await
            .map_err(bus("make the proxy of the bus"))?
            .receive_name_owner_changed()
            .await
            .map_err(bus("follow the names on the bus"))?
            .filter(|signal| {
                let ours = signal.args().is_ok_and(|args| args.name().as_str() == NAME);
                std::future::ready(ours)
            });
        Ok(stream::select_all([
            logind.map(drop).boxed(),
            service.map(drop).boxed(),
            names.map(drop).boxed(),
        ])
        .boxed())
    }
}

/// The signals sent by a name on the bus.
async fn signals_of(conn: &Connection, sender: &'static str) -> Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(sender)
        .build()
        .map_err(bus("match the signals"))?;
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(bus("follow the signals"))
}

/// The name of a user, from `/etc/passwd`; its uid when not found.
pub fn user_name(uid: u32) -> String {
    let text = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(':');
            let name = fields.next()?;
            let id = fields.nth(1)?;
            (id.parse() == Ok(uid)).then(|| name.to_owned())
        })
        .next()
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
    let mut last: Option<String> = None;
    loop {
        let observed = client.observe().await?;
        let shown = show(&observed, Monotonic.now(), &user_name);
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
                if change.is_none() {
                    return Ok(());
                }
                tokio::time::sleep(SETTLE).await;
            }
            () = tick => {}
        }
    }
}

/// A short wait after a change, so a burst of signals gives one line.
const SETTLE: Duration = Duration::from_millis(50);
