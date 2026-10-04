//! The CLI grammar: `letme [globals] <segment>...`, each segment a name plus
//! its own flags. This module owns argv end to end: segmenting, flag
//! validation and help rendering. The names and flags themselves come from
//! `detect` ([`CanonicalCommand::all`] and [`CommandKey::VARIANTS`]); only
//! their wording for the terminal lives here.

use owo_colors::OwoColorize;

use crate::detect::{CanonicalCommand, CommandKey, Modifier};
use crate::theme::Theme;

/// A command that runs alone: it must be the first segment, everything after
/// it is its own arguments, and it can't be part of an alias. Standalone names
/// only resolve exactly, never by prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standalone {
    Doctor,
}

impl Standalone {
    pub fn all() -> &'static [Standalone] {
        &[Self::Doctor]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Self::Doctor => "Diagnose project health",
        }
    }

    /// Parse everything after the name on the command line.
    pub fn parse(self, args: &[String]) -> Result<Request, String> {
        match self {
            Self::Doctor => match args.first() {
                Some(arg) => Err(format!("doctor takes no arguments (got {arg})")),
                None => Ok(Request::Doctor),
            },
        }
    }
}

/// A resolvable name: a chainable canonical command or a standalone one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameTarget {
    Canonical(CanonicalCommand),
    Standalone(Standalone),
}

impl NameTarget {
    pub fn name(self) -> &'static str {
        match self {
            Self::Canonical(c) => c.as_str(),
            Self::Standalone(s) => s.name(),
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Self::Standalone(s) => s.about(),
            Self::Canonical(c) => match c {
                CanonicalCommand::Install => "Run install command(s) for detected ecosystems",
                CanonicalCommand::Test => "Run test command(s)",
                CanonicalCommand::E2e => "Run end-to-end test command(s)",
                CanonicalCommand::Lint => "Run lint command(s)",
                CanonicalCommand::Typecheck => "Run typecheck command(s)",
                CanonicalCommand::Format => "Run format command(s)",
                CanonicalCommand::Build => "Run build command(s)",
                CanonicalCommand::Clean => "Remove build artifacts/dependencies",
            },
        }
    }

    /// The modifiers this name accepts as flags, from [`CommandKey::VARIANTS`].
    pub fn flags(self) -> Vec<Modifier> {
        match self {
            Self::Canonical(canonical) => CommandKey::variants_of(canonical)
                .filter_map(|key| key.modifier)
                .collect(),
            Self::Standalone(_) => Vec::new(),
        }
    }
}

/// What a modifier's flag does, for the help pages.
fn flag_help(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Check => "Check formatting without writing",
        Modifier::Fix => "Fix what the linter can fix",
    }
}

/// Every resolvable name, in display order: `CanonicalCommand::all()`, then
/// `Standalone::all()`.
pub fn name_targets() -> impl Iterator<Item = NameTarget> {
    CanonicalCommand::all()
        .iter()
        .copied()
        .map(NameTarget::Canonical)
        .chain(
            Standalone::all()
                .iter()
                .copied()
                .map(NameTarget::Standalone),
        )
}

/// Resolve an exact name (no prefix matching, no aliases).
pub fn lookup_exact(input: &str) -> Option<NameTarget> {
    name_targets().find(|t| t.name() == input)
}

/// Comma-separated list of every valid name, for error messages.
pub fn valid_names() -> String {
    name_targets()
        .map(|t| t.name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Validate one segment's flags against its name's flag table, returning the
/// modifier they resolve to (`None` for a bare name). Callers guarantee every
/// element of `flags` is flag-shaped.
pub fn parse_segment_flags(
    canonical: CanonicalCommand,
    flags: &[String],
) -> Result<Option<Modifier>, String> {
    let target = NameTarget::Canonical(canonical);
    let table = target.flags();

    let Some(first) = flags.first() else {
        return Ok(None);
    };

    if table.is_empty() {
        return Err(format!(
            "{} does not take flags (got {first})",
            target.name()
        ));
    }

    if let Some(extra) = flags.get(1) {
        return Err(format!(
            "{} does not take more than one flag (got {first} and {extra})",
            target.name()
        ));
    }

    match table.iter().find(|m| m.flag() == first.as_str()) {
        Some(modifier) => Ok(Some(*modifier)),
        None => {
            let supported = table
                .iter()
                .map(|m| m.flag())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "{} does not support {first}. Supported flags: {supported}",
                target.name()
            ))
        }
    }
}

/// Global flags: recognized in any position, never claimable by a name.
struct GlobalFlag {
    kind: GlobalKind,
    short: char,
    long: &'static str,
    help: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalKind {
    Interactive,
    Verbose,
    Help,
    Version,
}

const GLOBALS: &[GlobalFlag] = &[
    GlobalFlag {
        kind: GlobalKind::Interactive,
        short: 'i',
        long: "interactive",
        help: "Prompt before executing each command",
    },
    GlobalFlag {
        kind: GlobalKind::Verbose,
        short: 'v',
        long: "verbose",
        help: "Show detection details on stderr",
    },
    GlobalFlag {
        kind: GlobalKind::Help,
        short: 'h',
        long: "help",
        help: "Print help",
    },
    GlobalFlag {
        kind: GlobalKind::Version,
        short: 'V',
        long: "version",
        help: "Print version",
    },
];

/// The global flags resolved from argv.
#[derive(Debug, Clone, Copy, Default)]
pub struct Globals {
    pub interactive: bool,
    pub verbose: bool,
}

/// One raw `name [flags...]` segment, before any name resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSegment {
    pub head: String,
    pub flags: Vec<String>,
}

impl RawSegment {
    /// The head followed by its flags, in argv order.
    pub fn tokens(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.head).chain(&self.flags)
    }
}

/// Which help page `--help`/`-h` asked for: the top-level page, or the page
/// for the name it followed (the last segment, since segmenting stops there).
#[derive(Debug, Clone, Copy)]
pub enum HelpTarget {
    TopLevel,
    LastSegment,
}

#[derive(Debug, Clone, Copy)]
pub enum EarlyExit {
    Help(HelpTarget),
    Version,
}

/// The result of segmenting argv: globals, segments in order, and an early
/// exit if `--help`/`--version` was seen (segments up to that point are still
/// populated, for help routing).
#[derive(Debug, Default)]
pub struct ParsedArgv {
    pub globals: Globals,
    pub segments: Vec<RawSegment>,
    pub early: Option<EarlyExit>,
}

/// Segment argv into `[globals] <name [flags...]>...`. Only syntax is checked;
/// names are resolved later, against the config.
pub fn segment_argv(args: &[String]) -> Result<ParsedArgv, String> {
    let mut globals = Globals::default();
    let mut segments: Vec<RawSegment> = Vec::new();
    let mut early: Option<EarlyExit> = None;

    for arg in args {
        if arg == "--" {
            return Err(
                "'--' is reserved: letme commands take no passthrough arguments".to_string(),
            );
        }

        if arg == "-" {
            return Err("unexpected argument '-'".to_string());
        }

        if let Some(rest) = arg.strip_prefix("--") {
            let (name, value) = match rest.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            };
            if let Some(global) = GLOBALS.iter().find(|g| g.long == name) {
                if value.is_some() {
                    return Err(format!("--{name} does not take a value"));
                }
                if apply_global(global, &mut globals, &segments, &mut early) {
                    break;
                }
                continue;
            }
            // No flag letme knows of takes a value, so `--check=all` is
            // rejected for its value, not as an unsupported flag.
            if value.is_some() {
                return Err(format!("--{name} does not take a value"));
            }
            bind_flag(&mut segments, arg.clone())?;
            continue;
        }

        if let Some(shorts) = arg.strip_prefix('-') {
            let chars: Vec<char> = shorts.chars().collect();
            let all_global = chars.iter().all(|c| GLOBALS.iter().any(|g| g.short == *c));
            if all_global {
                let mut exited = false;
                for c in &chars {
                    let global = GLOBALS.iter().find(|g| g.short == *c).unwrap();
                    if apply_global(global, &mut globals, &segments, &mut early) {
                        exited = true;
                        break;
                    }
                }
                if exited {
                    break;
                }
                continue;
            }
            bind_flag(&mut segments, arg.clone())?;
            continue;
        }

        segments.push(RawSegment {
            head: arg.clone(),
            flags: Vec::new(),
        });
    }

    Ok(ParsedArgv {
        globals,
        segments,
        early,
    })
}

/// Apply one recognized global flag. Returns `true` if it was `--help`/
/// `--version` and the caller should stop segmenting (early exit).
fn apply_global(
    global: &GlobalFlag,
    globals: &mut Globals,
    segments: &[RawSegment],
    early: &mut Option<EarlyExit>,
) -> bool {
    match global.kind {
        GlobalKind::Help => {
            *early = Some(EarlyExit::Help(if segments.is_empty() {
                HelpTarget::TopLevel
            } else {
                HelpTarget::LastSegment
            }));
            true
        }
        GlobalKind::Version => {
            *early = Some(EarlyExit::Version);
            true
        }
        GlobalKind::Interactive => {
            globals.interactive = true;
            false
        }
        GlobalKind::Verbose => {
            globals.verbose = true;
            false
        }
    }
}

fn bind_flag(segments: &mut [RawSegment], flag: String) -> Result<(), String> {
    match segments.last_mut() {
        Some(seg) => {
            seg.flags.push(flag);
            Ok(())
        }
        None => Err(format!("flag '{flag}' found before any command name")),
    }
}

/// What the segments asked for: the info view, one standalone command, or a
/// chain of canonical commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Info,
    Doctor,
    Run(Vec<CommandKey>),
}

/// The whole `--help` page: about line, usage, global options, then the
/// command block. Headings are styled with `theme`.
pub fn top_level_help(theme: &Theme) -> String {
    let mut out = String::new();
    out.push_str("Auto-detecting dev command runner\n\n");
    out.push_str(&format!(
        "{} letme [-i] [-v] <command [flags]>...\n\n",
        "Usage:".style(theme.header)
    ));
    out.push_str(&format!("{}\n", "Options:".style(theme.header)));

    let opt_names: Vec<String> = GLOBALS
        .iter()
        .map(|g| format!("-{}, --{}", g.short, g.long))
        .collect();
    let opt_width = opt_names.iter().map(|s| s.len()).max().unwrap_or(0) + 2;
    for (name, global) in opt_names.iter().zip(GLOBALS) {
        out.push_str(&format!("  {name:<opt_width$}{}\n", global.help));
    }

    out.push('\n');
    out.push_str(&command_block(theme));
    out.push('\n');
    out
}

/// One name's own help page: its usage line (with its flags), and its about
/// text.
pub fn segment_help(target: NameTarget, theme: &Theme) -> String {
    let flags = target.flags();

    let mut usage = format!("{} letme {}", "Usage:".style(theme.header), target.name());
    for modifier in &flags {
        usage.push_str(&format!(" [{}]", modifier.flag()));
    }

    let mut out = format!("{usage}\n\n{}\n", target.about());
    if !flags.is_empty() {
        out.push_str(&format!("\n{}\n", "Flags:".style(theme.header)));
        let width = flags.iter().map(|m| m.flag().len()).max().unwrap_or(0) + 2;
        for modifier in &flags {
            out.push_str(&format!(
                "  {:<width$}{}\n",
                modifier.flag(),
                flag_help(*modifier)
            ));
        }
    }
    out
}

/// The `Canonical commands (chainable):` and `Standalone commands:` blocks,
/// generated from `name_targets()`, plus the static `Examples:` block.
fn command_block(theme: &Theme) -> String {
    let width = name_targets().map(|t| t.name().len()).max().unwrap_or(0) + 2;
    let (chainable, standalone): (Vec<_>, Vec<_>) =
        name_targets().partition(|t| matches!(t, NameTarget::Canonical(_)));

    let mut lines = Vec::new();
    for (heading, targets) in [
        ("Canonical commands (chainable):", chainable),
        ("Standalone commands:", standalone),
    ] {
        lines.push(heading.style(theme.header).to_string());
        for target in targets {
            lines.push(format!("  {:<width$}{}", target.name(), target.about()));
            for modifier in target.flags() {
                let flag_width = width.saturating_sub(2);
                lines.push(format!(
                    "    {:<flag_width$}{}",
                    modifier.flag(),
                    flag_help(modifier)
                ));
            }
        }
        lines.push(String::new());
    }
    lines.push("Examples:".style(theme.header).to_string());

    const EXAMPLES: &[(&str, &str)] = &[
        ("letme", "Show detected project info"),
        ("letme test", "Run test command(s)"),
        ("letme test lint", "Chain multiple commands"),
        ("letme clean -i", "Interactive mode (confirm each action)"),
        ("letme doctor", "Project health checker"),
    ];
    let example_width = EXAMPLES.iter().map(|(e, _)| e.len()).max().unwrap_or(0) + 4;
    for (example, desc) in EXAMPLES {
        lines.push(format!("  {example:<example_width$}{desc}"));
    }
    lines.push(String::new());
    lines.push(
        "Aliases and palettes: ~/.config/letme/config.toml"
            .style(theme.hint)
            .to_string(),
    );

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(head: &str, flags: &[&str]) -> RawSegment {
        RawSegment {
            head: head.to_string(),
            flags: flags.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plain_names_open_segments() {
        let parsed = segment_argv(&args(&["test", "lint"])).unwrap();
        assert_eq!(parsed.segments.len(), 2);
        assert_eq!(parsed.segments[0].head, "test");
        assert_eq!(parsed.segments[1].head, "lint");
        assert!(parsed.early.is_none());
    }

    #[test]
    fn global_after_name_is_recognized() {
        let parsed = segment_argv(&args(&["clean", "-i"])).unwrap();
        assert!(parsed.globals.interactive);
        assert_eq!(parsed.segments, vec![seg("clean", &[])]);
    }

    #[test]
    fn global_before_and_after_name() {
        let parsed = segment_argv(&args(&["-v", "format", "--check", "test"])).unwrap();
        assert!(parsed.globals.verbose);
        assert_eq!(
            parsed.segments,
            vec![seg("format", &["--check"]), seg("test", &[])]
        );
    }

    #[test]
    fn flag_before_any_name_errors() {
        let err = segment_argv(&args(&["--check"])).unwrap_err();
        assert_eq!(err, "flag '--check' found before any command name");
    }

    #[test]
    fn unknown_flag_binds_to_open_segment() {
        let parsed = segment_argv(&args(&["test", "--check"])).unwrap();
        assert_eq!(parsed.segments, vec![seg("test", &["--check"])]);
    }

    #[test]
    fn double_dash_is_reserved_anywhere() {
        let err = segment_argv(&args(&["test", "--", "--nocapture"])).unwrap_err();
        assert_eq!(
            err,
            "'--' is reserved: letme commands take no passthrough arguments"
        );
        let err = segment_argv(&args(&["--", "test"])).unwrap_err();
        assert_eq!(
            err,
            "'--' is reserved: letme commands take no passthrough arguments"
        );
    }

    #[test]
    fn lone_dash_errors() {
        let err = segment_argv(&args(&["-"])).unwrap_err();
        assert_eq!(err, "unexpected argument '-'");
    }

    #[test]
    fn combined_global_shorts_all_apply() {
        let parsed = segment_argv(&args(&["-iv", "test"])).unwrap();
        assert!(parsed.globals.interactive);
        assert!(parsed.globals.verbose);
        assert_eq!(parsed.segments, vec![seg("test", &[])]);
    }

    #[test]
    fn global_long_with_value_errors() {
        let err = segment_argv(&args(&["--interactive=x"])).unwrap_err();
        assert_eq!(err, "--interactive does not take a value");
    }

    #[test]
    fn segment_flag_with_value_errors_at_the_segmenter() {
        // Rejected as a value, not as an unsupported `--check=all` flag.
        let err = segment_argv(&args(&["format", "--check=all"])).unwrap_err();
        assert_eq!(err, "--check does not take a value");
    }

    #[test]
    fn help_before_any_name_is_top_level() {
        let parsed = segment_argv(&args(&["--help"])).unwrap();
        assert!(matches!(
            parsed.early,
            Some(EarlyExit::Help(HelpTarget::TopLevel))
        ));
    }

    #[test]
    fn help_after_name_targets_that_segment() {
        let parsed = segment_argv(&args(&["lint", "--help"])).unwrap();
        assert!(matches!(
            parsed.early,
            Some(EarlyExit::Help(HelpTarget::LastSegment))
        ));
        assert_eq!(parsed.segments, vec![seg("lint", &[])]);
    }

    #[test]
    fn version_flag_short_circuits() {
        let parsed = segment_argv(&args(&["--version"])).unwrap();
        assert!(matches!(parsed.early, Some(EarlyExit::Version)));
    }

    #[test]
    fn global_between_name_and_flag_does_not_close_segment() {
        let parsed = segment_argv(&args(&["format", "-v", "--check"])).unwrap();
        assert!(parsed.globals.verbose);
        assert_eq!(parsed.segments, vec![seg("format", &["--check"])]);
    }

    #[test]
    fn empty_flag_table_rejects_any_flag() {
        let err = parse_segment_flags(CanonicalCommand::Test, &["--json".to_string()]).unwrap_err();
        assert_eq!(err, "test does not take flags (got --json)");
    }

    #[test]
    fn no_flags_is_ok() {
        assert_eq!(
            parse_segment_flags(CanonicalCommand::Format, &[]).unwrap(),
            None
        );
    }

    #[test]
    fn recognized_flag_resolves_its_modifier() {
        let modifier =
            parse_segment_flags(CanonicalCommand::Format, &["--check".to_string()]).unwrap();
        assert_eq!(modifier, Some(Modifier::Check));
    }

    #[test]
    fn unsupported_flag_on_a_non_empty_table_names_the_supported_ones() {
        let err = parse_segment_flags(CanonicalCommand::Format, &["-c".to_string()]).unwrap_err();
        assert_eq!(err, "format does not support -c. Supported flags: --check");
    }

    #[test]
    fn a_second_flag_is_rejected() {
        let err = parse_segment_flags(
            CanonicalCommand::Format,
            &["--check".to_string(), "--check".to_string()],
        )
        .unwrap_err();
        assert_eq!(
            err,
            "format does not take more than one flag (got --check and --check)"
        );
    }

    #[test]
    fn flags_come_from_the_variant_table() {
        assert_eq!(
            NameTarget::Canonical(CanonicalCommand::Lint).flags(),
            vec![Modifier::Fix]
        );
        assert_eq!(
            NameTarget::Canonical(CanonicalCommand::Format).flags(),
            vec![Modifier::Check]
        );
        assert!(
            NameTarget::Canonical(CanonicalCommand::Test)
                .flags()
                .is_empty()
        );
        assert!(
            NameTarget::Standalone(Standalone::Doctor)
                .flags()
                .is_empty()
        );
    }

    #[test]
    fn doctor_parses_without_arguments_only() {
        assert_eq!(Standalone::Doctor.parse(&[]).unwrap(), Request::Doctor);
        for arg in ["test", "--json"] {
            let err = Standalone::Doctor.parse(&args(&[arg])).unwrap_err();
            assert_eq!(err, format!("doctor takes no arguments (got {arg})"));
        }
    }

    #[test]
    fn valid_names_lists_canonicals_then_doctor() {
        assert!(valid_names().ends_with(", doctor"));
        assert!(valid_names().starts_with("install, "));
    }

    #[test]
    fn lookup_exact_finds_standalones_and_canonicals() {
        assert_eq!(
            lookup_exact("doctor"),
            Some(NameTarget::Standalone(Standalone::Doctor))
        );
        assert_eq!(
            lookup_exact("test"),
            Some(NameTarget::Canonical(CanonicalCommand::Test))
        );
        assert_eq!(lookup_exact("nonsense"), None);
    }

    #[test]
    fn command_block_shows_flag_sub_lines_and_standalone_names() {
        // Each name's flags are listed under it, indented; doctor gets its own
        // block since it doesn't chain.
        let expected = "\
Canonical commands (chainable):
  install    Run install command(s) for detected ecosystems
  test       Run test command(s)
  e2e        Run end-to-end test command(s)
  lint       Run lint command(s)
    --fix    Fix what the linter can fix
  typecheck  Run typecheck command(s)
  format     Run format command(s)
    --check  Check formatting without writing
  build      Run build command(s)
  clean      Remove build artifacts/dependencies

Standalone commands:
  doctor     Diagnose project health

Examples:
  letme              Show detected project info
  letme test         Run test command(s)
  letme test lint    Chain multiple commands
  letme clean -i     Interactive mode (confirm each action)
  letme doctor       Project health checker

Aliases and palettes: ~/.config/letme/config.toml";
        assert_eq!(command_block(&Theme::plain()), expected);
    }

    #[test]
    fn top_level_help_pins_the_whole_page() {
        // Also pins the about line, the usage line and the Options: block.
        let expected = "\
Auto-detecting dev command runner

Usage: letme [-i] [-v] <command [flags]>...

Options:
  -i, --interactive  Prompt before executing each command
  -v, --verbose      Show detection details on stderr
  -h, --help         Print help
  -V, --version      Print version

Canonical commands (chainable):
  install    Run install command(s) for detected ecosystems
  test       Run test command(s)
  e2e        Run end-to-end test command(s)
  lint       Run lint command(s)
    --fix    Fix what the linter can fix
  typecheck  Run typecheck command(s)
  format     Run format command(s)
    --check  Check formatting without writing
  build      Run build command(s)
  clean      Remove build artifacts/dependencies

Standalone commands:
  doctor     Diagnose project health

Examples:
  letme              Show detected project info
  letme test         Run test command(s)
  letme test lint    Chain multiple commands
  letme clean -i     Interactive mode (confirm each action)
  letme doctor       Project health checker

Aliases and palettes: ~/.config/letme/config.toml
";
        assert_eq!(top_level_help(&Theme::plain()), expected);
    }

    #[test]
    fn segment_help_pins_lints_page() {
        let expected = "Usage: letme lint [--fix]\n\nRun lint command(s)\n\nFlags:\n  --fix  Fix what the linter can fix\n";
        assert_eq!(
            segment_help(
                NameTarget::Canonical(CanonicalCommand::Lint),
                &Theme::plain()
            ),
            expected
        );
    }

    #[test]
    fn segment_help_pins_a_flagless_page() {
        assert_eq!(
            segment_help(NameTarget::Standalone(Standalone::Doctor), &Theme::plain()),
            "Usage: letme doctor\n\nDiagnose project health\n"
        );
    }
}
