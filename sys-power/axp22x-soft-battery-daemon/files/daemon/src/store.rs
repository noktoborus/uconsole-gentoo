//! The files of the runs:
//!
//! ```text
//! discharge/full/     the last 5 valid runs to the silence
//! discharge/archive/  the valid runs past 5
//! discharge/invalid/  the runs that failed the check
//! discharge/partial/  the runs cut short
//! charge/full/        the last 5 charges that reached the end
//! charge/archive/     those past 5
//! charge/invalid/     those that failed the check
//! ```
//!
//! A file is `<seq>.env`, six digits; seq is 1 + the highest of the kind.
//! full/ keeps 5, the others 100: past it the least seq goes. Past 5 in
//! charge/full/ the charge with the highest voltage at its start goes
//! first (`leave_order`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::envfile::EnvFile;
use crate::Result;

const FULL_KEEP: usize = 5;
const OTHER_KEEP: usize = 100;

pub const DISCHARGE: &str = "discharge";
pub const CHARGE: &str = "charge";

fn seqs(dir: &Path) -> Vec<u32> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut seqs: Vec<u32> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix(".env")?.parse().ok()
        })
        .collect();
    seqs.sort_unstable();
    seqs
}

fn file(dir: &Path, seq: u32) -> PathBuf {
    dir.join(format!("{seq:06}.env"))
}

fn next_seq(kind_dir: &Path) -> u32 {
    let subs = fs::read_dir(kind_dir).into_iter().flatten().flatten();
    subs.flat_map(|e| seqs(&e.path())).max().unwrap_or(0) + 1
}

fn sync_dir(dir: &Path) -> Result<()> {
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

fn write_synced(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    sync_dir(path.parent().ok_or("no directory")?)
}

/// Past 100 files the least seq is deleted.
fn trim(dir: &Path) -> Result<()> {
    let s = seqs(dir);
    for seq in s.iter().take(s.len().saturating_sub(OTHER_KEEP)) {
        fs::remove_file(file(dir, *seq))?;
    }
    Ok(())
}

/// The order in which the charges leave full/: the highest voltage at the
/// start first, the shortest span of the reads; at the same voltage the
/// least seq. A file with no START_V_UV, or that cannot be read, goes
/// first.
fn charge_leave_order(runs: &[(u32, Option<EnvFile>)]) -> Vec<u32> {
    let mut order: Vec<(i64, u32)> = runs
        .iter()
        .map(|(seq, e)| {
            let v = e.as_ref().and_then(|e| e.get_i64("START_V_UV")).unwrap_or(i64::MAX);
            (v, *seq)
        })
        .collect();
    order.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    order.into_iter().map(|(_, seq)| seq).collect()
}

/// The seqs of full/ in the order they leave: for a charge by
/// `charge_leave_order`, else the least seq first.
fn leave_order(kind_dir: &Path, full: &Path) -> Vec<u32> {
    let s = seqs(full);
    if kind_dir.file_name().is_none_or(|n| n != CHARGE) {
        return s;
    }
    let runs: Vec<(u32, Option<EnvFile>)> = s
        .into_iter()
        .map(|seq| {
            let text = fs::read_to_string(file(full, seq)).ok();
            (seq, text.and_then(|t| EnvFile::parse(&t).ok()))
        })
        .collect();
    charge_leave_order(&runs)
}

/// Past 5 in full/ the first of `leave_order` go to archive/.
fn rotate_full(kind_dir: &Path) -> Result<()> {
    let full = kind_dir.join("full");
    let archive = kind_dir.join("archive");
    let s = leave_order(kind_dir, &full);
    for seq in s.iter().take(s.len().saturating_sub(FULL_KEEP)) {
        fs::create_dir_all(&archive)?;
        fs::rename(file(&full, *seq), file(&archive, *seq))?;
        println!("{}: run {seq:06} to archive/", kind_dir.display());
    }
    trim(&archive)
}

/// Saves a run in <dir>/<kind>/<sub>/; the path of its file.
pub fn save(dir: &Path, kind: &str, sub: &str, env: &mut EnvFile) -> Result<PathBuf> {
    let kind_dir = dir.join(kind);
    let sub_dir = kind_dir.join(sub);
    fs::create_dir_all(&sub_dir)?;
    let seq = next_seq(&kind_dir);
    env.set("SEQ", format!("{seq:06}"));
    let path = file(&sub_dir, seq);
    write_synced(&path, &env.text())?;
    if sub == "full" {
        rotate_full(&kind_dir)?;
    } else {
        trim(&sub_dir)?;
    }
    Ok(path)
}

/// The runs of <dir>/<kind>/<sub>/, by seq.
pub fn load(dir: &Path, kind: &str, sub: &str) -> Result<Vec<(PathBuf, EnvFile)>> {
    let sub_dir = dir.join(kind).join(sub);
    seqs(&sub_dir)
        .into_iter()
        .map(|seq| {
            let path = file(&sub_dir, seq);
            let text = fs::read_to_string(&path)?;
            let env = EnvFile::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((path, env))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charge(start_v_uv: i64) -> Option<EnvFile> {
        let mut e = EnvFile::default();
        e.set("START_V_UV", start_v_uv);
        Some(e)
    }

    #[test]
    fn the_shortest_span_leaves_first() {
        let runs = vec![
            (1, charge(3_600_000)),
            (2, charge(4_150_000)),
            (3, Some(EnvFile::default())),
            (4, charge(3_900_000)),
            (5, None),
            (6, charge(3_900_000)),
            (7, charge(3_500_000)),
        ];
        assert_eq!(charge_leave_order(&runs), vec![3, 5, 2, 4, 6, 1, 7]);
    }
}
