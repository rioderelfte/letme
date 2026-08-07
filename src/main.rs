mod cli;
mod config;
mod detect;
mod detectors;
mod doctor;
mod info;
mod local_config;
mod run;
mod summary;
mod theme;

use clap::Parser;
use cli::Cli;

fn main() {
    let cli = Cli::parse();
    let config = config::load_config();
    let theme = theme::Theme::load(&config);

    let result = run_app(cli, &config, &theme);

    match result {
        Ok(()) => {}
        Err(e) => {
            if let Some(exit) = e.downcast_ref::<run::CommandExit>() {
                std::process::exit(exit.0);
            }
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }
}

fn run_app(cli: Cli, config: &config::Config, theme: &theme::Theme) -> anyhow::Result<()> {
    let dir = std::env::current_dir()?;

    let local = local_config::LocalConfig::load(&dir)?;

    detectors::js::warn_conflicting_lockfiles(&dir, theme);
    let mise_tasks = detectors::mise::MiseTasks::load(&dir);
    detectors::mise::warn_untrusted_config(&mise_tasks, theme);

    let groups = detectors::all_detectors(&mise_tasks);

    if cli.commands.is_empty() {
        info::show(&dir, &groups, cli.verbose, theme, config, &local)?;
    } else {
        run::run(&dir, &groups, &cli, theme, config, &local)?;
    }

    Ok(())
}
