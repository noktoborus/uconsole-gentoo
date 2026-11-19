//! A script over the driver: the world on a mock, the clock of tokio
//! paused, the times of the test in milliseconds from its start. The
//! script changes the world at set times and reads what the service did.

use std::cell::RefCell;
use std::future::Future;

use tokio::time::{Instant, sleep_until};

use super::*;

/// The world, what the service published, and the time of the start.
pub(super) struct Script {
    world: MockWorld,
    events: RefCell<Option<UnboundedReceiver<Event>>>,
    publisher: MockPublisher,
    start: Instant,
}

impl Script {
    /// A script on one idle session and this power source, from now.
    pub(super) fn new(power: PowerSource) -> Self {
        let (world, events) = world(power);
        Self {
            world,
            events: RefCell::new(Some(events)),
            publisher: MockPublisher::default(),
            start: Instant::now(),
        }
    }

    /// Runs the service with these settings while the script runs.
    pub(super) async fn run(&self, chain: &str, script: impl Future<Output = ()>) {
        let mut events = self.events.borrow_mut().take().unwrap();
        let clock = TokioClock { base: self.start };
        let settings = settings::parse(chain).unwrap();
        run(
            &self.world,
            &mut events,
            &self.publisher,
            &clock,
            settings,
            script,
        )
        .await
        .unwrap();
    }

    /// Waits until this time of the test, in milliseconds.
    pub(super) async fn at(&self, millis: u64) {
        sleep_until(self.start + Duration::from_millis(millis)).await;
    }

    /// Tells the service that logind changed.
    pub(super) fn tell(&self, event: Event) {
        let _ = self.world.events.send(event);
    }

    /// The lid is closed or opened; logind sends its signal.
    pub(super) fn lid(&self, closed: bool) {
        self.world.lid_closed.set(closed);
        self.tell(Event::Login);
    }

    /// The power source changes; UPower sends its signal.
    pub(super) fn power(&self, power: PowerSource) {
        self.world.power.set(power);
        self.tell(Event::Power(power));
    }

    /// The session is used or left idle; logind sends its signal.
    pub(super) fn idle(&self, idle: bool, since: u64) {
        let mut sessions = self.world.sessions.borrow_mut();
        sessions[0].idle = idle;
        sessions[0].idle_since = since;
        drop(sessions);
        self.tell(Event::Login);
    }

    /// The time of the test now in microseconds of the clock of the
    /// service.
    pub(super) fn now(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    /// An inhibitor is taken; with `told`, logind sends its signal.
    pub(super) fn take(&self, inhibitor: Inhibitor, told: bool) {
        self.world.inhibitors.borrow_mut().push(inhibitor);
        if told {
            self.tell(Event::Login);
        }
    }

    /// Every inhibitor is let go; logind sends its signal.
    pub(super) fn let_go(&self) {
        self.world.inhibitors.borrow_mut().clear();
        self.tell(Event::Login);
    }

    /// The view published last.
    pub(super) fn view(&self) -> View {
        self.publisher.views.borrow().last().cloned().unwrap()
    }

    /// The jobs asked of systemd.
    pub(super) fn jobs(&self) -> Vec<String> {
        self.world.jobs.borrow().clone()
    }

    /// The actions asked of logind.
    pub(super) fn acted(&self) -> Vec<LogindAction> {
        self.world.acted.borrow().clone()
    }
}
