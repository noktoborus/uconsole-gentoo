//! The daemon: one read each 10 s.
//!
//! - The voltage and the current come from the ADC of the PMIC (IIO
//!   `axp22x-adc`). The status comes from the module; with no module, no
//!   file or no value, the daemon works it out itself.
//! - Each read of the ADC that fails is a line of the journal. When no read
//!   works for 60 s, the AXP is gone.
//! - A run of a discharge starts when the status leaves Full; it ends when
//!   the AXP is gone: the run is saved and the system powered off.
//! - A run of a charge starts when the charger comes in; it ends at Full
//!   and is saved; the charger out before it cuts it.
//! - Each thing that spoils the count ends the runs: they are printed, a
//!   discharge is kept in discharge/partial/, and the daemon exits.

use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::battery::{self, Adc, FullRule, Sample, Status};
use crate::charge::{self, Charge};
use crate::discharge::{self, Discharge};
use crate::store::{self, CHARGE, DISCHARGE};
use crate::Result;

const PERIOD: Duration = Duration::from_secs(10);
const TEMP_PERIOD: Duration = Duration::from_secs(60);

/// More than 60 s between two tries to read spoils the count: the daemon
/// was held.
const MAX_GAP_S: f64 = 60.0;

/// No read of the ADC works for this time: the AXP is gone.
const GONE_S: f64 = 60.0;

/// The time asleep: CLOCK_BOOTTIME grew more than CLOCK_MONOTONIC.
const SUSPEND_S: f64 = 5.0;

/// A current past the reach of the ADC of the AXP22x is a wrong read.
const MAX_CURRENT_UA: i64 = 4_000_000;

static STOP: AtomicBool = AtomicBool::new(false);

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}

extern "C" fn on_stop(_: i32) {
    STOP.store(true, Ordering::SeqCst);
}

/// SIGTERM (a power off by upower or by hand) and SIGINT only set a flag:
/// the loop then prints what it gathered and exits.
fn catch_stop() {
    unsafe {
        signal(SIGTERM, on_stop);
        signal(SIGINT, on_stop);
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The time since the boot with the time asleep (CLOCK_BOOTTIME).
fn boottime_s() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.0)
}

/// A wait of one period, cut short by a stop.
fn wait_period() {
    let end = Instant::now() + PERIOD;
    while Instant::now() < end && !STOP.load(Ordering::SeqCst) {
        sleep(Duration::from_millis(500));
    }
}

/// One good read: the status and the values of the cells.
#[derive(Clone, Copy)]
struct Reading {
    status: Status,
    v_uv: i64,
    i_ua: i64,
}

/// Where the status comes from; a change is a line of the journal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Module,
    Daemon,
    Nothing,
}

/// A row of failed reads of the ADC.
struct Fails {
    count: u32,
    since: f64,
}

struct Daemon<'a> {
    dir: &'a Path,
    adc: Adc,
    rule: Option<FullRule>,
    started: Instant,
    last: Option<(f64, Reading)>,
    last_try: Option<f64>,
    fails: Option<Fails>,
    discharge: Option<Discharge>,
    charge: Option<Charge>,
    last_temp: Option<Instant>,
    boot_offset: f64,
    told_gone: bool,
    source: Option<Source>,
}

impl<'a> Daemon<'a> {
    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn in_run(&self) -> bool {
        self.discharge.is_some() || self.charge.is_some()
    }

    fn slept(&mut self) -> bool {
        let offset = boottime_s() - self.now();
        let grew = offset - self.boot_offset;
        self.boot_offset = offset;
        grew > SUSPEND_S
    }

    fn start_discharge(&mut self, rest: Option<(f64, Reading)>) {
        let r_start = match battery::dt_resistance() {
            Ok(r) => r,
            Err(e) => {
                println!("no run of a discharge: {e}");
                return;
            }
        };
        let mut d = Discharge::new(unix_now(), r_start);
        if let Some((t, r)) = rest {
            d.add(t, r.status, r.v_uv, -r.i_ua);
        }
        println!("run of a discharge started");
        self.discharge = Some(d);
    }

    /// The end of the charge: the run is saved, or kept in invalid/.
    fn end_charge(&mut self) -> Result<()> {
        let Some(c) = self.charge.take() else {
            return Ok(());
        };
        c.print();
        let cv = c.cv_phase();
        let mut env = c.env(&cv);
        let sub = match charge::check(&cv) {
            None => "full",
            Some(reason) => {
                println!("the run of the charge failed the check: {reason}");
                env.set("REASON", reason);
                "invalid"
            }
        };
        let path = store::save(self.dir, CHARGE, sub, &mut env)?;
        println!("run of a charge saved: {}", path.display());
        Ok(())
    }

    fn cut_charge(&mut self, why: &str) {
        if let Some(c) = self.charge.take() {
            println!("the run of the charge was cut: {why}; not kept");
            c.print();
        }
    }

    /// A run cut short: kept in discharge/partial/ with the reason and the
    /// level at the cut, by OCV on the curve of the device tree.
    fn break_off(&mut self, why: &str) -> Result<()> {
        self.cut_charge(why);
        let Some(d) = self.discharge.take() else {
            return Ok(());
        };
        println!("the run was broken off: {why}");
        let curve = battery::dt_curve();
        let stop_level = curve.as_deref().and_then(|c| d.level_now(c));
        let model = stop_level.and_then(|l| d.model_partial(l, curve.as_deref()));
        discharge::print(&d, model.as_ref());
        let Some(model) = model else {
            println!("  not kept: no level to count from");
            return Ok(());
        };
        let mut env = d.env(&model, "daemon");
        env.set("REASON", why);
        let path = store::save(self.dir, DISCHARGE, "partial", &mut env)?;
        println!("  kept: {}", path.display());
        Ok(())
    }

    /// The AXP gone at the end of a discharge: the run is saved, then the
    /// system is powered off.
    fn end_discharge(&mut self) -> Result<()> {
        let Some(d) = self.discharge.take() else {
            return Ok(());
        };
        println!("the AXP is gone: the end of the run");
        let model = d.model_whole();
        discharge::print(&d, Some(&model));
        let mut env = d.env(&model, "daemon");
        let v_min = battery::dt_voltage_min().unwrap_or(0);
        let sub = match discharge::check(&model, v_min) {
            None => "full",
            Some(reason) => {
                println!("the run failed the check: {reason}");
                env.set("REASON", reason);
                "invalid"
            }
        };
        let path = store::save(self.dir, DISCHARGE, sub, &mut env)?;
        println!("run of a discharge saved: {}", path.display());
        Ok(())
    }

    fn power_off() {
        println!("power off");
        if let Err(e) = Command::new("systemctl").args(["poweroff", "--force"]).status() {
            println!("systemctl poweroff --force: {e}");
        }
    }

    fn read_temp(&mut self) {
        if self.last_temp.is_some_and(|t| t.elapsed() < TEMP_PERIOD) {
            return;
        }
        self.last_temp = Some(Instant::now());
        let Some(mc) = self.adc.read_temp() else {
            return;
        };
        if let Some(d) = self.discharge.as_mut() {
            d.add_temp(mc);
        }
    }

    fn tell_source(&mut self, source: Source) {
        if self.source == Some(source) {
            return;
        }
        self.source = Some(source);
        println!(
            "status: {}",
            match source {
                Source::Module => "from the module",
                Source::Daemon => "worked out by the daemon: no status of the module",
                Source::Nothing => "none: no status of the module, no values of the device tree to work it out; reads skipped",
            }
        );
    }

    /// The status of the module, or the one the daemon works out.
    fn status(&mut self, s: &Sample) -> Option<Status> {
        if let Some(status) = battery::module_status() {
            self.tell_source(Source::Module);
            return Some(status);
        }
        let Some(rule) = self.rule else {
            self.tell_source(Source::Nothing);
            return None;
        };
        match battery::charger_online() {
            Ok(online) => {
                self.tell_source(Source::Daemon);
                Some(rule.status(self.last.map(|(_, r)| r.status), online, s))
            }
            Err(e) => {
                println!("a read of the charger failed: {e}; skipped");
                None
            }
        }
    }

    /// The changes of the status that start and end the runs.
    fn follow_status(&mut self, t: f64, r: Reading) -> Result<()> {
        let prev = self.last.map(|(_, p)| p.status);
        if prev == Some(Status::Full) && r.status != Status::Full && self.discharge.is_none() {
            self.start_discharge(self.last);
        }
        if self.discharge.is_some() && r.status == Status::Full {
            println!("full again: the run of the discharge starts again");
            if let Some(d) = self.discharge.take() {
                discharge::print(&d, None);
            }
        }
        if r.status == Status::Charging
            && self.charge.is_none()
            && !matches!(prev, Some(Status::Charging) | Some(Status::Full))
        {
            println!("run of a charge started");
            self.charge = Some(Charge::new(unix_now()));
        }
        if self.charge.is_some() && r.status == Status::Full {
            self.end_charge()?;
        }
        if r.status == Status::Discharging {
            self.cut_charge("the charger out");
        }
        if let Some(d) = self.discharge.as_mut() {
            d.add(t, r.status, r.v_uv, -r.i_ua);
        }
        if let Some(c) = self.charge.as_mut() {
            c.add(t, r.v_uv, r.i_ua);
        }
        Ok(())
    }

    /// A good read; the reason to break the runs off, if any.
    fn on_reading(&mut self, t: f64, r: Reading) -> Result<Option<String>> {
        if r.i_ua.abs() > MAX_CURRENT_UA {
            return Ok(Some(format!("a current of {} uA", r.i_ua)));
        }
        self.follow_status(t, r)?;
        self.last = Some((t, r));
        self.read_temp();
        Ok(None)
    }

    /// A failed read of the ADC: a line; after GONE_S of them the AXP is
    /// gone. Some(exit code) to stop.
    fn on_failed(&mut self, t: f64, e: std::io::Error) -> Result<Option<ExitCode>> {
        let f = self.fails.get_or_insert(Fails { count: 0, since: t });
        f.count += 1;
        let (count, gone_s) = (f.count, t - f.since);
        println!("the AXP did not answer: {e}; {count} in a row, {gone_s:.0} s");
        if gone_s < GONE_S {
            return Ok(None);
        }
        self.cut_charge("the AXP is gone");
        if self.discharge.is_some() {
            self.end_discharge()?;
            Self::power_off();
            return Ok(Some(ExitCode::SUCCESS));
        }
        if !self.told_gone {
            println!("the AXP is gone, with no run of a discharge");
            self.told_gone = true;
        }
        Ok(None)
    }

    fn answers_again(&mut self, t: f64) {
        if let Some(f) = self.fails.take() {
            println!("the AXP answers again after {} failed reads, {:.0} s", f.count, t - f.since);
        }
        self.told_gone = false;
    }

    /// One period; Some(exit code) to stop.
    fn step(&mut self) -> Result<Option<ExitCode>> {
        if STOP.load(Ordering::SeqCst) {
            if !self.in_run() {
                println!("stopped; no run was going on");
            }
            self.break_off("the daemon was stopped")?;
            return Ok(Some(ExitCode::SUCCESS));
        }
        if self.slept() && self.in_run() {
            self.break_off("a suspend")?;
            return Ok(Some(ExitCode::SUCCESS));
        }
        let t = self.now();
        let gap = self.last_try.replace(t).map(|prev| t - prev);
        if let Some(gap) = gap.filter(|&g| g > MAX_GAP_S && self.in_run()) {
            self.break_off(&format!("{gap:.0} s between two reads"))?;
            return Ok(Some(ExitCode::FAILURE));
        }
        let s = match self.adc.read() {
            Ok(s) => s,
            Err(e) => return self.on_failed(t, e),
        };
        self.answers_again(t);
        let Some(status) = self.status(&s) else {
            return Ok(None);
        };
        let r = Reading { status, v_uv: s.v_uv, i_ua: s.i_ua() };
        if let Some(why) = self.on_reading(t, r)? {
            self.break_off(&why)?;
            return Ok(Some(ExitCode::FAILURE));
        }
        Ok(None)
    }
}

/// The ADC of the PMIC; None on a stop before it came.
fn wait_for_adc() -> Option<Adc> {
    let mut told = false;
    loop {
        if let Some(adc) = Adc::find() {
            return Some(adc);
        }
        if !told {
            println!("waiting for the IIO device axp22x-adc");
            told = true;
        }
        if STOP.load(Ordering::SeqCst) {
            return None;
        }
        wait_period();
    }
}

pub fn run(dir: &Path) -> Result<ExitCode> {
    catch_stop();
    println!("started; the runs in {}", dir.display());
    let Some(adc) = wait_for_adc() else {
        return Ok(ExitCode::SUCCESS);
    };
    let rule = match FullRule::from_dt() {
        Ok(rule) => Some(rule),
        Err(e) => {
            println!("no status of its own: {e}");
            None
        }
    };
    let started = Instant::now();
    let mut d = Daemon {
        dir,
        adc,
        rule,
        started,
        last: None,
        last_try: None,
        fails: None,
        discharge: None,
        charge: None,
        last_temp: None,
        boot_offset: boottime_s(),
        told_gone: false,
        source: None,
    };
    loop {
        if let Some(code) = d.step()? {
            return Ok(code);
        }
        wait_period();
    }
}
