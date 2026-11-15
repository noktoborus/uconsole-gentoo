//! The proxy of UPower on the system bus.

use zbus::proxy;

/// The daemon of UPower.
#[proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
pub trait UPower {
    /// Whether the system runs on battery.
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}
