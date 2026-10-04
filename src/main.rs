mod config;
mod detect;
mod detectors;
mod doctor;
mod grammar;
mod info;
mod local_config;
mod run;
mod summary;
mod theme;

use grammar::{EarlyExit, HelpTarget, ParsedArgv, Request};

/// Everything letme itself rejects exits with this code. A command letme ran
/// on your behalf propagates its own exit code instead, via [`run::CommandExit`].
const FAILURE: i32 = 1;

fn main() {
    let args = match collect_utf8_args() {
        Ok(args) => args,
        Err(e) => fail(&e),
    };

    let parsed = match grammar::segment_argv(&args) {
        Ok(parsed) => parsed,
        Err(e) => fail(&e),
    };

    let config = config::load_config();
    let theme = theme::Theme::load(&config);

    if let Some(early) = parsed.early {
        print_early_exit(early, &parsed, &config, &theme);
        std::process::exit(0);
    }

    let result = run_app(parsed, &config, &theme);

    match result {
        Ok(()) => {}
        Err(e) => {
            if let Some(exit) = e.downcast_ref::<run::CommandExit>() {
                std::process::exit(exit.0);
            }
            fail(&format!("{e:#}"));
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("Error: {message}");
    std::process::exit(FAILURE)
}

/// Collect argv (minus argv[0]) as UTF-8, rejecting any non-UTF-8 argument.
fn collect_utf8_args() -> Result<Vec<String>, String> {
    std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "argument is not valid UTF-8".to_string())
        })
        .collect()
}

fn print_early_exit(
    early: EarlyExit,
    parsed: &ParsedArgv,
    config: &config::Config,
    theme: &theme::Theme,
) {
    match early {
        EarlyExit::Version => println!("letme {}", env!("CARGO_PKG_VERSION")),
        EarlyExit::Help(HelpTarget::TopLevel) => print!("{}", grammar::top_level_help(theme)),
        EarlyExit::Help(HelpTarget::LastSegment) => {
            // An alias (or an unresolvable name) has no page of its own.
            match parsed.segments.last().map(|s| config.resolve(&s.head)) {
                Some(Ok(config::Resolved::Name(target))) => {
                    print!("{}", grammar::segment_help(target, theme))
                }
                _ => print!("{}", grammar::top_level_help(theme)),
            }
        }
    }
}

fn run_app(
    parsed: ParsedArgv,
    config: &config::Config,
    theme: &theme::Theme,
) -> anyhow::Result<()> {
    let dir = std::env::current_dir()?;

    let local = local_config::LocalConfig::load(&dir)?;

    detectors::js::warn_conflicting_lockfiles(&dir, theme);
    let mise_tasks = detectors::mise::MiseTasks::load(&dir);
    detectors::mise::warn_untrusted_config(&mise_tasks, theme);

    let groups = detectors::all_detectors(&mise_tasks);

    let request = config
        .expand(&parsed.segments)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    match request {
        Request::Info => info::show(&dir, &groups, parsed.globals.verbose, theme, config, &local),
        Request::Doctor => {
            if doctor::run(&dir, &groups, theme)? {
                Ok(())
            } else {
                Err(run::CommandExit(FAILURE).into())
            }
        }
        Request::Run(keys) => run::run(&dir, &groups, &keys, parsed.globals, theme, &local),
    }
}
