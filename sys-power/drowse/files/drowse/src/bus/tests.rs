//! Tests of the objects on a bus peer to peer, with no `dbus-daemon`.

use futures_util::StreamExt;
use std::os::unix::net::UnixStream;
use zbus::connection::Builder;
use zbus::fdo::PropertiesProxy;
use zbus::names::InterfaceName;
use zbus::{Guid, OwnedVariant};

use super::*;
use crate::engine::State;
use crate::world::{Inhibitor, SessionState};

fn view(state: State, power: PowerSource) -> View {
    View {
        state,
        scheduled: ("poweroff".to_owned(), 42),
        sessions: Vec::new(),
        inhibitors: Vec::new(),
        power,
        lid_closed: false,
    }
}

async fn pair() -> (Connection, Connection) {
    let (a, b) = UnixStream::pair().unwrap();
    let guid = Guid::generate();
    let server = Builder::unix_stream(a).server(guid).p2p().build();
    let client = Builder::unix_stream(b).p2p().build();
    let (server, client) = tokio::join!(server, client);
    (server.unwrap(), client.unwrap())
}

async fn properties(client: &Connection, path: &'static str) -> PropertiesProxy<'static> {
    PropertiesProxy::builder(client)
        .destination("io.github.noktoborus.Drowse")
        .path(path)
        .build()
        .await
        .unwrap()
}

fn interface() -> InterfaceName<'static> {
    InterfaceName::from_static_str_unchecked(NAME)
}

#[tokio::test]
async fn the_properties_and_their_signatures() {
    let (server, client) = pair().await;
    let mut first = view(State::Idle, PowerSource::Battery);
    first.sessions.push(Session {
        id: "4".to_owned(),
        uid: 1000,
        user: "alice".to_owned(),
        class: "user".to_owned(),
        kind: "tty".to_owned(),
        tty: String::new(),
        remote_host: "192.168.1.5".to_owned(),
        idle: false,
        idle_since: 0,
        state: SessionState::Active,
    });
    serve(&server, &first).await.unwrap();
    let state = properties(&client, PATH).await;
    let all = state.get_all(interface()).await.unwrap();
    assert_eq!(all["State"].value_signature().to_string(), "s");
    assert_eq!(all["ScheduledAction"].value_signature().to_string(), "(st)");
    assert_eq!(all["PowerSource"].value_signature().to_string(), "s");
    assert_eq!(String::try_from(all["Lid"].clone()).unwrap(), "open");
    assert_eq!(String::try_from(all["State"].clone()).unwrap(), "idle");
    assert_eq!(
        String::try_from(all["PowerSource"].clone()).unwrap(),
        "battery"
    );
    let held = properties(&client, HELD_PATH).await;
    let all = held.get_all(interface()).await.unwrap();
    assert_eq!(
        all["HeldBySessions"].value_signature().to_string(),
        "a(sussss)"
    );
    assert_eq!(
        all["HeldByInhibitors"].value_signature().to_string(),
        "a(ssssuu)"
    );
}

#[tokio::test]
async fn held_state_and_inhibitors_are_published() {
    let (server, client) = pair().await;
    let mut held_view = view(State::Held, PowerSource::Battery);
    held_view.scheduled = ("power save".to_owned(), 0);
    held_view.inhibitors.push(Inhibitor {
        what: "idle".to_owned(),
        who: "drowse-inhibit".to_owned(),
        why: "build".to_owned(),
        mode: "block-weak".to_owned(),
        uid: 1000,
        pid: 42,
    });
    serve(&server, &held_view).await.unwrap();
    let state = properties(&client, PATH).await;
    let all = state.get_all(interface()).await.unwrap();
    assert_eq!(String::try_from(all["State"].clone()).unwrap(), "held");
    let held = properties(&client, HELD_PATH).await;
    let all = held.get_all(interface()).await.unwrap();
    assert_eq!(
        Vec::<InhibitorRow>::try_from(all["HeldByInhibitors"].clone())
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn one_change_sends_one_signal_an_object() {
    let (server, client) = pair().await;
    serve(&server, &view(State::Idle, PowerSource::Battery))
        .await
        .unwrap();
    let state = properties(&client, PATH).await;
    let mut changes = state.receive_properties_changed().await.unwrap();
    let mut next = view(State::Announced, PowerSource::Ac);
    next.scheduled = ("poweroff".to_owned(), 99);
    next.lid_closed = true;
    publish(&server, &next).await.unwrap();
    let signal = changes.next().await.unwrap();
    let args = signal.args().unwrap();
    let mut names: Vec<&str> = args.changed_properties().keys().copied().collect();
    names.sort_unstable();
    assert_eq!(names, ["Lid", "PowerSource", "ScheduledAction", "State"]);
    let state_now: OwnedVariant = args.changed_properties()["State"].try_to_owned().unwrap();
    assert_eq!(String::try_from(state_now).unwrap(), "announced");
}

#[tokio::test]
async fn no_change_sends_nothing() {
    let (server, _client) = pair().await;
    let first = view(State::Idle, PowerSource::Battery);
    serve(&server, &first).await.unwrap();
    publish(&server, &first).await.unwrap();
}
