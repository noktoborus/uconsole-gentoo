//! import: a run of a discharge from a log of axp-log (JSON lines, UTF-8 or
//! UTF-16 as PowerShell saves it). The samples with the charger out are
//! the run, one each 10 s as the daemon reads; the last sample with the
//! charger in is the state at rest. The log ends at the silence of the AXP.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::battery::{self, Status};
use crate::discharge::{self, Discharge};
use crate::store::{self, DISCHARGE};
use crate::Result;

/// The daemon reads each 10 s; the log is thinned to the same.
const PERIOD_S: f64 = 10.0;

struct Sample {
    unix: f64,
    charger: bool,
    v_uv: i64,
    i_ua: i64,
    temp_mc: Option<i64>,
}

fn text(raw: &[u8]) -> String {
    match raw {
        [0xff, 0xfe, rest @ ..] => {
            let units: Vec<u16> =
                rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(raw).into_owned(),
    }
}

/// The value of a key of a flat JSON line: what is between `"key":` and
/// the next comma or brace.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(&format!("\"{key}\":"))? + key.len() + 3;
    let rest = &line[at..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn number(line: &str, key: &str) -> Option<f64> {
    field(line, key)?.parse().ok()
}

fn sample(line: &str) -> Option<Sample> {
    if field(line, "type")? != "\"sample\"" {
        return None;
    }
    Some(Sample {
        unix: number(line, "unix")?,
        charger: number(line, "ac_online")? == 1.0,
        v_uv: number(line, "bat_voltage_uv")? as i64,
        i_ua: number(line, "bat_current_ua")? as i64,
        temp_mc: number(line, "pmic_temp_mc").map(|t| t as i64),
    })
}

/// The run: the last sample with the charger in, then the samples with
/// it out, one each 10 s.
fn build(samples: &[Sample], r_start: i64) -> Result<Discharge> {
    let first = samples.iter().position(|s| !s.charger).ok_or("no sample with the charger out")?;
    let t0 = samples[first].unix;
    let mut d = Discharge::new(t0 as u64, r_start);
    if let Some(rest) = first.checked_sub(1).map(|i| &samples[i]) {
        d.add(rest.unix - t0, Status::Full, rest.v_uv, -rest.i_ua.max(0));
    }
    let mut next = t0;
    for s in samples[first..].iter().filter(|s| !s.charger) {
        if s.unix < next {
            continue;
        }
        next = s.unix + PERIOD_S;
        d.add(s.unix - t0, Status::Discharging, s.v_uv, s.i_ua.abs());
        if let Some(t) = s.temp_mc {
            d.add_temp(t);
        }
    }
    Ok(d)
}

fn or_design(value: Option<i64>, name: &str, flag: &str) -> Result<i64> {
    match value {
        Some(v) => Ok(v),
        None => battery::design(name).map_err(|e| format!("{e}; give {flag}").into()),
    }
}

pub fn run(dir: &Path, log: &Path, r_start: Option<i64>, v_min: Option<i64>) -> Result<ExitCode> {
    let r_start = match r_start {
        Some(r) => r,
        None => battery::dt_resistance().map_err(|e| format!("{e}; give --r-start"))?,
    };
    let v_min = or_design(v_min, "voltage_min_design", "--v-min")?;
    let raw = fs::read(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let samples: Vec<Sample> = text(&raw).lines().filter_map(sample).collect();

    let d = build(&samples, r_start)?;
    let model = d.model_whole();
    discharge::print(&d, Some(&model));
    let source = format!("import:{}", log.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()));
    let mut env = d.env(&model, &source);
    let sub = match discharge::check(&model, v_min) {
        None => "full",
        Some(reason) => {
            println!("the run failed the check: {reason}");
            env.set("REASON", reason);
            "invalid"
        }
    };
    let path = store::save(dir, DISCHARGE, sub, &mut env)?;
    println!("saved: {}", path.display());
    Ok(ExitCode::SUCCESS)
}
