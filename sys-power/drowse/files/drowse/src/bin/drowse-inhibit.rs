//! drowse-inhibit: the tool of the user that holds the inhibitor of
//! system: `on [--why <text>]`, `off`, `toggle [--why <text>]`.

use std::process::ExitCode;

/// The exit code of a wrong command line.
const USAGE: u8 = 2;

/// A command of the tool.
enum Verb {
    /// Take the inhibitor.
    On,
    /// Let it go.
    Off,
    /// Take it or let it go.
    Toggle,
}

fn main() -> ExitCode {
    drowse::language::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (verb, why) = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["on"] => (Verb::On, None),
        ["on", "--why", why] => (Verb::On, Some((*why).to_owned())),
        ["off"] => (Verb::Off, None),
        ["toggle"] => (Verb::Toggle, None),
        ["toggle", "--why", why] => (Verb::Toggle, Some((*why).to_owned())),
        _ => {
            eprintln!("usage: drowse-inhibit on|toggle [--why <text>]");
            eprintln!("       drowse-inhibit off");
            return ExitCode::from(USAGE);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("drowse-inhibit: {error}");
            return ExitCode::FAILURE;
        }
    };
    let why = why.as_deref();
    let done = runtime.block_on(async {
        match verb {
            Verb::On => drowse::inhibit::on(why).await,
            Verb::Off => drowse::inhibit::off().await,
            Verb::Toggle => drowse::inhibit::toggle(why).await,
        }
    });
    match done {
        Ok(outcome) => {
            println!("{}", outcome.message());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("drowse-inhibit: {}", drowse::error::chain(&error));
            ExitCode::FAILURE
        }
    }
}
