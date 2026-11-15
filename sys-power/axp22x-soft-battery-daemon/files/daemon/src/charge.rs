//! A run of a charge: from the charger in to the end of the charge. It
//! keeps the phase of constant voltage: the current, and the charge that
//! came in after each point. gen-dts makes the percent of each point by
//! Q_full of the runs of a discharge.

use crate::envfile::{join, EnvFile};

/// The phase of constant voltage starts where the voltage comes within
/// 15 mV of its value at the end of the charge.
const CV_MARGIN_UV: i64 = 15_000;

#[derive(Clone, Copy)]
struct Point {
    t: f64,
    v_uv: i64,
    in_ua: i64,
    q_uah: f64,
}

pub struct Charge {
    pub start_unix: u64,
    points: Vec<Point>,
}

/// The phase of constant voltage: for each point, the current and the
/// charge in after it.
pub struct CvPhase {
    pub q_after_uah: Vec<i64>,
    pub i_ua: Vec<i64>,
}

impl Charge {
    pub fn new(start_unix: u64) -> Self {
        Charge { start_unix, points: Vec::new() }
    }

    pub fn add(&mut self, t: f64, v_uv: i64, in_ua: i64) {
        let mut p = Point { t, v_uv, in_ua, q_uah: 0.0 };
        if let Some(a) = self.points.last() {
            p.q_uah = a.q_uah + (a.in_ua + in_ua) as f64 / 2.0 * (t - a.t) / 3600.0;
        }
        self.points.push(p);
    }

    pub fn q_in_uah(&self) -> f64 {
        self.points.last().map_or(0.0, |p| p.q_uah)
    }

    fn seconds(&self) -> f64 {
        match (self.points.first(), self.points.last()) {
            (Some(a), Some(b)) => b.t - a.t,
            _ => 0.0,
        }
    }

    /// The voltage at the end of the charge: the mean of the last 3 reads.
    fn v_end(&self) -> i64 {
        let last = &self.points[self.points.len().saturating_sub(3)..];
        last.iter().map(|p| p.v_uv).sum::<i64>() / last.len().max(1) as i64
    }

    pub fn cv_phase(&self) -> CvPhase {
        let total = self.q_in_uah();
        let v_cv = self.v_end() - CV_MARGIN_UV;
        let start = self.points.iter().position(|p| p.v_uv >= v_cv).unwrap_or(self.points.len());
        let cv = &self.points[start..];
        CvPhase {
            q_after_uah: cv.iter().map(|p| (total - p.q_uah).round() as i64).collect(),
            i_ua: cv.iter().map(|p| p.in_ua).collect(),
        }
    }

    pub fn env(&self, cv: &CvPhase) -> EnvFile {
        let mut env = EnvFile::default();
        env.set("KIND", "charge");
        env.set("START", self.start_unix);
        env.set("SECONDS", self.seconds().round() as i64);
        env.set("READS", self.points.len());
        env.set("Q_IN_UAH", self.q_in_uah().round() as i64);
        env.set("START_V_UV", self.points.first().map_or(0, |p| p.v_uv));
        env.set("V_END_UV", self.v_end());
        env.set("CV_Q_AFTER_UAH", join(&cv.q_after_uah));
        env.set("CV_I_UA", join(&cv.i_ua));
        env
    }

    pub fn print(&self) {
        println!(
            "run of a charge: started {}, {:.0} s, {} reads, charge in {:.0} mAh",
            self.start_unix,
            self.seconds(),
            self.points.len(),
            self.q_in_uah() / 1000.0
        );
    }
}

fn mean(v: &[i64]) -> f64 {
    v.iter().sum::<i64>() as f64 / v.len().max(1) as f64
}

/// The check of a charge: the current falls along the phase of constant
/// voltage, by the means of its three thirds.
pub fn check(cv: &CvPhase) -> Option<String> {
    let n = cv.i_ua.len();
    if n < 6 {
        return Some(format!("the phase of constant voltage has {n} reads"));
    }
    let (a, b, c) = (
        mean(&cv.i_ua[..n / 3]),
        mean(&cv.i_ua[n / 3..2 * n / 3]),
        mean(&cv.i_ua[2 * n / 3..]),
    );
    if a > b && b > c {
        None
    } else {
        Some(format!("the current does not fall: {a:.0}, {b:.0}, {c:.0} uA"))
    }
}
