//! drowse-inhibit: the tool of the user that holds the inhibitor of
//! system: `on [--why <text>]`, `off`, `toggle [--why <text>]`,
//! `[--why <text>] -- <command>`; and `hold <why>`, the holder that `on`
//! starts.

use std::process::ExitCode;

use drowse::inhibit::{self, Outcome, hold, run};

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
    /// Hold it while a command runs.
    Run(Vec<String>),
    /// Be the holder of `on`.
    Hold,
}

/// The words of the command line, owned.
fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// The verb and the `why` of the command line; none when it is wrong.
fn parse(args: &[&str]) -> Option<(Verb, Option<String>)> {
    let why = |text: &str| Some(text.to_owned());
    Some(match args {
        ["on"] => (Verb::On, None),
        ["on", "--why", text] => (Verb::On, why(text)),
        ["off"] => (Verb::Off, None),
        ["toggle"] => (Verb::Toggle, None),
        ["toggle", "--why", text] => (Verb::Toggle, why(text)),
        ["--", command @ ..] if !command.is_empty() => (Verb::Run(owned(command)), None),
        ["--why", text, "--", command @ ..] if !command.is_empty() => {
            (Verb::Run(owned(command)), why(text))
        }
        [hold::VERB, text] => (Verb::Hold, why(text)),
        _ => return None,
    })
}

/// What the verb came to: a message, or the exit code of a command.
enum Done {
    /// `on`, `off`, `toggle`.
    Said(Outcome),
    /// The command, or the holder, ended with this code.
    Exited(u8),
}

fn main() -> ExitCode {
    drowse::language::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some((verb, why)) = parse(&words) else {
        eprintln!("usage: drowse-inhibit on|toggle [--why <text>]");
        eprintln!("       drowse-inhibit off");
        eprintln!("       drowse-inhibit [--why <text>] -- <command> [<argument>...]");
        return ExitCode::from(USAGE);
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
            Verb::On => inhibit::on(why).await.map(Done::Said),
            Verb::Off => inhibit::off().await.map(Done::Said),
            Verb::Toggle => inhibit::toggle(why).await.map(Done::Said),
            Verb::Run(command) => run::run(why, &command).await.map(Done::Exited),
            Verb::Hold => hold::hold(why.unwrap_or_default())
                .await
                .map(|()| Done::Exited(0)),
        }
    });
    match done {
        Ok(Done::Said(outcome)) => {
            println!("{}", outcome.message());
            ExitCode::SUCCESS
        }
        Ok(Done::Exited(code)) => ExitCode::from(code),
        Err(error) => {
            eprintln!("drowse-inhibit: {}", drowse::error::chain(&error));
            ExitCode::FAILURE
        }
    }
}
