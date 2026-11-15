//! drowse-status: the tool of the user that shows the state of the
//! service: as text, or for waybar (`--waybar`), once or at each change
//! (`--follow`).

use std::process::ExitCode;

use drowse::status::client::{self, Form};

/// The exit code of a wrong command line.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    drowse::language::init();
    let mut form = Form::Plain;
    let mut follow = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--waybar" => form = Form::Waybar,
            "--follow" => follow = true,
            _ => {
                eprintln!("usage: drowse-status [--waybar] [--follow]");
                return ExitCode::from(USAGE);
            }
        }
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("drowse-status: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(client::run(form, follow)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("drowse-status: {}", drowse::error::chain(&error));
            ExitCode::FAILURE
        }
    }
}
