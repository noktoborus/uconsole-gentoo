//! What the daemon reads: the ADC of the PMIC (IIO `axp22x-adc`), the
//! charger (`axp22x-ac`), the status of the module when it is there, and
//! the values of the device tree.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::Result;

const IIO: &str = "/sys/bus/iio/devices";
const ADC_NAME: &str = "axp22x-adc";
const MODULE_STATUS: &str = "/sys/class/power_supply/axp22x-soft-battery/status";
const CHARGER_ONLINE: &str = "/sys/class/power_supply/axp22x-ac/online";
const DT_NODE: &str = "/proc/device-tree/axp22x-soft-battery";
const DT_CELLS: &str = "/proc/device-tree/axp22x-soft-battery-cells";

/// The status of the battery, as `status` of the module names it.
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

/// One read of the ADC. The currents are both positive.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub v_uv: i64,
    pub chrg_ua: i64,
    pub dischrg_ua: i64,
}

impl Sample {
    /// The current of the cells, positive on a charge.
    pub fn i_ua(&self) -> i64 {
        self.chrg_ua - self.dischrg_ua
    }
}

fn read_file(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_string())
}

fn read_value(dir: &Path, name: &str) -> io::Result<String> {
    read_file(&dir.join(name))
}

fn parse<T: std::str::FromStr>(text: &str, name: &str) -> io::Result<T>
where
    T::Err: std::fmt::Display,
{
    text.parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {e}")))
}

/// The ADC of the PMIC: the channels of the battery and of the
/// temperature, with their scales read once.
pub struct Adc {
    dir: PathBuf,
    v_scale: f64,
    chrg_scale: f64,
    dischrg_scale: f64,
    temp: Option<(f64, f64)>,
}

impl Adc {
    /// The IIO device named `axp22x-adc`, if it is there.
    pub fn find() -> Option<Adc> {
        let dir = fs::read_dir(IIO)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| read_value(p, "name").is_ok_and(|n| n == ADC_NAME))?;
        let scale = |name: &str| read_value(&dir, name).ok()?.parse::<f64>().ok();
        let temp = scale("in_temp_offset").zip(scale("in_temp_scale"));
        Some(Adc {
            v_scale: scale("in_voltage1_scale")?,
            chrg_scale: scale("in_current1_scale")?,
            dischrg_scale: scale("in_current2_scale")?,
            temp,
            dir,
        })
    }

    /// A raw value times its scale, in mV or mA, as micro units.
    fn micro(&self, name: &str, scale: f64) -> io::Result<i64> {
        let raw: f64 = parse(&read_value(&self.dir, name)?, name)?;
        Ok((raw * scale * 1000.0).round() as i64)
    }

    /// The voltage and the two currents of the battery, from the bus now.
    pub fn read(&self) -> io::Result<Sample> {
        Ok(Sample {
            v_uv: self.micro("in_voltage1_raw", self.v_scale)?,
            chrg_ua: self.micro("in_current1_raw", self.chrg_scale)?,
            dischrg_ua: self.micro("in_current2_raw", self.dischrg_scale)?,
        })
    }

    /// The temperature of the PMIC, in m°C.
    pub fn read_temp(&self) -> Option<i64> {
        let (offset, scale) = self.temp?;
        let raw: f64 = read_value(&self.dir, "in_temp_raw").ok()?.parse().ok()?;
        Some(((raw + offset) * scale).round() as i64)
    }
}

/// The status of the module; None when there is no module, no file, or
/// no value (the module found the AXP silent).
pub fn module_status() -> Option<Status> {
    read_file(Path::new(MODULE_STATUS)).ok().map(|s| Status::parse(&s))
}

/// The charger is in.
pub fn charger_online() -> io::Result<bool> {
    Ok(parse::<i64>(&read_file(Path::new(CHARGER_ONLINE))?, "online")? != 0)
}

/// One cell of the device tree, a big endian u32.
fn dt_u32(node: &str, name: &str) -> Result<i64> {
    let path = Path::new(node).join(name);
    let raw = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cell: [u8; 4] = raw.as_slice().try_into().map_err(|_| format!("{}: not one cell", path.display()))?;
    Ok(u32::from_be_bytes(cell) as i64)
}

/// R of the device tree, the start of the count of R.
pub fn dt_resistance() -> Result<i64> {
    dt_u32(DT_CELLS, "factory-internal-resistance-micro-ohms")
}

/// V_OFF of the device tree, the lowest V_end of a valid run.
pub fn dt_voltage_min() -> Result<i64> {
    dt_u32(DT_CELLS, "voltage-min-design-microvolt")
}

/// What the daemon needs to tell the status itself, as the module does
/// with no bit 6 of 0x01: the end of the charge by the current.
#[derive(Clone, Copy)]
pub struct FullRule {
    pub current_ua: i64,
    pub full_uv: i64,
}

impl FullRule {
    pub fn from_dt() -> Result<FullRule> {
        let target = dt_u32(DT_CELLS, "constant-charge-voltage-max-microvolt")?;
        let margin = dt_u32(DT_NODE, "axp22x-soft-battery,full-margin-microvolt")?;
        Ok(FullRule {
            current_ua: dt_u32(DT_NODE, "axp22x-soft-battery,full-current-microamp")?,
            full_uv: target - margin,
        })
    }

    /// The status by the charger, the current and the voltage. The cells
    /// that give current with the charger in discharge: the charger is too
    /// weak for the load. Full holds until the charger goes, the charge
    /// starts again or the cells give current.
    pub fn status(&self, prev: Option<Status>, online: bool, s: &Sample) -> Status {
        if !online || s.i_ua() < 0 {
            Status::Discharging
        } else if s.chrg_ua >= self.current_ua {
            Status::Charging
        } else if prev == Some(Status::Full) || s.v_uv >= self.full_uv {
            Status::Full
        } else {
            Status::NotCharging
        }
    }
}

/// The curve of the discharge the module uses: pairs (uV, percent), the
/// voltage falling.
pub fn dt_curve() -> Option<Vec<(i64, i64)>> {
    let raw = fs::read(Path::new(DT_CELLS).join("ocv-capacity-table-0")).ok()?;
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

/// The level at a voltage on a curve (uV, percent), by a line between two
/// points; over the first point its level, under the last its level.
pub fn curve_level_at(curve: &[(i64, i64)], ocv_uv: i64) -> Option<i64> {
    let (first, last) = (curve.first()?, curve.last()?);
    if ocv_uv >= first.0 {
        return Some(first.1);
    }
    if ocv_uv <= last.0 {
        return Some(last.1);
    }
    curve.windows(2).find_map(|w| {
        let ((v_hi, l_hi), (v_lo, l_lo)) = (w[0], w[1]);
        (ocv_uv <= v_hi && ocv_uv >= v_lo && v_hi != v_lo)
            .then(|| l_lo + (l_hi - l_lo) * (ocv_uv - v_lo) / (v_hi - v_lo))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVE: [(i64, i64); 3] = [(4200000, 100), (3800000, 50), (3400000, 0)];

    #[test]
    fn the_level_is_the_inverse_of_the_voltage() {
        assert_eq!(curve_level_at(&CURVE, 4000000), Some(75));
        assert_eq!(curve_level_at(&CURVE, 3600000), Some(25));
        assert_eq!(curve_level_at(&CURVE, 4300000), Some(100));
        assert_eq!(curve_level_at(&CURVE, 3000000), Some(0));
        assert_eq!(curve_ocv_at(&CURVE, 75), Some(4000000));
    }

    #[test]
    fn full_holds_until_the_charger_goes() {
        let rule = FullRule { current_ua: 100000, full_uv: 4150000 };
        let at = |v_uv, chrg_ua| Sample { v_uv, chrg_ua, dischrg_ua: 0 };
        assert_eq!(rule.status(None, true, &at(4200000, 500000)), Status::Charging);
        assert_eq!(rule.status(None, true, &at(4200000, 0)), Status::Full);
        assert_eq!(rule.status(Some(Status::Full), true, &at(4140000, 0)), Status::Full);
        assert_eq!(rule.status(None, true, &at(4140000, 0)), Status::NotCharging);
        assert_eq!(rule.status(Some(Status::Full), true, &at(4140000, 300000)), Status::Charging);
        assert_eq!(rule.status(Some(Status::Full), false, &at(4140000, 0)), Status::Discharging);
    }

    #[test]
    fn the_cells_that_give_current_discharge_with_the_charger_in() {
        let rule = FullRule { current_ua: 100000, full_uv: 4150000 };
        let out = Sample { v_uv: 4180000, chrg_ua: 0, dischrg_ua: 300000 };
        let rest = Sample { v_uv: 4180000, chrg_ua: 20000, dischrg_ua: 0 };
        assert_eq!(rule.status(Some(Status::Full), true, &out), Status::Discharging);
        assert_eq!(rule.status(None, true, &out), Status::Discharging);
        assert_eq!(rule.status(Some(Status::Full), true, &rest), Status::Full);
    }
}
