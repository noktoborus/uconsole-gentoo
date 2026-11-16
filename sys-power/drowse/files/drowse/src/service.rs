//! The service: the driver that joins the engine to the world. It reads
//! the world, gives the engine what happens with the time, does what the
//! engine says and publishes the state.

use std::cell::Cell;
use std::collections::VecDeque;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;
use zbus::Connection;

use crate::engine::{Command, Due, Engine, Job, Level, Snapshot, View};
use crate::error::{Error, Result};
use crate::settings::{self, Settings};
use crate::world::{Event, Login, Power, Refusal, Units};
use crate::{bus, journal};

/// How long the stop of the service waits for the `Resume` of the stages.
const STOP_WAIT: Duration = Duration::from_secs(10);

/// The time of `CLOCK_MONOTONIC`.
pub trait Clock {
    /// Now, in microseconds.
    fn now(&self) -> u64;
}

/// Where the state goes.
pub trait Publish {
    /// Publishes a view.
    fn publish(&self, view: &View) -> impl Future<Output = Result<()>>;
}

/// The clock of the system.
pub struct Monotonic;

impl Clock for Monotonic {
    fn now(&self) -> u64 {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let seconds = u64::try_from(time.tv_sec).unwrap_or_default();
        let nanos = u64::try_from(time.tv_nsec).unwrap_or_default();
        seconds
            .saturating_mul(1_000_000)
            .saturating_add(nanos / 1000)
    }
}

/// The state on the system bus: the objects come with the first view, then
/// the name, so a reader finds them when the name is there.
pub struct BusPublisher {
    conn: Connection,
    served: Cell<bool>,
}

impl BusPublisher {
    /// A publisher on a connection.
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            served: Cell::new(false),
        }
    }
}

impl Publish for BusPublisher {
    async fn publish(&self, view: &View) -> Result<()> {
        if self.served.get() {
            return bus::publish(&self.conn, view).await;
        }
        bus::serve(&self.conn, view).await?;
        bus::take_name(&self.conn).await?;
        self.served.set(true);
        Ok(())
    }
}

/// Runs the service on the real world until `SIGTERM`.
pub async fn serve() -> Result<()> {
    let (settings, warnings) = settings::load(Path::new(settings::PATH))?;
    for warning in warnings {
        journal::write(Level::Warning, &format!("{}: {warning}", settings::PATH));
    }
    let (system, mut events) = crate::world::system::connect().await?;
    let publisher = BusPublisher::new(system.connection().clone());
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|source| Error::Signal {
        name: "SIGTERM",
        source,
    })?;
    let stop = async move {
        terminate.recv().await;
    };
    run(&system, &mut events, &publisher, &Monotonic, settings, stop).await
}

/// Runs the engine over a world until `stop` comes, then stops the units
/// of the stages, waiting at most `STOP_WAIT`.
pub async fn run<W, P, C>(
    world: &W,
    events: &mut UnboundedReceiver<Event>,
    publisher: &P,
    clock: &C,
    settings: Settings,
    stop: impl Future<Output = ()>,
) -> Result<()>
where
    W: Login + Power + Units,
    P: Publish,
    C: Clock,
{
    let left_up = world.stage_units_up().await;
    let power = world.source().await;
    let (engine, out) = Engine::new(settings, power, clock.now(), left_up);
    let mut driver = Driver {
        world,
        clock,
        engine,
        published: None,
    };
    driver.execute(out).await;
    let snapshot = read(world).await?;
    let out = driver.engine.on_snapshot(clock.now(), snapshot);
    driver.execute(out).await;
    driver.publish(publisher).await?;
    tokio::pin!(stop);
    loop {
        let due = driver.engine.next_due();
        let wait = due.map(|(at, _)| Duration::from_micros(at.saturating_sub(clock.now())));
        tokio::select! {
            () = &mut stop => break,
            event = events.recv() => {
                let Some(event) = event else {
                    return Err(Error::LogindGone);
                };
                driver.on_event(event).await?;
            }
            () = sleep_or_never(wait) => {
                if let Some((_, kind)) = due {
                    driver.on_due(kind).await?;
                }
            }
        }
        driver.publish(publisher).await?;
    }
    driver.stop(events).await;
    driver.publish(publisher).await
}

/// Sleeps for a time, or never.
async fn sleep_or_never(wait: Option<Duration>) {
    match wait {
        Some(wait) => tokio::time::sleep(wait).await,
        None => std::future::pending().await,
    }
}

/// Reads the sessions and the inhibitors from logind.
async fn read<W: Login>(world: &W) -> Result<Snapshot> {
    Ok(Snapshot {
        sessions: world.sessions().await?,
        inhibitors: world.inhibitors().await?,
    })
}

/// The engine and the world it works on.
struct Driver<'a, W, C> {
    world: &'a W,
    clock: &'a C,
    engine: Engine,
    published: Option<View>,
}

impl<W, C> Driver<'_, W, C>
where
    W: Login + Power + Units,
    C: Clock,
{
    /// Gives an event of the world to the engine.
    async fn on_event(&mut self, event: Event) -> Result<()> {
        let out = match event {
            Event::Login => {
                let snapshot = read(self.world).await?;
                self.engine.on_snapshot(self.clock.now(), snapshot)
            }
            Event::Power(power) => self.engine.on_power(self.clock.now(), power),
            Event::JobEnded { unit, done } => {
                self.engine.on_job_ended(self.clock.now(), &unit, done)
            }
            Event::LoginGone => return Err(Error::LogindGone),
        };
        self.execute(out).await;
        Ok(())
    }

    /// The time of the engine came; before an action, logind is read again.
    async fn on_due(&mut self, due: Due) -> Result<()> {
        let out = match due {
            Due::Announce => self.engine.on_due(self.clock.now()),
            Due::Action => {
                let snapshot = read(self.world).await?;
                self.engine.on_snapshot(self.clock.now(), snapshot)
            }
        };
        self.execute(out).await;
        Ok(())
    }

    /// Stops the units of the stages and waits for their `Resume`.
    async fn stop(&mut self, events: &mut UnboundedReceiver<Event>) {
        let out = self.engine.stop(self.clock.now());
        self.execute(out).await;
        let deadline = tokio::time::sleep(STOP_WAIT);
        tokio::pin!(deadline);
        while !self.engine.settled() {
            tokio::select! {
                () = &mut deadline => {
                    journal::write(Level::Warning, "stop: the Resume of the stages did not end in time");
                    return;
                }
                event = events.recv() => match event {
                    Some(Event::JobEnded { unit, done }) => {
                        let out = self.engine.on_job_ended(self.clock.now(), &unit, done);
                        self.execute(out).await;
                    }
                    Some(_) => {}
                    None => return,
                }
            }
        }
    }

    /// Does what the engine says, and what comes of it.
    async fn execute(&mut self, out: Vec<Command>) {
        let mut queue: VecDeque<Command> = out.into();
        while let Some(command) = queue.pop_front() {
            match command {
                Command::Journal(level, text) => journal::write(level, &text),
                Command::Logind(action) => match self.world.act(action).await {
                    Ok(()) => {}
                    Err(Refusal::Inhibited) => journal::write(
                        Level::Notice,
                        &format!("{action}: held by an inhibitor taken just now"),
                    ),
                    Err(Refusal::Other(text)) => journal::write(
                        Level::Warning,
                        &format!("{action} refused by logind: {text}"),
                    ),
                },
                Command::Job(Job::Start(unit)) => {
                    if let Err(text) = self.world.start(&unit).await {
                        journal::write(
                            Level::Warning,
                            &format!("{unit} cannot be started: {text}"),
                        );
                        queue.extend(self.engine.on_job_ended(self.clock.now(), &unit, false));
                    }
                }
                Command::Job(Job::Stop(unit)) => {
                    if let Err(text) = self.world.stop(&unit).await {
                        journal::write(
                            Level::Warning,
                            &format!("{unit} cannot be stopped: {text}"),
                        );
                        queue.extend(self.engine.on_job_ended(self.clock.now(), &unit, true));
                    }
                }
            }
        }
    }

    /// Publishes the view when it changed.
    async fn publish<P: Publish>(&mut self, publisher: &P) -> Result<()> {
        let view = self.engine.view();
        if self.published.as_ref() == Some(&view) {
            return Ok(());
        }
        publisher.publish(&view).await?;
        self.published = Some(view);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
