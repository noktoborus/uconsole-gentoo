//! The proxies of logind on the system bus.

use zbus::{OwnedObjectPath, proxy};

/// A session of `ListSessionsEx()`: id, uid, user, seat, leader, class,
/// tty, idle hint, idle since (monotonic), object path.
pub type SessionEx = (
    String,
    u32,
    String,
    String,
    u32,
    String,
    String,
    bool,
    u64,
    OwnedObjectPath,
);

/// An inhibitor of `ListInhibitors()`: what, who, why, mode, uid, pid.
pub type InhibitorRow = (String, String, String, String, u32, u32);

/// The manager of logind.
#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait Manager {
    /// Every session.
    fn list_sessions_ex(&self) -> zbus::Result<Vec<SessionEx>>;

    /// Takes an inhibitor: what, who, why, mode. It holds while the fd
    /// is open. polkit may ask a password of an agent.
    #[zbus(allow_interactive_auth)]
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<zbus::OwnedFd>;

    /// Every inhibitor of every user.
    fn list_inhibitors(&self) -> zbus::Result<Vec<InhibitorRow>>;

    /// Whether the lid is closed; logind sends its changes.
    #[zbus(property)]
    fn lid_closed(&self) -> zbus::Result<bool>;

    /// `poweroff`.
    fn power_off_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `reboot`; with a flag, `kexec` or `soft-reboot`.
    fn reboot_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `halt`.
    fn halt_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `suspend`.
    fn suspend_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `hibernate`.
    fn hibernate_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `hybrid-sleep`.
    fn hybrid_sleep_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `suspend-then-hibernate`.
    fn suspend_then_hibernate_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `sleep`: the sleep logind picks.
    #[zbus(name = "Sleep")]
    fn sleep_with_flags(&self, flags: u64) -> zbus::Result<()>;

    /// `lock`: every session locks its screen.
    fn lock_sessions(&self) -> zbus::Result<()>;
}

/// A session of logind, for what `ListSessionsEx()` lacks.
#[proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
pub trait Session {
    /// `online`, `active` or `closing`.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<String>;

    /// `wayland`, `tty`, …
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;

    /// The tty, or empty.
    #[zbus(property, name = "TTY")]
    fn tty(&self) -> zbus::Result<String>;

    /// The remote host, or empty.
    #[zbus(property)]
    fn remote_host(&self) -> zbus::Result<String>;
}
