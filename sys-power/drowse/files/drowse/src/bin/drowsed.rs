//! drowsed: the system service that enters its sections while
//! every session is idle. `drowsed stage enter|leave <instance>` is the
//! command of a unit of a section.

use std::process::ExitCode;

use drowse::stage::{self, Part};

/// The exit code of a wrong command line.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => serve(),
        [word, part, instance] if word == "stage" => match Part::from_name(part) {
            Some(part) => run_stage(instance, part),
            None => usage(),
        },
        _ => usage(),
    }
}

/// Runs the service.
fn serve() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            drowse::journal::error(&error);
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(drowse::service::serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            drowse::journal::error(&error);
            ExitCode::FAILURE
        }
    }
}

/// Runs the command of a section in place of this process.
fn run_stage(instance: &str, part: Part) -> ExitCode {
    match stage::exec(instance, part) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            drowse::journal::error(&error);
            ExitCode::FAILURE
        }
    }
}

/// Says how to call the program.
fn usage() -> ExitCode {
    eprintln!("usage: drowsed");
    eprintln!("       drowsed stage enter|leave <main|lid>.<battery|ac>.<n>");
    ExitCode::from(USAGE)
}
