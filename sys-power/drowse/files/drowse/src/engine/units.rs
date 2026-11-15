//! The units of the stages and their queues: a start, then a stop, one job
//! at a time, so a stop never cuts a start that runs.

use std::collections::BTreeMap;

use crate::settings::Power;

/// The name of the unit of a stage: `drowsed-stage@battery-1.service`.
pub fn unit_name(power: Power, stage: usize) -> String {
    format!("drowsed-stage@{}-{stage}.service", power.instance())
}

/// The number of the stage of a unit, from its name; 0 when it has none.
fn stage_number(name: &str) -> usize {
    name.trim_end_matches(".service")
        .rsplit('-')
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or(0)
}

/// A job asked of systemd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Start the unit: its `Action`.
    Start(String),
    /// Stop the unit: its `Resume`.
    Stop(String),
}

/// What is known of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Known {
    /// Not running.
    Down,
    /// Its start runs.
    Starting,
    /// Started: its `Action` ran.
    Up,
    /// Its stop runs.
    Stopping,
}

/// A unit: what is known of it and what is wanted of it.
#[derive(Debug, Clone, Copy)]
struct Unit {
    known: Known,
    wanted_up: bool,
    started_at: u64,
}

/// What came of the end of a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// Nothing to tell.
    Quiet,
    /// The start was cut or failed: its `Action` did not end well, and
    /// there is no `Resume`.
    StartFailed,
    /// A stop that waited for the start ran now, late.
    ResumeLate,
}

/// The units of the stages.
#[derive(Debug, Default)]
pub struct Units {
    units: BTreeMap<String, Unit>,
}

impl Units {
    /// Units left up from before: they are to be stopped.
    pub fn adopt_left_up(&mut self, names: impl IntoIterator<Item = String>) -> Vec<Job> {
        let mut jobs = Vec::new();
        for name in names {
            self.units.insert(
                name.clone(),
                Unit {
                    known: Known::Stopping,
                    wanted_up: false,
                    started_at: 0,
                },
            );
            jobs.push(Job::Stop(name));
        }
        jobs
    }

    /// Wants a unit up: a start, now or after its stop.
    pub fn want_up(&mut self, name: &str, now: u64) -> Vec<Job> {
        let unit = self.units.entry(name.to_owned()).or_insert(Unit {
            known: Known::Down,
            wanted_up: false,
            started_at: now,
        });
        unit.wanted_up = true;
        if unit.known == Known::Down {
            unit.known = Known::Starting;
            unit.started_at = now;
            return vec![Job::Start(name.to_owned())];
        }
        Vec::new()
    }

    /// Wants every unit down, the last stage first. Gives the jobs, and
    /// the units whose start still runs, with how long it has run.
    pub fn want_all_down(&mut self, now: u64) -> (Vec<Job>, Vec<(String, u64)>) {
        let mut jobs = Vec::new();
        let mut waiting = Vec::new();
        let mut order: Vec<(&String, &mut Unit)> = self.units.iter_mut().collect();
        order.sort_by_key(|(name, _)| std::cmp::Reverse(stage_number(name)));
        for (name, unit) in order {
            unit.wanted_up = false;
            match unit.known {
                Known::Up => {
                    unit.known = Known::Stopping;
                    jobs.push(Job::Stop(name.clone()));
                }
                Known::Starting => {
                    waiting.push((name.clone(), now.saturating_sub(unit.started_at)))
                }
                Known::Down | Known::Stopping => {}
            }
        }
        (jobs, waiting)
    }

    /// The job of a unit ended. Gives the next job of its queue, if any.
    pub fn job_ended(&mut self, name: &str, done: bool, now: u64) -> (Vec<Job>, Ended) {
        let Some(unit) = self.units.get_mut(name) else {
            return (Vec::new(), Ended::Quiet);
        };
        match unit.known {
            Known::Starting if !done => {
                unit.known = Known::Down;
                (self.reconcile(name, now), Ended::StartFailed)
            }
            Known::Starting => {
                unit.known = Known::Up;
                let late = !unit.wanted_up;
                let jobs = self.reconcile(name, now);
                let ended = if late {
                    Ended::ResumeLate
                } else {
                    Ended::Quiet
                };
                (jobs, ended)
            }
            Known::Stopping => {
                unit.known = Known::Down;
                (self.reconcile(name, now), Ended::Quiet)
            }
            Known::Down | Known::Up => (Vec::new(), Ended::Quiet),
        }
    }

    /// Whether no job runs and no unit is up.
    pub fn all_down(&self) -> bool {
        self.units.values().all(|unit| unit.known == Known::Down)
    }

    /// The next job that makes a unit as it is wanted.
    fn reconcile(&mut self, name: &str, now: u64) -> Vec<Job> {
        let Some(unit) = self.units.get_mut(name) else {
            return Vec::new();
        };
        match (unit.known, unit.wanted_up) {
            (Known::Up, false) => {
                unit.known = Known::Stopping;
                vec![Job::Stop(name.to_owned())]
            }
            (Known::Down, true) => {
                unit.known = Known::Starting;
                unit.started_at = now;
                vec![Job::Start(name.to_owned())]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "drowsed-stage@battery-0.service";
    const B: &str = "drowsed-stage@battery-1.service";

    #[test]
    fn names() {
        assert_eq!(unit_name(Power::Battery, 0), A);
        assert_eq!(unit_name(Power::Ac, 2), "drowsed-stage@ac-2.service");
    }

    #[test]
    fn start_then_stop() {
        let mut units = Units::default();
        assert_eq!(units.want_up(A, 0), [Job::Start(A.to_owned())]);
        assert_eq!(units.job_ended(A, true, 1), (vec![], Ended::Quiet));
        let (jobs, waiting) = units.want_all_down(2);
        assert_eq!(jobs, [Job::Stop(A.to_owned())]);
        assert!(waiting.is_empty());
        assert!(!units.all_down());
        units.job_ended(A, true, 3);
        assert!(units.all_down());
    }

    #[test]
    fn a_stop_waits_for_the_start() {
        let mut units = Units::default();
        units.want_up(A, 0);
        let (jobs, waiting) = units.want_all_down(4_000_000);
        assert!(jobs.is_empty());
        assert_eq!(waiting, [(A.to_owned(), 4_000_000)]);
        assert_eq!(
            units.job_ended(A, true, 5_000_000),
            (vec![Job::Stop(A.to_owned())], Ended::ResumeLate)
        );
    }

    #[test]
    fn a_failed_start_has_no_resume() {
        let mut units = Units::default();
        units.want_up(A, 0);
        units.want_all_down(1);
        assert_eq!(units.job_ended(A, false, 2), (vec![], Ended::StartFailed));
        assert!(units.all_down());
    }

    #[test]
    fn a_start_waits_for_the_stop() {
        let mut units = Units::default();
        units.want_up(A, 0);
        units.job_ended(A, true, 1);
        units.want_all_down(2);
        assert!(units.want_up(A, 3).is_empty());
        assert_eq!(
            units.job_ended(A, true, 4),
            (vec![Job::Start(A.to_owned())], Ended::Quiet)
        );
    }

    #[test]
    fn the_last_stage_stops_first() {
        let mut units = Units::default();
        units.want_up(A, 0);
        units.want_up(B, 0);
        units.job_ended(A, true, 1);
        units.job_ended(B, true, 1);
        let (jobs, _) = units.want_all_down(2);
        assert_eq!(jobs, [Job::Stop(B.to_owned()), Job::Stop(A.to_owned())]);
    }

    #[test]
    fn the_order_is_by_number() {
        let mut units = Units::default();
        let ten = "drowsed-stage@battery-10.service";
        for name in [A, B, ten] {
            units.want_up(name, 0);
            units.job_ended(name, true, 1);
        }
        let (jobs, _) = units.want_all_down(2);
        assert_eq!(
            jobs,
            [
                Job::Stop(ten.to_owned()),
                Job::Stop(B.to_owned()),
                Job::Stop(A.to_owned())
            ]
        );
    }

    #[test]
    fn units_left_up_are_stopped() {
        let mut units = Units::default();
        assert_eq!(
            units.adopt_left_up([A.to_owned()]),
            [Job::Stop(A.to_owned())]
        );
        assert!(!units.all_down());
        units.job_ended(A, true, 1);
        assert!(units.all_down());
    }
}
