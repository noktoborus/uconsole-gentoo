//! A run of a discharge: from the end of a full charge to the silence of
//! the AXP. It counts the charge and the energy out, learns R by the steps
//! of the load, and makes the table of OCV at each 5 % of the charge.

use crate::battery::{curve_level_at, curve_ocv_at, Status};
use crate::envfile::{join, EnvFile};

/// The table has a point each 5 %, from 100 % down to 0 %.
pub const TABLE_POINTS: usize = 21;

/// R is learned on a step of the current of at least 65 mA, over 3.7 V,
/// within 2/3..4/3 of the mean of the last 8 steps taken, as the kernel
/// does in adc-battery-helper.c.
const R_MIN_STEP_UA: i64 = 65_000;
const R_MIN_UV: i64 = 3_700_000;
const R_WINDOW: usize = 8;

/// The values at each point of the table are means of 60 s about it.
const TABLE_HALF_WINDOW_S: f64 = 30.0;

/// The bounds of a valid run.
const Q_MIN_UAH: f64 = 500_000.0;
const Q_MAX_UAH: f64 = 50_000_000.0;
const R_MIN_UOHM: i64 = 10_000;
const R_MAX_UOHM: i64 = 500_000;
const V_END_MAX_UV: i64 = 3_800_000;

#[derive(Clone, Copy)]
struct Point {
    t: f64,
    v_uv: i64,
    /// The current out of the cells, positive on a discharge.
    out_ua: i64,
    status: Status,
    /// The charge out from the start to this point.
    q_uah: f64,
}

/// R by the steps of the load.
struct RCount {
    start_uohm: i64,
    taken: Vec<f64>,
}

impl RCount {
    fn window_mean(&self) -> f64 {
        let last = &self.taken[self.taken.len().saturating_sub(R_WINDOW)..];
        if last.is_empty() {
            self.start_uohm as f64
        } else {
            last.iter().sum::<f64>() / last.len() as f64
        }
    }

    fn step(&mut self, a: &Point, b: &Point) {
        let di = (b.out_ua - a.out_ua).abs();
        if a.status != b.status || b.v_uv < R_MIN_UV || di < R_MIN_STEP_UA {
            return;
        }
        let r = (b.v_uv - a.v_uv).abs() as f64 * 1e6 / di as f64;
        let mean = self.window_mean();
        if r >= mean * 2.0 / 3.0 && r <= mean * 4.0 / 3.0 {
            self.taken.push(r);
        }
    }

    /// R of the run: the mean of all the steps taken; 0 with none.
    fn result(&self) -> i64 {
        if self.taken.is_empty() {
            0
        } else {
            (self.taken.iter().sum::<f64>() / self.taken.len() as f64).round() as i64
        }
    }
}

pub struct Discharge {
    pub start_unix: u64,
    points: Vec<Point>,
    e_uwh: f64,
    r: RCount,
    temps_mc: Vec<i64>,
}

/// What is worked out of a run.
pub struct Model {
    pub q_full_uah: f64,
    pub e_full_uwh: f64,
    pub r_uohm: i64,
    pub v_end_uv: Option<i64>,
    pub i_end_ua: Option<i64>,
    pub table: Vec<i64>,
    pub stop_level: Option<i64>,
}

impl Discharge {
    pub fn new(start_unix: u64, r_start_uohm: i64) -> Self {
        Discharge {
            start_unix,
            points: Vec::new(),
            e_uwh: 0.0,
            r: RCount { start_uohm: r_start_uohm, taken: Vec::new() },
            temps_mc: Vec::new(),
        }
    }

    pub fn reads(&self) -> usize {
        self.points.len()
    }

    pub fn seconds(&self) -> f64 {
        match (self.points.first(), self.points.last()) {
            (Some(a), Some(b)) => b.t - a.t,
            _ => 0.0,
        }
    }

    pub fn q_out_uah(&self) -> f64 {
        self.points.last().map_or(0.0, |p| p.q_uah)
    }

    /// A read at the time t (s): the charge and the energy by trapezoids.
    pub fn add(&mut self, t: f64, status: Status, v_uv: i64, out_ua: i64) {
        let mut p = Point { t, v_uv, out_ua, status, q_uah: 0.0 };
        if let Some(a) = self.points.last().copied() {
            let h = (t - a.t) / 3600.0;
            p.q_uah = a.q_uah + (a.out_ua + out_ua) as f64 / 2.0 * h;
            self.e_uwh += (a.v_uv as f64 * a.out_ua as f64 + v_uv as f64 * out_ua as f64)
                / 2.0e6
                * h;
            self.r.step(&a, &p);
        }
        self.points.push(p);
    }

    pub fn add_temp(&mut self, mc: i64) {
        self.temps_mc.push(mc);
    }

    /// R for the table: of the run, or the one it started with.
    fn r_for_table(&self) -> i64 {
        match self.r.result() {
            0 => self.r.start_uohm,
            r => r,
        }
    }

    /// OCV at the point where the charge out is q: the means of V and I
    /// over 60 s about it, V + I * R.
    fn ocv_at(&self, q_uah: f64) -> Option<i64> {
        let at = self.points.iter().position(|p| p.q_uah >= q_uah)?;
        let t = self.points[at].t;
        let near: Vec<&Point> = self
            .points
            .iter()
            .filter(|p| (p.t - t).abs() <= TABLE_HALF_WINDOW_S)
            .collect();
        let n = near.len() as f64;
        let v = near.iter().map(|p| p.v_uv as f64).sum::<f64>() / n;
        let i = near.iter().map(|p| p.out_ua as f64).sum::<f64>() / n;
        Some((v + i * self.r_for_table() as f64 / 1e6).round() as i64)
    }

    /// The level at the last read: OCV as in the table, on the curve.
    pub fn level_now(&self, curve: &[(i64, i64)]) -> Option<i64> {
        curve_level_at(curve, self.ocv_at(self.q_out_uah())?)
    }

    fn level(k: usize) -> i64 {
        100 - 5 * k as i64
    }

    fn table(&self, q_full: f64, q_known: f64, curve: Option<&[(i64, i64)]>) -> Vec<i64> {
        let mut table = Vec::with_capacity(TABLE_POINTS);
        for k in 0..TABLE_POINTS {
            let q = q_full * (100 - Self::level(k)) as f64 / 100.0;
            let ocv = if q <= q_known + 1.0 {
                self.ocv_at(q.min(q_known))
            } else {
                curve.and_then(|c| curve_ocv_at(c, Self::level(k)))
            };
            match ocv {
                Some(v) => table.push(v),
                None => break,
            }
        }
        table
    }

    fn end_means(&self) -> (i64, i64) {
        let last = &self.points[self.points.len().saturating_sub(3)..];
        let n = last.len().max(1) as i64;
        (
            last.iter().map(|p| p.v_uv).sum::<i64>() / n,
            last.iter().map(|p| p.out_ua).sum::<i64>() / n,
        )
    }

    /// The model of a run that reached the silence of the AXP.
    pub fn model_whole(&self) -> Model {
        let q_full = self.q_out_uah();
        let (v_end, i_end) = self.end_means();
        Model {
            q_full_uah: q_full,
            e_full_uwh: self.e_uwh,
            r_uohm: self.r.result(),
            v_end_uv: Some(v_end),
            i_end_ua: Some(i_end),
            table: self.table(q_full, q_full, None),
            stop_level: None,
        }
    }

    /// The model of a run cut short at a level: the charge and the energy
    /// out / (1 - the level); the points under the level from the curve
    /// the module uses. None when the level leaves nothing to count.
    pub fn model_partial(&self, stop_level: i64, curve: Option<&[(i64, i64)]>) -> Option<Model> {
        if !(0..100).contains(&stop_level) || self.points.len() < 2 {
            return None;
        }
        let part = 1.0 - stop_level as f64 / 100.0;
        let q_full = self.q_out_uah() / part;
        Some(Model {
            q_full_uah: q_full,
            e_full_uwh: self.e_uwh / part,
            r_uohm: self.r.result(),
            v_end_uv: None,
            i_end_ua: None,
            table: self.table(q_full, self.q_out_uah(), curve),
            stop_level: Some(stop_level),
        })
    }

    pub fn temps(&self) -> Option<(i64, i64, i64)> {
        let t = &self.temps_mc;
        if t.is_empty() {
            return None;
        }
        let mean = t.iter().sum::<i64>() / t.len() as i64;
        Some((mean, *t.iter().min()?, *t.iter().max()?))
    }

    /// The file of a run.
    pub fn env(&self, model: &Model, source: &str) -> EnvFile {
        let mut env = EnvFile::default();
        env.set("KIND", "discharge");
        env.set("SOURCE", source);
        env.set("START", self.start_unix);
        env.set("SECONDS", self.seconds().round() as i64);
        env.set("READS", self.points.len());
        env.set("Q_OUT_UAH", self.q_out_uah().round() as i64);
        env.set("Q_FULL_UAH", model.q_full_uah.round() as i64);
        env.set("E_FULL_UWH", model.e_full_uwh.round() as i64);
        env.set("R_UOHM", model.r_uohm);
        env.set("R_STEPS", self.r.taken.len());
        if let (Some(v), Some(i)) = (model.v_end_uv, model.i_end_ua) {
            env.set("V_END_UV", v);
            env.set("I_END_UA", i);
        }
        if let Some(level) = model.stop_level {
            env.set("STOP_LEVEL", level);
        }
        if let Some((mean, min, max)) = self.temps() {
            env.set("TEMP_MEAN_MC", mean);
            env.set("TEMP_MIN_MC", min);
            env.set("TEMP_MAX_MC", max);
        }
        env.set("OCV_TABLE", join(&model.table));
        env
    }
}

/// The checks of a run before discharge/full/; the reason it fails.
pub fn check(model: &Model, v_min_uv: i64) -> Option<String> {
    if model.table.len() != TABLE_POINTS {
        return Some(format!("the table has {} points, not {TABLE_POINTS}", model.table.len()));
    }
    if model.table.windows(2).any(|w| w[1] >= w[0]) {
        return Some("the voltage of the table does not fall".into());
    }
    if !(Q_MIN_UAH..=Q_MAX_UAH).contains(&model.q_full_uah) {
        return Some(format!("Q_full {:.0} uAh not in 0.5..50 Ah", model.q_full_uah));
    }
    if model.r_uohm != 0 && !(R_MIN_UOHM..=R_MAX_UOHM).contains(&model.r_uohm) {
        return Some(format!("R {} uOhm not 0 nor in 10..500 mOhm", model.r_uohm));
    }
    match model.v_end_uv {
        Some(v) if !(v_min_uv..=V_END_MAX_UV).contains(&v) => {
            Some(format!("V_end {v} uV not in {v_min_uv}..{V_END_MAX_UV}"))
        }
        _ => None,
    }
}

/// The lines of the journal with what a run gathered.
pub fn print(d: &Discharge, model: Option<&Model>) {
    println!(
        "run of a discharge: started {}, {:.0} s, {} reads, charge out {:.0} mAh, energy out {:.0} mWh, R {} mOhm of {} steps",
        d.start_unix,
        d.seconds(),
        d.reads(),
        d.q_out_uah() / 1000.0,
        d.e_uwh / 1000.0,
        d.r.result() / 1000,
        d.r.taken.len()
    );
    if let Some(m) = model {
        println!(
            "  Q_full {:.0} mAh, E_full {:.0} mWh, V_end {}, table {}",
            m.q_full_uah / 1000.0,
            m.e_full_uwh / 1000.0,
            m.v_end_uv.map_or("none".into(), |v| format!("{} mV", v / 1000)),
            join(&m.table)
        );
    }
}
