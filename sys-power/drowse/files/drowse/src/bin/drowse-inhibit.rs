//! drowse-inhibit: the tool of the user that holds the inhibitor of
//! system: `on [--why <text>]`, `off`, `toggle [--why <text>]`,
//! `[--why <text>] -- <command>`; and `hold <why>`, the holder that `on`
//! starts.

use std::process::ExitCode;

use drowse::inhibit::{self, Kind, Outcome, hold, run};

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
    Cycle,
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
fn parse(args: &[&str]) -> Option<(Verb, Kind, Option<String>)> {
    let (verb, tail) = match *args.first()? {
        "on" => (Verb::On, &args[1..]),
        "off" => (Verb::Off, &args[1..]),
        "toggle" => (Verb::Toggle, &args[1..]),
        "cycle" => (Verb::Cycle, &args[1..]),
        "--" if args.len() > 1 => {
            return Some((Verb::Run(owned(&args[1..])), Kind::Idle, None));
        }
        hold::VERB if args.len() == 3 => {
            let why = args[1];
            let what = args[2];
            return Some((Verb::Hold, Kind::Idle, Some(format!("{why}\n{what}"))));
        }
        _ if args[0].starts_with("--") => (Verb::Run(Vec::new()), args),
        _ => return None,
    };
    let mut kind = Kind::Idle;
    let mut why = None;
    let mut index = 0;
    while index < tail.len() {
        match tail[index] {
            "--idle" => kind = Kind::Idle,
            "--power" => kind = Kind::Power,
            "--all" => kind = Kind::All,
            "--why" if index + 1 < tail.len() => {
                index += 1;
                why = Some(tail[index].to_owned());
            }
            "--" if index + 1 < tail.len() => {
                return Some((Verb::Run(owned(&tail[index + 1..])), kind, why));
            }
            _ => return None,
        }
        index += 1;
    }
    Some((verb, kind, why))
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
    let Some((verb, kind, why)) = parse(&words) else {
        eprintln!("usage: drowse-inhibit on|off|toggle [--idle|--power|--all] [--why <text>]");
        eprintln!("       drowse-inhibit cycle");
        eprintln!(
            "       drowse-inhibit [--idle|--power|--all] [--why <text>] -- <command> [<argument>...]"
        );
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
            Verb::On => inhibit::on(kind, why).await.map(Done::Said),
            Verb::Off => inhibit::off(kind).await.map(Done::Said),
            Verb::Toggle => inhibit::toggle(kind, why).await.map(Done::Said),
            Verb::Cycle => inhibit::cycle(why).await.map(Done::Said),
            Verb::Run(command) => run::run(kind, why, &command).await.map(Done::Exited),
            Verb::Hold => {
                let (why, what) = why
                    .unwrap_or_default()
                    .split_once('\n')
                    .unwrap_or(("", "idle"));
                hold::hold(why, what).await.map(|()| Done::Exited(0))
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_kind_and_defaults_to_idle() {
        assert!(matches!(parse(&["on"]), Some((Verb::On, Kind::Idle, None))));
        assert!(matches!(
            parse(&["off", "--power"]),
            Some((Verb::Off, Kind::Power, None))
        ));
        assert!(matches!(
            parse(&["toggle", "--all"]),
            Some((Verb::Toggle, Kind::All, None))
        ));
    }

    #[test]
    fn parses_cycle_and_command_kinds() {
        assert!(matches!(parse(&["cycle"]), Some((Verb::Cycle, _, None))));
        assert!(matches!(
            parse(&["--power", "--why", "build", "--", "make", "-j4"]),
            Some((Verb::Run(command), Kind::Power, Some(why)))
                if command == ["make", "-j4"] && why == "build"
        ));
    }
}
