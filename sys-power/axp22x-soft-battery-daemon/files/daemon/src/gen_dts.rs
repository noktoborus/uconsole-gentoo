//! gen-dts: an overlay of the values worked out from the runs, and only
//! those. A value with no run to work it out is not written: the overlay
//! of the package keeps it. The overlay goes after the one of the package.

use std::fmt::Write;
use std::path::Path;
use std::process::ExitCode;

use crate::discharge::TABLE_POINTS;
use crate::envfile::EnvFile;
use crate::store::{self, CHARGE, DISCHARGE};
use crate::Result;

/// A run cut short counts with --use-partial when it went down to 10 %.
const PARTIAL_MAX_STOP: i64 = 10;

/// 0 % is 50 mV over the highest V_end.
const END_MARGIN_UV: i64 = 50_000;

/// The curve of the charge: a point each 2 %, at most 16 pairs.
const CV_STEP: i64 = 2;
const CV_MAX_PAIRS: usize = 16;

#[derive(Default)]
struct Values {
    table: Option<Vec<i64>>,
    celsius: Option<i64>,
    r_uohm: Option<i64>,
    energy_uwh: Option<i64>,
    end_uv: Option<i64>,
    cv: Option<Vec<(i64, i64)>>,
    notes: Vec<String>,
}

fn mean(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        None
    } else {
        Some((values.iter().sum::<i64>() as f64 / values.len() as f64).round() as i64)
    }
}

fn name(path: &Path) -> String {
    path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
}

/// The runs of the curve: discharge/full/, and with --use-partial the runs
/// of discharge/partial/ cut at 10 % or under with a whole table.
fn curve_runs(dir: &Path, use_partial: bool, notes: &mut Vec<String>) -> Result<Vec<EnvFile>> {
    let mut runs: Vec<EnvFile> = Vec::new();
    for (path, env) in store::load(dir, DISCHARGE, "full")? {
        notes.push(format!("discharge/full/{}", name(&path)));
        runs.push(env);
    }
    if !use_partial {
        return Ok(runs);
    }
    for (path, env) in store::load(dir, DISCHARGE, "partial")? {
        let stop = env.get_i64("STOP_LEVEL").unwrap_or(100);
        let whole = env.get_list("OCV_TABLE").is_some_and(|t| t.len() == TABLE_POINTS);
        if stop <= PARTIAL_MAX_STOP && whole {
            notes.push(format!("discharge/partial/{} (cut at {stop} %)", name(&path)));
            runs.push(env);
        } else {
            eprintln!("skipped discharge/partial/{}: cut at {stop} %", name(&path));
        }
    }
    Ok(runs)
}

fn mean_table(runs: &[EnvFile]) -> Option<Vec<i64>> {
    let tables: Vec<Vec<i64>> = runs
        .iter()
        .filter_map(|r| r.get_list("OCV_TABLE"))
        .filter(|t| t.len() == TABLE_POINTS)
        .collect();
    if tables.is_empty() {
        return None;
    }
    let table: Vec<i64> = (0..TABLE_POINTS)
        .map(|k| mean(&tables.iter().map(|t| t[k]).collect::<Vec<_>>()).unwrap_or(0))
        .collect();
    if table.windows(2).any(|w| w[1] >= w[0]) {
        eprintln!("the mean table does not fall point by point: not written");
        return None;
    }
    Some(table)
}

fn values_of(runs: &[EnvFile], key: &str, skip_zero: bool) -> Vec<i64> {
    runs.iter()
        .filter_map(|r| r.get_i64(key))
        .filter(|v| !skip_zero || *v != 0)
        .collect()
}

/// The percent of each point of a charge: 100 - the charge in after it
/// / Q_full; with its current, the percent rising.
fn charge_points(run: &EnvFile, q_full: f64) -> Option<Vec<(f64, f64)>> {
    let q = run.get_list("CV_Q_AFTER_UAH")?;
    let i = run.get_list("CV_I_UA")?;
    if q.len() != i.len() || q.is_empty() {
        return None;
    }
    Some(q.iter().zip(&i).map(|(q, i)| (100.0 - *q as f64 * 100.0 / q_full, *i as f64)).collect())
}

/// The current of a run at a percent, by a line between two points.
fn current_at(points: &[(f64, f64)], pct: f64) -> Option<f64> {
    points.windows(2).find_map(|w| {
        let ((p0, i0), (p1, i1)) = (w[0], w[1]);
        (pct >= p0 && pct <= p1 && p1 > p0).then(|| i0 + (i1 - i0) * (pct - p0) / (p1 - p0))
    })
}

/// The curve of the charge: on a grid of 2 %, at each step the mean current
/// of the runs that have that step; pairs with the current falling, at
/// most 16, the last ones.
fn mean_cv(charges: &[EnvFile], q_full: f64) -> Option<Vec<(i64, i64)>> {
    let runs: Vec<Vec<(f64, f64)>> =
        charges.iter().filter_map(|r| charge_points(r, q_full)).collect();
    let start = runs.iter().map(|p| p[0].0).fold(f64::MAX, f64::min);
    if runs.is_empty() || start >= 100.0 {
        return None;
    }
    let first = ((start / CV_STEP as f64).ceil() as i64 * CV_STEP).max(0);
    let mut pairs: Vec<(i64, i64)> = Vec::new();
    for pct in (first..=100).step_by(CV_STEP as usize) {
        let at: Vec<f64> = runs.iter().filter_map(|p| current_at(p, pct as f64)).collect();
        if at.is_empty() {
            continue;
        }
        let ua = (at.iter().sum::<f64>() / at.len() as f64).round() as i64;
        if pairs.last().is_none_or(|&(prev, _)| ua < prev) {
            pairs.push((ua, pct));
        }
    }
    let skip = pairs.len().saturating_sub(CV_MAX_PAIRS);
    let pairs: Vec<(i64, i64)> = pairs.into_iter().skip(skip).collect();
    (!pairs.is_empty()).then_some(pairs)
}

fn work_out(dir: &Path, use_partial: bool) -> Result<Values> {
    let mut v = Values::default();
    let runs = curve_runs(dir, use_partial, &mut v.notes)?;
    v.table = mean_table(&runs);
    if v.table.is_some() {
        let temps = values_of(&runs, "TEMP_MEAN_MC", false);
        v.celsius = mean(&temps).map(|mc| (mc as f64 / 1000.0).round() as i64);
        v.energy_uwh = mean(&values_of(&runs, "E_FULL_UWH", true));
    }
    v.r_uohm = mean(&values_of(&runs, "R_UOHM", true));

    let full = store::load(dir, DISCHARGE, "full")?;
    let full: Vec<EnvFile> = full.into_iter().map(|(_, e)| e).collect();
    v.end_uv = values_of(&full, "V_END_UV", false).into_iter().max().map(|e| e + END_MARGIN_UV);

    let charges: Vec<EnvFile> =
        store::load(dir, CHARGE, "full")?.into_iter().map(|(_, e)| e).collect();
    if let Some(q_full) = mean(&values_of(&full, "Q_FULL_UAH", true)) {
        v.cv = mean_cv(&charges, q_full as f64);
        v.notes.extend(charges.iter().filter_map(|c| c.get("SEQ")).map(|s| format!("charge/full/{s}.env")));
    } else if !charges.is_empty() {
        eprintln!("the runs of a charge need Q_full of discharge/full/: no curve of the charge");
    }
    Ok(v)
}

fn list(pairs: &[(i64, i64)]) -> String {
    pairs.iter().map(|(a, b)| format!("<{a} {b}>")).collect::<Vec<_>>().join(", ")
}

fn cells_props(v: &Values) -> String {
    let mut s = String::new();
    if let Some(t) = &v.table {
        if let Some(c) = v.celsius {
            let _ = writeln!(s, "\t\t\t\tocv-capacity-celsius = <{c}>;");
        }
        let pairs: Vec<(i64, i64)> =
            t.iter().enumerate().map(|(k, ocv)| (*ocv, 100 - 5 * k as i64)).collect();
        let _ = writeln!(s, "\t\t\t\tocv-capacity-table-0 = {};", list(&pairs));
    }
    if let Some(r) = v.r_uohm {
        let _ = writeln!(s, "\t\t\t\tfactory-internal-resistance-micro-ohms = <{r}>;");
    }
    s
}

fn node_props(v: &Values) -> String {
    let mut s = String::new();
    if let Some(e) = v.energy_uwh {
        let _ = writeln!(s, "\t\t\t\taxp22x-soft-battery,energy-full-microwatt-hours = <{e}>;");
    }
    if let Some(e) = v.end_uv {
        let _ = writeln!(s, "\t\t\t\taxp22x-soft-battery,end-microvolt = <{e}>;");
    }
    if let Some(cv) = &v.cv {
        let _ = writeln!(s, "\t\t\t\taxp22x-soft-battery,charge-cv-table = {};", list(cv));
    }
    s
}

fn dts(v: &Values) -> String {
    let mut s = String::from("// SPDX-License-Identifier: GPL-2.0\n/*\n");
    s += " * The values learned by axp22x-soft-battery, from:\n";
    for n in &v.notes {
        let _ = writeln!(s, " *\t{n}");
    }
    s += " *\n * In config.txt after dtoverlay=axp22x-soft-battery.\n */\n\n";
    s += "/dts-v1/;\n/plugin/;\n\n/ {\n\tcompatible = \"brcm,bcm2835\";\n\n";
    s += "\tfragment@0 {\n\t\ttarget-path = \"/\";\n\t\t__overlay__ {\n";
    let (node, cells) = (node_props(v), cells_props(v));
    if !node.is_empty() {
        s += "\t\t\taxp22x-soft-battery {\n";
        s += &node;
        s += "\t\t\t};\n";
    }
    if !cells.is_empty() {
        s += "\t\t\taxp22x-soft-battery-cells {\n";
        s += &cells;
        s += "\t\t\t};\n";
    }
    s += "\t\t};\n\t};\n};\n";
    s
}

pub fn run(dir: &Path, use_partial: bool) -> Result<ExitCode> {
    let v = work_out(dir, use_partial)?;
    if v.table.is_none() && v.r_uohm.is_none() && v.end_uv.is_none() && v.cv.is_none() {
        eprintln!("nothing to write: no run in {}", dir.display());
        return Ok(ExitCode::FAILURE);
    }
    print!("{}", dts(&v));
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charge(q_after: &str, i: &str) -> EnvFile {
        let mut e = EnvFile::default();
        e.set("CV_Q_AFTER_UAH", q_after);
        e.set("CV_I_UA", i);
        e
    }

    #[test]
    fn a_short_charge_does_not_cut_the_curve() {
        let long = charge("200,100,0", "1000,500,100");
        let short = charge("50,0", "400,200");
        let pairs = mean_cv(&[long, short], 1000.0).unwrap();
        assert_eq!(pairs.first(), Some(&(1000, 80)));
        assert_eq!(pairs.last(), Some(&(150, 100)));
    }
}
