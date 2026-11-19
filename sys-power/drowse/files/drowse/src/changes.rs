//! The signals `PropertiesChanged`: a stream of those of one object, and
//! the properties a signal changed or invalidated.

use std::collections::HashMap;

use zbus::OwnedVariant;
use zbus::message::Type as MessageType;
use zbus::{Connection, MatchRule, Message, MessageStream, fdo};

use crate::error::{Error, Result};

/// The interface of the signal.
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// What one signal says of the properties of an interface.
#[derive(Debug, Default)]
pub struct Changed {
    /// The properties with their new values.
    pub values: HashMap<String, OwnedVariant>,
    /// The properties whose new values must be read.
    pub invalidated: Vec<String>,
}

impl Changed {
    /// The names of the properties it tells of, changed or invalidated.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values
            .keys()
            .chain(self.invalidated.iter())
            .map(String::as_str)
    }
}

/// The signals `PropertiesChanged` of the interface `interface` sent by
/// `sender`, of the object `path` or, with none, of every object of it.
/// The bus sends no other signal to this stream.
pub async fn follow(
    conn: &Connection,
    sender: &'static str,
    path: Option<&'static str>,
    interface: &'static str,
) -> Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(sender)
        .interface(PROPERTIES)
        .member("PropertiesChanged")
        .arg(0, interface);
    let rule = match path {
        Some(path) => rule.path(path),
        None => rule,
    };
    let rule = rule.build().map_err(|source| Error::Bus {
        what: "match the changes of properties",
        source,
    })?;
    MessageStream::for_match_rule(rule, conn, None)
        .await
        .map_err(|source| Error::Bus {
            what: "follow the changes of properties",
            source,
        })
}

/// What a signal `PropertiesChanged` says; none for another message or a
/// body that does not read.
pub fn read(message: &Message) -> Option<Changed> {
    let signal = fdo::PropertiesChanged::from_message(message.clone())?;
    let args = signal.args().ok()?;
    let values = args
        .changed_properties()
        .iter()
        .filter_map(|(name, value)| Some(((*name).to_owned(), value.try_to_owned().ok()?)))
        .collect();
    let invalidated = args
        .invalidated_properties()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    Some(Changed {
        values,
        invalidated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_the_changed_then_the_invalidated() {
        let changed = Changed {
            values: HashMap::from([("IdleHint".to_owned(), OwnedVariant::from(true))]),
            invalidated: vec!["LidClosed".to_owned()],
        };
        assert_eq!(
            changed.names().collect::<Vec<_>>(),
            ["IdleHint", "LidClosed"]
        );
    }
}
