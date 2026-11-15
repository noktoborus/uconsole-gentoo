//! The daemon: one read of the battery each 10 s.
//!
//! - A run of a discharge starts when the status leaves Full; it ends at
//!   the silence of the AXP: the run is saved and the system powered off.
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

use crate::battery::{self, Read, Reading, Status};
use crate::charge::{self, Charge};
use crate::discharge::{self, Discharge};
use crate::store::{self, CHARGE, DISCHARGE};
use crate::Result;

const PERIOD: Duration = Duration::from_secs(10);
const TEMP_PERIOD: Duration = Duration::from_secs(60);

/// More than 60 s between two reads spoils the count.
const MAX_GAP_S: f64 = 60.0;

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

struct Daemon<'a> {
    dir: &'a Path,
    started: Instant,
    last: Option<(f64, Reading)>,
    discharge: Option<Discharge>,
    charge: Option<Charge>,
    hwmon: Option<std::path::PathBuf>,
    last_temp: Option<Instant>,
    boot_offset: f64,
    told_silent: bool,
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

    /// A run cut short: kept in discharge/partial/ with the reason.
    fn break_off(&mut self, why: &str) -> Result<()> {
        self.cut_charge(why);
        let Some(d) = self.discharge.take() else {
            return Ok(());
        };
        println!("the run was broken off: {why}");
        let stop_level = self.last.map(|(_, r)| r.capacity);
        let curve = battery::dt_curve();
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

    /// The silence of the AXP at the end of a discharge: the run is saved,
    /// then the system is powered off.
    fn end_discharge(&mut self) -> Result<()> {
        let Some(d) = self.discharge.take() else {
            return Ok(());
        };
        println!("the AXP is silent: the end of the run");
        let model = d.model_whole();
        discharge::print(&d, Some(&model));
        let mut env = d.env(&model, "daemon");
        let v_min = battery::design("voltage_min_design").unwrap_or(0);
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
        if self.hwmon.is_none() {
            self.hwmon = battery::find_hwmon();
        }
        let Some(mc) = self.hwmon.as_deref().and_then(battery::read_temp) else {
            return;
        };
        if let Some(d) = self.discharge.as_mut() {
            d.add_temp(mc);
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
    fn on_reading(&mut self, r: Reading) -> Result<Option<String>> {
        let t = self.now();
        self.told_silent = false;
        if let Some((last_t, _)) = self.last {
            if self.in_run() && t - last_t > MAX_GAP_S {
                return Ok(Some(format!("{:.0} s between two reads", t - last_t)));
            }
        }
        if r.i_ua.abs() > MAX_CURRENT_UA {
            return Ok(Some(format!("a current of {} uA", r.i_ua)));
        }
        self.follow_status(t, r)?;
        self.last = Some((t, r));
        self.read_temp();
        Ok(None)
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
        match battery::read() {
            Read::Ok(r) => {
                if let Some(why) = self.on_reading(r)? {
                    self.break_off(&why)?;
                    return Ok(Some(ExitCode::FAILURE));
                }
            }
            Read::Silent if self.discharge.is_some() => {
                self.cut_charge("the AXP is silent");
                self.end_discharge()?;
                Self::power_off();
                return Ok(Some(ExitCode::SUCCESS));
            }
            Read::Silent => {
                if !self.told_silent {
                    println!("the AXP is silent, with no run of a discharge");
                    self.told_silent = true;
                }
            }
            Read::Gone if self.in_run() => {
                self.break_off("the battery of the module is gone")?;
                return Ok(Some(ExitCode::FAILURE));
            }
            Read::Gone => self.last = None,
            Read::Failed(e) if self.in_run() => {
                self.break_off(&format!("a read failed: {e}"))?;
                return Ok(Some(ExitCode::FAILURE));
            }
            Read::Failed(e) => println!("a read failed: {e}"),
        }
        Ok(None)
    }
}

fn wait_for_battery() -> bool {
    if battery::present() {
        return true;
    }
    println!("waiting for {}", battery::BATTERY);
    while !battery::present() {
        if STOP.load(Ordering::SeqCst) {
            return false;
        }
        wait_period();
    }
    true
}

pub fn run(dir: &Path) -> Result<ExitCode> {
    catch_stop();
    println!("started; the runs in {}", dir.display());
    if !wait_for_battery() {
        return Ok(ExitCode::SUCCESS);
    }
    let started = Instant::now();
    let mut d = Daemon {
        dir,
        started,
        last: None,
        discharge: None,
        charge: None,
        hwmon: battery::find_hwmon(),
        last_temp: None,
        boot_offset: boottime_s(),
        told_silent: false,
    };
    loop {
        if let Some(code) = d.step()? {
            return Ok(code);
        }
        wait_period();
    }
}
