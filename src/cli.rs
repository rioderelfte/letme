use clap::Parser;

#[derive(Parser)]
#[command(
    name = "letme",
    about = "Auto-detecting dev command runner",
    after_help = "\
Canonical commands (chainable):
  install    Run install command(s) for detected ecosystems
  test       Run test command(s)
  e2e        Run end-to-end test command(s)
  lint       Run lint command(s)
  typecheck  Run typecheck command(s)
  fix        Auto-fix lint issues
  format     Run format command(s)
  build      Run build command(s)
  clean      Remove build artifacts/dependencies
  doctor     Diagnose project health

Built-in aliases:
  ok         Expands to: format, lint, typecheck, test

Examples:
  letme              Show detected project info
  letme test         Run test command(s)
  letme test lint    Chain multiple commands
  letme ok           Run format + lint + typecheck + test
  letme clean -i     Interactive mode (confirm each action)
  letme doctor       Project health checker"
)]
pub struct Cli {
    /// Prompt before executing each command
    #[arg(short, long)]
    pub interactive: bool,

    /// Show detection details on stderr
    #[arg(short, long)]
    pub verbose: bool,

    /// Commands to run (e.g. test, lint, doctor)
    pub commands: Vec<String>,
}
