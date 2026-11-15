//! The proxy of the manager of systemd on the system bus.

use zbus::{OwnedObjectPath, proxy};

/// A unit of `ListUnitsByPatterns()`: name, description, load state,
/// active state, sub state, followed, path, job id, job type, job path.
pub type UnitRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);

/// The manager of systemd.
#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Manager {
    /// Asks for the signals of jobs.
    fn subscribe(&self) -> zbus::Result<()>;

    /// Starts a unit.
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// Stops a unit.
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// The units of these states and names.
    fn list_units_by_patterns(
        &self,
        states: &[&str],
        patterns: &[&str],
    ) -> zbus::Result<Vec<UnitRow>>;

    /// A job ended: its id, its path, the unit, the result.
    #[zbus(signal)]
    fn job_removed(
        &self,
        id: u32,
        job: OwnedObjectPath,
        unit: String,
        result: String,
    ) -> zbus::Result<()>;
}
