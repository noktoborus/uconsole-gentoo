//! What the daemon reads: the battery of the module in sysfs, the
//! temperature of the PMIC in hwmon, the curve of the discharge in the
//! device tree.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::Result;

pub const BATTERY: &str = "/sys/class/power_supply/axp22x-soft-battery";
const HWMON: &str = "/sys/class/hwmon";
const HWMON_NAME: &str = "axp_hwmon";
const DT_CELLS: &str = "/proc/device-tree/axp22x-soft-battery-cells";
const DT_CURVE: &str = "/proc/device-tree/axp22x-soft-battery-cells/ocv-capacity-table-0";

/// The status of the module, as `status` names it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Charging,
    Discharging,
    NotCharging,
    Full,
    Unknown,
}

impl Status {
    fn parse(s: &str) -> Status {
        match s {
            "Charging" => Status::Charging,
            "Discharging" => Status::Discharging,
            "Not charging" => Status::NotCharging,
            "Full" => Status::Full,
            _ => Status::Unknown,
        }
    }
}

/// One read of the battery. The current is positive on a charge, as the
/// module gives it.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    pub status: Status,
    pub v_uv: i64,
    pub i_ua: i64,
    pub capacity: i64,
}

/// What a read of the battery found.
pub enum Read {
    Ok(Reading),
    /// The values of now are gone: the AXP is silent.
    Silent,
    /// The directory of the battery is not there.
    Gone,
    /// Another error.
    Failed(io::Error),
}

const ENODATA: i32 = 61;

fn read_value(dir: &Path, name: &str) -> io::Result<String> {
    Ok(fs::read_to_string(dir.join(name))?.trim().to_string())
}

fn read_number(dir: &Path, name: &str) -> io::Result<i64> {
    read_value(dir, name)?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {e}")))
}

fn read_all(dir: &Path) -> io::Result<Reading> {
    Ok(Reading {
        status: Status::parse(&read_value(dir, "status")?),
        v_uv: read_number(dir, "voltage_now")?,
        i_ua: read_number(dir, "current_now")?,
        capacity: read_number(dir, "capacity")?,
    })
}

pub fn read() -> Read {
    let dir = Path::new(BATTERY);
    if !dir.exists() {
        return Read::Gone;
    }
    match read_all(dir) {
        Ok(r) => Read::Ok(r),
        Err(e) if e.raw_os_error() == Some(ENODATA) => Read::Silent,
        Err(_) if !dir.exists() => Read::Gone,
        Err(e) => Read::Failed(e),
    }
}

pub fn present() -> bool {
    Path::new(BATTERY).exists()
}

/// A value of design of the battery: internal_resistance, voltage_min_design.
pub fn design(name: &str) -> Result<i64> {
    Ok(read_number(Path::new(BATTERY), name)
        .map_err(|e| format!("{BATTERY}/{name}: {e}"))?)
}

/// The hwmon of the ADC of the PMIC, found by its name.
pub fn find_hwmon() -> Option<PathBuf> {
    let entries = fs::read_dir(HWMON).ok()?;
    entries
        .flatten()
        .map(|e| e.path())
        .find(|p| read_value(p, "name").map(|n| n == HWMON_NAME).unwrap_or(false))
}

/// R of the device tree, the start of the count of R: the module starts
/// from it too, and learns its own.
pub fn dt_resistance() -> Result<i64> {
    let path = Path::new(DT_CELLS).join("factory-internal-resistance-micro-ohms");
    let raw = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cell: [u8; 4] = raw.as_slice().try_into().map_err(|_| format!("{}: not one cell", path.display()))?;
    Ok(u32::from_be_bytes(cell) as i64)
}

/// The temperature of the PMIC, in m°C.
pub fn read_temp(hwmon: &Path) -> Option<i64> {
    read_number(hwmon, "temp1_input").ok()
}

/// The curve of the discharge the module uses: pairs (uV, percent), the
/// voltage falling.
pub fn dt_curve() -> Option<Vec<(i64, i64)>> {
    let raw = fs::read(DT_CURVE).ok()?;
    if raw.len() % 8 != 0 || raw.is_empty() {
        return None;
    }
    let cells: Vec<i64> = raw
        .chunks(4)
        .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]) as i64)
        .collect();
    Some(cells.chunks(2).map(|p| (p[0], p[1])).collect())
}

/// The voltage at a level on a curve (uV, percent), by a line between
/// two points.
pub fn curve_ocv_at(curve: &[(i64, i64)], level: i64) -> Option<i64> {
    curve.windows(2).find_map(|w| {
        let ((v_hi, l_hi), (v_lo, l_lo)) = (w[0], w[1]);
        if level <= l_hi && level >= l_lo && l_hi != l_lo {
            Some(v_lo + (v_hi - v_lo) * (level - l_lo) / (l_hi - l_lo))
        } else {
            None
        }
    })
}
