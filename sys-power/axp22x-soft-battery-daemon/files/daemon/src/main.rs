//! axp22x-soft-battery: the program of user space of axp22x_soft_battery.
//!
//! - `daemon`: reads the battery of the module each 10 s, keeps the runs of
//!   a discharge (from a full charge to the silence of the AXP) and of a
//!   charge (to the end of the charge), and powers the system off at the
//!   silence;
//! - `gen-dts`: an overlay of the values worked out from the runs;
//! - `import`: a run of a discharge from a log of axp-log.
//!
//! The parts:
//!
//! - `battery.rs`: the files of the module, the hwmon of the PMIC and the
//!   device tree;
//! - `discharge.rs`, `charge.rs`: a run and what is worked out of it;
//! - `store.rs`: the files of the runs;
//! - `daemon.rs`, `gen_dts.rs`, `import.rs`: the three commands.

mod battery;
mod charge;
mod daemon;
mod discharge;
mod envfile;
mod gen_dts;
mod import;
mod store;

use std::path::PathBuf;
use std::process::ExitCode;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

const USAGE: &str = "\
usage: axp22x-soft-battery [--dir <dir>] daemon
       axp22x-soft-battery [--dir <dir>] gen-dts [--use-partial]
       axp22x-soft-battery [--dir <dir>] import <log> [--r-start <uohm>] [--v-min <uv>]

--dir      the directory of the runs, /var/lib/axp22x-soft-battery by default
--r-start  R to start the count of R, in micro-ohms; by default
           factory-internal-resistance-micro-ohms of the device tree
--v-min    the lowest V_end of a valid run, in uV; by default
           voltage_min_design of the battery of the module";

const DEFAULT_DIR: &str = "/var/lib/axp22x-soft-battery";

enum Command {
    Daemon,
    GenDts { use_partial: bool },
    Import { log: PathBuf, r_start: Option<i64>, v_min: Option<i64> },
}

struct Args {
    dir: PathBuf,
    command: Command,
}

fn parse_number(value: std::ffi::OsString) -> std::result::Result<i64, lexopt::Error> {
    use lexopt::prelude::*;
    let text = value.string()?;
    text.parse().map_err(|e| format!("{text}: {e}").into())
}

fn parse_import(parser: &mut lexopt::Parser) -> std::result::Result<Command, lexopt::Error> {
    use lexopt::prelude::*;
    let mut log = None;
    let mut r_start = None;
    let mut v_min = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("r-start") => r_start = Some(parse_number(parser.value()?)?),
            Long("v-min") => v_min = Some(parse_number(parser.value()?)?),
            Value(v) if log.is_none() => log = Some(PathBuf::from(v)),
            _ => return Err(arg.unexpected()),
        }
    }
    let log = log.ok_or("import: no log")?;
    Ok(Command::Import { log, r_start, v_min })
}

fn parse_gen_dts(parser: &mut lexopt::Parser) -> std::result::Result<Command, lexopt::Error> {
    use lexopt::prelude::*;
    let mut use_partial = false;
    while let Some(arg) = parser.next()? {
        match arg {
            Long("use-partial") => use_partial = true,
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(Command::GenDts { use_partial })
}

fn parse() -> std::result::Result<Args, lexopt::Error> {
    use lexopt::prelude::*;
    let mut parser = lexopt::Parser::from_env();
    let mut dir = PathBuf::from(DEFAULT_DIR);
    loop {
        match parser.next()? {
            Some(Long("dir")) => dir = parser.value()?.into(),
            Some(Value(v)) => {
                let name = v.string()?;
                let command = match name.as_str() {
                    "daemon" => {
                        if let Some(arg) = parser.next()? {
                            return Err(arg.unexpected());
                        }
                        Command::Daemon
                    }
                    "gen-dts" => parse_gen_dts(&mut parser)?,
                    "import" => parse_import(&mut parser)?,
                    _ => return Err(format!("unknown command {name}").into()),
                };
                return Ok(Args { dir, command });
            }
            Some(arg) => return Err(arg.unexpected()),
            None => return Err("no command".into()),
        }
    }
}

fn run(args: Args) -> Result<ExitCode> {
    match args.command {
        Command::Daemon => daemon::run(&args.dir),
        Command::GenDts { use_partial } => gen_dts::run(&args.dir, use_partial),
        Command::Import { log, r_start, v_min } => import::run(&args.dir, &log, r_start, v_min),
    }
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
