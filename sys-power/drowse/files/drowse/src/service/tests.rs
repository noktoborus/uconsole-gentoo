//! Tests of the driver: a mock of the world, the clock of tokio paused.

use std::cell::RefCell;

use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::time::Instant;

use super::*;
use crate::engine::State;
use crate::settings::LogindAction;
use crate::world::{Inhibitor, PowerSource, Session, SessionState};

/// The world, as a test sets it.
struct MockWorld {
    sessions: RefCell<Vec<Session>>,
    power: PowerSource,
    acted: RefCell<Vec<LogindAction>>,
    jobs: RefCell<Vec<String>>,
    events: UnboundedSender<Event>,
}

impl Login for MockWorld {
    async fn sessions(&self) -> Result<Vec<Session>> {
        Ok(self.sessions.borrow().clone())
    }

    async fn inhibitors(&self) -> Result<Vec<Inhibitor>> {
        Ok(Vec::new())
    }

    async fn act(&self, action: LogindAction) -> std::result::Result<(), Refusal> {
        self.acted.borrow_mut().push(action);
        Ok(())
    }
}

impl Power for MockWorld {
    async fn source(&self) -> PowerSource {
        self.power
    }
}

impl Units for MockWorld {
    async fn start(&self, unit: &str) -> std::result::Result<(), String> {
        self.jobs.borrow_mut().push(format!("start {unit}"));
        let _ = self.events.send(Event::JobEnded {
            unit: unit.to_owned(),
            done: true,
        });
        Ok(())
    }

    async fn stop(&self, unit: &str) -> std::result::Result<(), String> {
        self.jobs.borrow_mut().push(format!("stop {unit}"));
        let _ = self.events.send(Event::JobEnded {
            unit: unit.to_owned(),
            done: true,
        });
        Ok(())
    }

    async fn stage_units_up(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The views published.
#[derive(Default)]
struct MockPublisher {
    views: RefCell<Vec<View>>,
}

impl Publish for MockPublisher {
    async fn publish(&self, view: &View) -> Result<()> {
        self.views.borrow_mut().push(view.clone());
        Ok(())
    }
}

/// The clock of tokio, from the start of the test.
struct TokioClock {
    base: Instant,
}

impl Clock for TokioClock {
    fn now(&self) -> u64 {
        u64::try_from(self.base.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

fn idle_session() -> Session {
    Session {
        id: "1".to_owned(),
        uid: 1000,
        user: "alice".to_owned(),
        class: "user".to_owned(),
        kind: "wayland".to_owned(),
        tty: "tty1".to_owned(),
        remote_host: String::new(),
        idle: true,
        idle_since: 0,
        state: SessionState::Active,
    }
}

fn world(power: PowerSource) -> (MockWorld, UnboundedReceiver<Event>) {
    let (send, receive) = mpsc::unbounded_channel();
    let world = MockWorld {
        sessions: RefCell::new(vec![idle_session()]),
        power,
        acted: RefCell::new(Vec::new()),
        jobs: RefCell::new(Vec::new()),
        events: send,
    };
    (world, receive)
}

#[tokio::test(start_paused = true)]
async fn the_defaults_power_off_after_17_minutes() {
    let (world, mut events) = world(PowerSource::Battery);
    let publisher = MockPublisher::default();
    let clock = TokioClock {
        base: Instant::now(),
    };
    let stop = tokio::time::sleep(Duration::from_secs(18 * 60));
    run(
        &world,
        &mut events,
        &publisher,
        &clock,
        Settings::default(),
        stop,
    )
    .await
    .unwrap();
    assert_eq!(*world.acted.borrow(), [LogindAction::Poweroff]);
    let states: Vec<State> = publisher
        .views
        .borrow()
        .iter()
        .map(|view| view.state)
        .collect();
    assert_eq!(states, [State::Idle, State::Announced, State::Idle]);
}

#[tokio::test(start_paused = true)]
async fn activity_cancels_and_the_stages_run_their_resume() {
    let (settings, _) = settings::parse(
        "[IdleBattery]\nIdleTime=1m\nCountdown=Never\nAction=nmcli radio all off\nResume=nmcli radio all on\n",
    );
    let (world, mut events) = world(PowerSource::Battery);
    let sender = world.events.clone();
    let publisher = MockPublisher::default();
    let clock = TokioClock {
        base: Instant::now(),
    };
    let world = &world;
    let stop = async {
        tokio::time::sleep(Duration::from_secs(2 * 60)).await;
        world.sessions.borrow_mut()[0].idle = false;
        let _ = sender.send(Event::Login);
        tokio::time::sleep(Duration::from_secs(60)).await;
    };
    run(world, &mut events, &publisher, &clock, settings, stop)
        .await
        .unwrap();
    assert_eq!(
        *world.jobs.borrow(),
        [
            "start drowsed-stage@battery-0.service",
            "stop drowsed-stage@battery-0.service"
        ]
    );
    assert!(world.acted.borrow().is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_stop_of_the_service_runs_the_resume() {
    let (settings, _) = settings::parse(
        "[IdleAC]\nIdleTime=0\nCountdown=Never\nAction=tuned-adm profile powersave\nResume=tuned-adm auto_profile\n",
    );
    let (world, mut events) = world(PowerSource::Ac);
    let publisher = MockPublisher::default();
    let clock = TokioClock {
        base: Instant::now(),
    };
    let stop = tokio::time::sleep(Duration::from_secs(5));
    run(&world, &mut events, &publisher, &clock, settings, stop)
        .await
        .unwrap();
    assert_eq!(
        *world.jobs.borrow(),
        [
            "start drowsed-stage@ac-0.service",
            "stop drowsed-stage@ac-0.service"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn logind_gone_is_an_error() {
    let (world, mut events) = world(PowerSource::Ac);
    let _ = world.events.send(Event::LoginGone);
    let publisher = MockPublisher::default();
    let clock = TokioClock {
        base: Instant::now(),
    };
    let result = run(
        &world,
        &mut events,
        &publisher,
        &clock,
        Settings::default(),
        std::future::pending(),
    )
    .await;
    assert!(matches!(result, Err(Error::LogindGone)));
}
