use std::fmt;
use std::path::Path;
use std::str::FromStr;

/// Canonical commands that letme understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalCommand {
    Install,
    Test,
    E2e,
    Lint,
    Typecheck,
    Fix,
    Format,
    Build,
    Clean,
}

impl CanonicalCommand {
    pub fn all() -> &'static [CanonicalCommand] {
        &[
            Self::Install,
            Self::Test,
            Self::E2e,
            Self::Lint,
            Self::Typecheck,
            Self::Fix,
            Self::Format,
            Self::Build,
            Self::Clean,
        ]
    }

    /// Comma-separated list of all command names, for error messages.
    pub fn all_names() -> String {
        Self::all()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for CanonicalCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Install => write!(f, "install"),
            Self::Test => write!(f, "test"),
            Self::E2e => write!(f, "e2e"),
            Self::Lint => write!(f, "lint"),
            Self::Typecheck => write!(f, "typecheck"),
            Self::Fix => write!(f, "fix"),
            Self::Format => write!(f, "format"),
            Self::Build => write!(f, "build"),
            Self::Clean => write!(f, "clean"),
        }
    }
}

impl FromStr for CanonicalCommand {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "install" => Ok(Self::Install),
            "test" => Ok(Self::Test),
            "e2e" => Ok(Self::E2e),
            "lint" => Ok(Self::Lint),
            "typecheck" => Ok(Self::Typecheck),
            "fix" => Ok(Self::Fix),
            "format" => Ok(Self::Format),
            "build" => Ok(Self::Build),
            "clean" => Ok(Self::Clean),
            _ => Err(format!(
                "Unknown command: {s}. Valid commands: {}",
                Self::all_names()
            )),
        }
    }
}

/// Detection tier. Lower numbers take precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Tier2,
    Tier3,
    Tier4,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tier2 => write!(f, "task runner"),
            Self::Tier3 => write!(f, "ecosystem script"),
            Self::Tier4 => write!(f, "convention"),
        }
    }
}

/// Ecosystem that a detector belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ecosystem {
    JavaScript,
    Php,
    Rust,
    TaskRunner,
}

impl fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JavaScript => write!(f, "JavaScript"),
            Self::Php => write!(f, "PHP"),
            Self::Rust => write!(f, "Rust"),
            Self::TaskRunner => write!(f, "Task Runner"),
        }
    }
}

/// A resolved command ready for execution.
#[derive(Debug, Clone)]
pub struct ResolvedCommand {
    pub canonical: CanonicalCommand,
    pub cmd: String,
    pub label: String,
    pub tier: Tier,
    pub ecosystem: Ecosystem,
    pub detector_name: String,
    pub priority: u32,
}

/// Trait implemented by all detectors.
pub trait Detector {
    fn name(&self) -> &str;
    fn tier(&self) -> Tier;
    fn ecosystem(&self) -> Ecosystem;

    /// Check whether this detector applies to the given directory.
    fn detect(&self, dir: &Path) -> bool;

    /// Return resolved commands for all canonical commands this detector can provide.
    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand>;

    /// System binaries required by this detector to resolve commands.
    /// If any are missing, `resolve_commands()` will be skipped.
    fn required_binaries(&self) -> &[&str] {
        &[]
    }

    /// Helper to build a ResolvedCommand with common fields filled in.
    fn make_command(
        &self,
        canonical: CanonicalCommand,
        cmd: String,
        priority: u32,
    ) -> ResolvedCommand {
        ResolvedCommand {
            canonical,
            label: cmd.clone(),
            cmd,
            tier: self.tier(),
            ecosystem: self.ecosystem(),
            detector_name: self.name().into(),
            priority,
        }
    }
}

/// An exclusive group of detectors: the first detector whose `detect()` returns
/// true wins, and the rest of the group is skipped. Single-element groups behave
/// as independent detectors.
pub struct DetectorGroup(pub Vec<Box<dyn Detector>>);

impl DetectorGroup {
    pub fn new(detectors: Vec<Box<dyn Detector>>) -> Self {
        Self(detectors)
    }
}

/// Wraps a detector and reports no required binaries, so resolution tests
/// don't depend on which package managers happen to be installed.
#[cfg(test)]
pub struct AssumeInstalled<D>(pub D);

#[cfg(test)]
impl<D: Detector> Detector for AssumeInstalled<D> {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn tier(&self) -> Tier {
        self.0.tier()
    }
    fn ecosystem(&self) -> Ecosystem {
        self.0.ecosystem()
    }
    fn detect(&self, dir: &Path) -> bool {
        self.0.detect(dir)
    }
    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        self.0.resolve_commands(dir)
    }
}

/// A detected binary that is missing from the system.
#[derive(Debug)]
pub struct MissingBinary {
    pub detector_name: String,
    pub binary: String,
}

/// Check all detected detectors for missing required binaries.
pub fn check_missing_binaries(groups: &[DetectorGroup], dir: &Path) -> Vec<MissingBinary> {
    let mut missing = Vec::new();
    for group in groups {
        for detector in &group.0 {
            if detector.detect(dir) {
                for &bin in detector.required_binaries() {
                    if which::which(bin).is_err() {
                        missing.push(MissingBinary {
                            detector_name: detector.name().into(),
                            binary: bin.into(),
                        });
                    }
                }
                break; // first match in group wins
            }
        }
    }
    missing
}

/// Run all detectors against a directory and resolve commands using tier logic.
///
/// Detectors are organized in **exclusive groups**: within each group, the first
/// detector whose `detect()` returns true wins, and the rest are skipped.
///
/// Resolution rules:
/// - Tier 2 overrides tier 3/4 for the same canonical command (cross-ecosystem)
/// - Within the same ecosystem, pick the highest priority match
/// - Across ecosystems at the same tier, run all
pub fn resolve_all(
    detector_groups: &[DetectorGroup],
    dir: &Path,
    commands: &[CanonicalCommand],
    verbose: bool,
) -> Vec<ResolvedCommand> {
    // Collect all resolved commands from all detectors that detect
    let mut all: Vec<ResolvedCommand> = Vec::new();
    for group in detector_groups {
        let mut group_matched: Option<&str> = None;
        for detector in &group.0 {
            if let Some(winner) = group_matched {
                if verbose {
                    eprintln!(
                        "[verbose] detector '{}' skipped ({} matched in group)",
                        detector.name(),
                        winner
                    );
                }
                continue;
            }
            if detector.detect(dir) {
                if verbose {
                    eprintln!(
                        "[verbose] detector '{}' matched ({})",
                        detector.name(),
                        detector.tier()
                    );
                }
                let missing: Vec<&str> = detector
                    .required_binaries()
                    .iter()
                    .filter(|b| which::which(b).is_err())
                    .copied()
                    .collect();
                if !missing.is_empty() {
                    if verbose {
                        eprintln!(
                            "[verbose] detector '{}' skipped: missing binaries: {}",
                            detector.name(),
                            missing.join(", ")
                        );
                    }
                    group_matched = Some(detector.name());
                    continue;
                }
                let resolved = detector.resolve_commands(dir);
                if verbose && resolved.is_empty() {
                    eprintln!(
                        "[verbose] detector '{}' matched ({}) but resolved no commands",
                        detector.name(),
                        detector.tier()
                    );
                }
                all.extend(resolved);
                group_matched = Some(detector.name());
            } else if verbose {
                eprintln!("[verbose] detector '{}' did not match", detector.name());
            }
        }
    }

    let mut result = Vec::new();

    for &cmd in commands {
        let matches: Vec<&ResolvedCommand> = all.iter().filter(|r| r.canonical == cmd).collect();
        if matches.is_empty() {
            continue;
        }

        // Find the best (lowest) tier among matches
        let best_tier = matches.iter().map(|r| r.tier).min().unwrap();

        if verbose && matches.iter().any(|r| r.tier != best_tier) {
            eprintln!("[verbose] {cmd}: tier {best_tier} overrides lower-priority tiers");
        }

        // If tier 2 matches, use only tier 2 (cross-ecosystem override)
        let tier_filtered: Vec<&ResolvedCommand> = matches
            .into_iter()
            .filter(|r| r.tier == best_tier)
            .collect();

        // Within the same ecosystem, keep only the highest priority
        let mut seen_ecosystems: std::collections::HashMap<Ecosystem, &ResolvedCommand> =
            std::collections::HashMap::new();

        for r in &tier_filtered {
            match seen_ecosystems.get(&r.ecosystem) {
                Some(existing) if existing.priority >= r.priority => {
                    if verbose {
                        eprintln!(
                            "[verbose] {cmd}: '{}' (priority {}) beaten by '{}' (priority {}) in {}",
                            r.cmd, r.priority, existing.cmd, existing.priority, r.ecosystem
                        );
                    }
                }
                _ => {
                    if verbose && let Some(old) = seen_ecosystems.get(&r.ecosystem) {
                        eprintln!(
                            "[verbose] {cmd}: '{}' (priority {}) replaces '{}' (priority {}) in {}",
                            r.cmd, r.priority, old.cmd, old.priority, r.ecosystem
                        );
                    }
                    seen_ecosystems.insert(r.ecosystem, r);
                }
            }
        }

        // Collect results in a stable order
        let mut cmd_results: Vec<ResolvedCommand> =
            seen_ecosystems.values().map(|r| (*r).clone()).collect();
        cmd_results.sort_by_key(|r| r.ecosystem);

        if verbose {
            for r in &cmd_results {
                eprintln!(
                    "[verbose] {cmd}: resolved → '{}' [{}]",
                    r.cmd, r.detector_name
                );
            }
        }

        result.extend(cmd_results);
    }

    result
}

/// Whether a script name matched exactly or via prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptMatchKind {
    Exact,
    Prefix,
}

impl ScriptMatchKind {
    /// Priority assigned to a name match of this kind.
    fn priority(self) -> u32 {
        match self {
            Self::Exact => 10,
            Self::Prefix => 5,
        }
    }
}

/// What content inspection concluded about a command string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inference {
    /// Recognized as a specific canonical command.
    Canonical(CanonicalCommand),
    /// Recognized as a check-only formatter run (`prettier --check`,
    /// `php-cs-fixer --dry-run`, `pint --test`, `biome format` without a write
    /// flag). It has no canonical slot: it does not mutate, so it is not a
    /// Format; it is not a Lint either.
    FormatVerify,
    /// Not recognized.
    Unknown,
}

impl Inference {
    /// The canonical command, if one was recognized. Format-verifies and
    /// unrecognized commands both yield `None`.
    pub fn canonical(self) -> Option<CanonicalCommand> {
        match self {
            Self::Canonical(cmd) => Some(cmd),
            Self::FormatVerify | Self::Unknown => None,
        }
    }
}

/// Map a name to a canonical command using exact matching only.
///
/// Covers all standard aliases: "test", "lint", "check", "format", "fmt", etc.
pub fn map_canonical_name(name: &str) -> Option<CanonicalCommand> {
    match name {
        "test" => Some(CanonicalCommand::Test),
        "e2e" | "test:e2e" => Some(CanonicalCommand::E2e),
        "fix" | "lint:fix" | "lint-fix" => Some(CanonicalCommand::Fix),
        "lint" | "check" | "analyse" | "analyze" => Some(CanonicalCommand::Lint),
        "typecheck" | "type-check" => Some(CanonicalCommand::Typecheck),
        "format" | "fmt" => Some(CanonicalCommand::Format),
        "build" => Some(CanonicalCommand::Build),
        "install" => Some(CanonicalCommand::Install),
        "clean" => Some(CanonicalCommand::Clean),
        _ => None,
    }
}

/// Map a script name to a canonical command using exact + prefix matching.
///
/// Matches: "test" and "test:unit" map to Test; "e2e", "test:e2e" and
/// "test:e2e:*" map to E2e (e2e suites must not hide behind `letme test`).
/// Does NOT match: "contest", "testing".
/// Returns the match kind so callers can assign different priorities.
pub fn map_script_name(name: &str) -> Option<(CanonicalCommand, ScriptMatchKind)> {
    if let Some(cmd) = map_canonical_name(name) {
        return Some((cmd, ScriptMatchKind::Exact));
    }

    // Prefix matches (name starts with "test:", "lint:", etc.)
    // "test:e2e:" must be claimed before the generic "test:" arm.
    if name.starts_with("test:e2e:") || name.starts_with("e2e:") {
        return Some((CanonicalCommand::E2e, ScriptMatchKind::Prefix));
    }
    if name.starts_with("test:") {
        return Some((CanonicalCommand::Test, ScriptMatchKind::Prefix));
    }
    if name.starts_with("fix:") {
        return Some((CanonicalCommand::Fix, ScriptMatchKind::Prefix));
    }
    if name.starts_with("lint:") {
        return Some((CanonicalCommand::Lint, ScriptMatchKind::Prefix));
    }
    if name.starts_with("typecheck:") || name.starts_with("type-check:") {
        return Some((CanonicalCommand::Typecheck, ScriptMatchKind::Prefix));
    }
    if name.starts_with("format:") || name.starts_with("fmt:") {
        return Some((CanonicalCommand::Format, ScriptMatchKind::Prefix));
    }
    if name.starts_with("build:") {
        return Some((CanonicalCommand::Build, ScriptMatchKind::Prefix));
    }

    None
}

/// Inspect a command string and classify it by recognizing known tool binaries.
///
/// Handles compound commands joined by `&&`, `||`, or `;`. If all recognized subcommands
/// agree on the same canonical command, returns it. If they disagree, returns `Unknown`.
/// A compound whose only recognized parts are format-verifies is itself a `FormatVerify`
/// (a recognized canonical always wins over one, so `prettier --check . && eslint .`
/// stays a Lint).
/// Pipes (`|`) do NOT split; they stay part of a single logical command.
pub fn infer_from_command(cmd: &str) -> Inference {
    let subcommands = split_compound_command(cmd);

    let mut result: Option<CanonicalCommand> = None;
    let mut saw_format_verify = false;

    for sub in &subcommands {
        match infer_single_command(sub) {
            Inference::Canonical(inferred) => match result {
                None => result = Some(inferred),
                Some(prev) if prev == inferred => {} // agree, continue
                Some(_) => return Inference::Unknown, // disagree
            },
            Inference::FormatVerify => saw_format_verify = true,
            Inference::Unknown => {}
        }
    }

    match result {
        Some(cmd) => Inference::Canonical(cmd),
        None if saw_format_verify => Inference::FormatVerify,
        None => Inference::Unknown,
    }
}

/// Classify a single (non-compound) command string.
///
/// Splits on whitespace, extracts basenames from paths (`vendor/bin/phpstan` counts as `phpstan`),
/// and skips interpreter prefixes (`php`, `node`, `npx`, `bunx`).
fn infer_single_command(cmd: &str) -> Inference {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Inference::Unknown;
    }

    // Skip interpreter prefixes to get the actual tool
    let interpreters = ["php", "node", "npx", "bunx"];
    let start = if interpreters.contains(&basename(parts[0])) {
        1
    } else {
        0
    };

    if start >= parts.len() {
        return Inference::Unknown;
    }

    let tool = basename(parts[start]);
    // Check for biome/playwright/cypress subcommands
    let subcommand = parts.get(start + 1).copied();

    let args = &parts[start + 1..];

    match tool {
        // Test
        "phpunit" | "pest" | "jest" | "vitest" | "mocha" => {
            Inference::Canonical(CanonicalCommand::Test)
        }
        // E2e: only an actual suite run counts; `playwright install`/`codegen`
        // and the interactive `cypress open` stay unclassified
        "playwright" => match subcommand {
            Some("test") => Inference::Canonical(CanonicalCommand::E2e),
            _ => Inference::Unknown,
        },
        "cypress" => match subcommand {
            Some("run") => Inference::Canonical(CanonicalCommand::E2e),
            _ => Inference::Unknown,
        },
        // Lint / Fix
        "phpstan" | "psalm" | "phpcs" => Inference::Canonical(CanonicalCommand::Lint),
        "eslint" => {
            if args.contains(&"--fix") {
                Inference::Canonical(CanonicalCommand::Fix)
            } else {
                Inference::Canonical(CanonicalCommand::Lint)
            }
        }
        // Format (with dry-run detection for check-only mode)
        "php-cs-fixer" => {
            if args.contains(&"--dry-run") {
                // Format-verify (dry-run) has no canonical slot: it doesn't
                // mutate, so it is neither a Format nor a Lint.
                Inference::FormatVerify
            } else {
                Inference::Canonical(CanonicalCommand::Format)
            }
        }
        "pint" => {
            if args.contains(&"--test") {
                // Format-verify (--test), not a lint.
                Inference::FormatVerify
            } else {
                Inference::Canonical(CanonicalCommand::Format)
            }
        }
        "prettier" => {
            if args.contains(&"--check") {
                // Format-verify (--check), not a lint.
                Inference::FormatVerify
            } else {
                Inference::Canonical(CanonicalCommand::Format)
            }
        }
        "phpcbf" => Inference::Canonical(CanonicalCommand::Format),
        // Build / Typecheck: bare tsc emits JS, --noEmit only checks types
        "tsc" => {
            if args.contains(&"--noEmit") {
                Inference::Canonical(CanonicalCommand::Typecheck)
            } else {
                Inference::Canonical(CanonicalCommand::Build)
            }
        }
        // Biome needs subcommand inspection
        "biome" => match subcommand {
            Some("check") | Some("lint") => {
                let biome_args = &parts[start + 2..];
                if biome_args
                    .iter()
                    .any(|a| *a == "--fix" || *a == "--apply" || *a == "--write")
                {
                    Inference::Canonical(CanonicalCommand::Fix)
                } else {
                    Inference::Canonical(CanonicalCommand::Lint)
                }
            }
            Some("format") => {
                let biome_args = &parts[start + 2..];
                // `--write` is Biome's standard write flag; `--fix` is its v2
                // alias. `--apply` (accepted by check/lint) was never valid
                // for `format`. Bare `biome format` only reports.
                if biome_args.iter().any(|a| *a == "--write" || *a == "--fix") {
                    Inference::Canonical(CanonicalCommand::Format)
                } else {
                    Inference::FormatVerify
                }
            }
            _ => Inference::Unknown,
        },
        _ => Inference::Unknown,
    }
}

/// Split a command string on `&&`, `||`, and `;` operators, respecting quoted strings.
/// Pipes (`|`) are left alone: a pipeline is one logical command.
fn split_compound_command(cmd: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = cmd.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < len {
        let ch = bytes[i];
        match ch {
            b'\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                i += 1;
            }
            b'"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                i += 1;
            }
            b'&' if !in_single_quote && !in_double_quote && i + 1 < len && bytes[i + 1] == b'&' => {
                let segment = cmd[start..i].trim();
                if !segment.is_empty() {
                    parts.push(segment);
                }
                i += 2;
                start = i;
            }
            b'|' if !in_single_quote && !in_double_quote && i + 1 < len && bytes[i + 1] == b'|' => {
                let segment = cmd[start..i].trim();
                if !segment.is_empty() {
                    parts.push(segment);
                }
                i += 2;
                start = i;
            }
            b';' if !in_single_quote && !in_double_quote => {
                let segment = cmd[start..i].trim();
                if !segment.is_empty() {
                    parts.push(segment);
                }
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    let segment = cmd[start..].trim();
    if !segment.is_empty() {
        parts.push(segment);
    }

    parts
}

/// Extract the basename (filename) from a potentially path-like string.
fn basename(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// Map a script by combining name matching with content inspection.
///
/// Returns `Option<(CanonicalCommand, priority)>`:
/// - Name claims Format/Fix but content is a check-only format run: None
/// - Name and content agree (or content unclassified): name's canonical, exact=10/prefix=5
/// - Name and content disagree: content's canonical, priority 3
/// - No name match, content matches: content's canonical, priority 7
/// - Neither matches: None
pub fn map_script(name: &str, command: &str) -> Option<(CanonicalCommand, u32)> {
    let name_match = map_script_name(name);
    let content_match = infer_from_command(command);

    match (name_match, content_match) {
        // Content is a check-only format run and the name claims a *mutating*
        // canonical. The script cannot deliver it, so leave the slot empty and
        // let a lower tier supply a real formatter. A check-shaped name
        // (`lint`, `check`) is still trusted by the arm below.
        (Some((CanonicalCommand::Format | CanonicalCommand::Fix, _)), Inference::FormatVerify) => {
            None
        }
        // Name matches, content unclassified: trust the name
        (Some((name_cmd, kind)), Inference::FormatVerify | Inference::Unknown) => {
            Some((name_cmd, kind.priority()))
        }
        // Name and content agree: trust the name
        (Some((name_cmd, kind)), Inference::Canonical(content_cmd)) if name_cmd == content_cmd => {
            Some((name_cmd, kind.priority()))
        }
        // Name and content disagree: trust the content
        (Some(_), Inference::Canonical(content_cmd)) => Some((content_cmd, 3)),
        // No name match, content matches
        (None, Inference::Canonical(content_cmd)) => Some((content_cmd, 7)),
        // Neither matches
        (None, Inference::FormatVerify | Inference::Unknown) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDetector {
        name: &'static str,
        tier: Tier,
        ecosystem: Ecosystem,
        commands: Vec<(CanonicalCommand, String, u32)>,
        binaries: &'static [&'static str],
    }

    impl Detector for MockDetector {
        fn name(&self) -> &str {
            self.name
        }
        fn tier(&self) -> Tier {
            self.tier
        }
        fn ecosystem(&self) -> Ecosystem {
            self.ecosystem
        }
        fn detect(&self, _dir: &Path) -> bool {
            true
        }
        fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
            self.commands
                .iter()
                .map(|(canonical, cmd, priority)| {
                    self.make_command(*canonical, cmd.clone(), *priority)
                })
                .collect()
        }
        fn required_binaries(&self) -> &[&str] {
            self.binaries
        }
    }

    fn mock(
        name: &'static str,
        tier: Tier,
        ecosystem: Ecosystem,
        commands: Vec<(CanonicalCommand, String, u32)>,
    ) -> Box<dyn Detector> {
        Box::new(MockDetector {
            name,
            tier,
            ecosystem,
            commands,
            binaries: &[],
        })
    }

    fn mock_with_binaries(
        name: &'static str,
        tier: Tier,
        ecosystem: Ecosystem,
        commands: Vec<(CanonicalCommand, String, u32)>,
        binaries: &'static [&'static str],
    ) -> Box<dyn Detector> {
        Box::new(MockDetector {
            name,
            tier,
            ecosystem,
            commands,
            binaries,
        })
    }

    /// Wrap a single mock in a one-element exclusive group.
    fn group(d: Box<dyn Detector>) -> DetectorGroup {
        DetectorGroup::new(vec![d])
    }

    #[test]
    fn tier2_overrides_tier3_and_tier4() {
        let groups = vec![
            group(mock(
                "justfile",
                Tier::Tier2,
                Ecosystem::TaskRunner,
                vec![(CanonicalCommand::Test, "just test".into(), 10)],
            )),
            group(mock(
                "npm",
                Tier::Tier3,
                Ecosystem::JavaScript,
                vec![(CanonicalCommand::Test, "npm run test".into(), 10)],
            )),
            group(mock(
                "cargo",
                Tier::Tier4,
                Ecosystem::Rust,
                vec![(CanonicalCommand::Test, "cargo test".into(), 10)],
            )),
        ];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "just test");
    }

    #[test]
    fn within_ecosystem_highest_priority_wins() {
        let groups = vec![
            group(mock(
                "pest",
                Tier::Tier4,
                Ecosystem::Php,
                vec![(CanonicalCommand::Test, "vendor/bin/pest".into(), 10)],
            )),
            group(mock(
                "phpunit",
                Tier::Tier4,
                Ecosystem::Php,
                vec![(CanonicalCommand::Test, "vendor/bin/phpunit".into(), 5)],
            )),
        ];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "vendor/bin/pest");
    }

    #[test]
    fn across_ecosystems_same_tier_all_run() {
        let groups = vec![
            group(mock(
                "cargo",
                Tier::Tier4,
                Ecosystem::Rust,
                vec![(CanonicalCommand::Test, "cargo test".into(), 10)],
            )),
            group(mock(
                "phpunit",
                Tier::Tier4,
                Ecosystem::Php,
                vec![(CanonicalCommand::Test, "vendor/bin/phpunit".into(), 10)],
            )),
        ];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 2);
        // Results sorted by ecosystem (JavaScript < Php < Rust < TaskRunner)
        assert_eq!(result[0].cmd, "vendor/bin/phpunit");
        assert_eq!(result[1].cmd, "cargo test");
    }

    #[test]
    fn zero_matches_returns_empty() {
        let groups = vec![group(mock(
            "cargo",
            Tier::Tier4,
            Ecosystem::Rust,
            vec![(CanonicalCommand::Build, "cargo build".into(), 10)],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert!(result.is_empty());
    }

    #[test]
    fn exact_script_beats_prefix_within_ecosystem() {
        // Simulates npm having both "test" (exact, priority 10) and "test:unit" (prefix, priority 5)
        let groups = vec![group(mock(
            "npm",
            Tier::Tier3,
            Ecosystem::JavaScript,
            vec![
                (CanonicalCommand::Test, "npm run test".into(), 10),
                (CanonicalCommand::Test, "npm run test:unit".into(), 5),
            ],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "npm run test");
        assert_eq!(result[0].priority, 10);
    }

    #[test]
    fn exclusive_group_first_match_wins() {
        // Two detectors in the same group: first one wins, second is skipped
        let groups = vec![DetectorGroup::new(vec![
            mock(
                "yarn",
                Tier::Tier3,
                Ecosystem::JavaScript,
                vec![(CanonicalCommand::Test, "yarn run test".into(), 10)],
            ),
            mock(
                "npm",
                Tier::Tier3,
                Ecosystem::JavaScript,
                vec![(CanonicalCommand::Test, "npm run test".into(), 10)],
            ),
        ])];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "yarn run test");
        assert_eq!(result[0].detector_name, "yarn");
    }

    #[test]
    fn canonical_command_from_str() {
        assert_eq!(
            "test".parse::<CanonicalCommand>().unwrap(),
            CanonicalCommand::Test
        );
        assert_eq!(
            "install".parse::<CanonicalCommand>().unwrap(),
            CanonicalCommand::Install
        );
        assert!("unknown".parse::<CanonicalCommand>().is_err());
    }

    #[test]
    fn canonical_name_exact_matches() {
        assert_eq!(map_canonical_name("test"), Some(CanonicalCommand::Test));
        assert_eq!(map_canonical_name("e2e"), Some(CanonicalCommand::E2e));
        assert_eq!(map_canonical_name("test:e2e"), Some(CanonicalCommand::E2e));
        assert_eq!(map_canonical_name("lint"), Some(CanonicalCommand::Lint));
        assert_eq!(map_canonical_name("check"), Some(CanonicalCommand::Lint));
        assert_eq!(map_canonical_name("analyse"), Some(CanonicalCommand::Lint));
        assert_eq!(map_canonical_name("analyze"), Some(CanonicalCommand::Lint));
        assert_eq!(
            map_canonical_name("typecheck"),
            Some(CanonicalCommand::Typecheck)
        );
        assert_eq!(
            map_canonical_name("type-check"),
            Some(CanonicalCommand::Typecheck)
        );
        assert_eq!(map_canonical_name("fix"), Some(CanonicalCommand::Fix));
        assert_eq!(map_canonical_name("lint:fix"), Some(CanonicalCommand::Fix));
        assert_eq!(map_canonical_name("lint-fix"), Some(CanonicalCommand::Fix));
        assert_eq!(map_canonical_name("format"), Some(CanonicalCommand::Format));
        assert_eq!(map_canonical_name("fmt"), Some(CanonicalCommand::Format));
        assert_eq!(map_canonical_name("build"), Some(CanonicalCommand::Build));
        assert_eq!(
            map_canonical_name("install"),
            Some(CanonicalCommand::Install)
        );
        assert_eq!(map_canonical_name("clean"), Some(CanonicalCommand::Clean));
        assert_eq!(map_canonical_name("unknown"), None);
    }

    #[test]
    fn script_name_exact_matches() {
        assert_eq!(
            map_script_name("test"),
            Some((CanonicalCommand::Test, ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("lint"),
            Some((CanonicalCommand::Lint, ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("check"),
            Some((CanonicalCommand::Lint, ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("format"),
            Some((CanonicalCommand::Format, ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("fmt"),
            Some((CanonicalCommand::Format, ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("build"),
            Some((CanonicalCommand::Build, ScriptMatchKind::Exact))
        );
    }

    #[test]
    fn script_name_prefix_matches() {
        assert_eq!(
            map_script_name("test:unit"),
            Some((CanonicalCommand::Test, ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("lint:fix"),
            Some((CanonicalCommand::Fix, ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("format:check"),
            Some((CanonicalCommand::Format, ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("typecheck:ci"),
            Some((CanonicalCommand::Typecheck, ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("type-check:strict"),
            Some((CanonicalCommand::Typecheck, ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("e2e:ui"),
            Some((CanonicalCommand::E2e, ScriptMatchKind::Prefix))
        );
        // "test:e2e:*" is claimed by E2e before the generic "test:" arm...
        assert_eq!(
            map_script_name("test:e2e:chrome"),
            Some((CanonicalCommand::E2e, ScriptMatchKind::Prefix))
        );
        // ...while other "test:*" names still map to Test.
        assert_eq!(
            map_script_name("test:unit"),
            Some((CanonicalCommand::Test, ScriptMatchKind::Prefix))
        );
    }

    #[test]
    fn script_name_no_match() {
        assert_eq!(map_script_name("contest"), None);
        assert_eq!(map_script_name("testing"), None);
        assert_eq!(map_script_name("dev"), None);
        assert_eq!(map_script_name("start"), None);
        assert_eq!(map_script_name("typechecker"), None);
        assert_eq!(map_script_name("e2etest"), None);
    }

    #[test]
    fn infer_php_tools() {
        assert_eq!(
            infer_from_command("phpunit").canonical(),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            infer_from_command("vendor/bin/pest").canonical(),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            infer_from_command("phpstan analyse").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("vendor/bin/phpstan analyse --level=max").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("psalm").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("phpcs").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("php-cs-fixer fix").canonical(),
            Some(CanonicalCommand::Format)
        );
        // --dry-run is a format-verify, so no canonical (see php_cs_fixer_dry_run_is_unclassified)
        assert_eq!(
            infer_from_command("php-cs-fixer fix --dry-run --diff").canonical(),
            None
        );
        assert_eq!(
            infer_from_command("vendor/bin/pint").canonical(),
            Some(CanonicalCommand::Format)
        );
        assert_eq!(
            infer_from_command("phpcbf").canonical(),
            Some(CanonicalCommand::Format)
        );
    }

    #[test]
    fn infer_js_tools() {
        assert_eq!(
            infer_from_command("jest").canonical(),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            infer_from_command("vitest run").canonical(),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            infer_from_command("mocha --reporter spec").canonical(),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            infer_from_command("eslint .").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("prettier --write .").canonical(),
            Some(CanonicalCommand::Format)
        );
        assert_eq!(
            infer_from_command("tsc --noEmit").canonical(),
            Some(CanonicalCommand::Typecheck)
        );
        assert_eq!(
            infer_from_command("tsc").canonical(),
            Some(CanonicalCommand::Build)
        );
        assert_eq!(
            infer_from_command("tsc -p tsconfig.json").canonical(),
            Some(CanonicalCommand::Build)
        );
    }

    #[test]
    fn infer_e2e_tools() {
        assert_eq!(
            infer_from_command("playwright test").canonical(),
            Some(CanonicalCommand::E2e)
        );
        assert_eq!(
            infer_from_command("cypress run").canonical(),
            Some(CanonicalCommand::E2e)
        );
        // Non-run subcommands stay unclassified: install/codegen aren't suite
        // runs, and `cypress open` is the interactive runner.
        assert_eq!(infer_from_command("playwright install"), Inference::Unknown);
        assert_eq!(infer_from_command("playwright codegen"), Inference::Unknown);
        assert_eq!(infer_from_command("cypress open"), Inference::Unknown);
        assert_eq!(infer_from_command("playwright"), Inference::Unknown);
    }

    #[test]
    fn infer_biome_subcommands() {
        assert_eq!(
            infer_from_command("biome check ."),
            Inference::Canonical(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("biome lint ."),
            Inference::Canonical(CanonicalCommand::Lint)
        );
        assert_eq!(infer_from_command("biome"), Inference::Unknown);
    }

    #[test]
    fn biome_format_without_write_is_format_verify() {
        // Bare `biome format` only reports unformatted files; it never writes,
        // so it cannot satisfy the Format canonical.
        assert_eq!(infer_from_command("biome format"), Inference::FormatVerify);
        assert_eq!(
            infer_from_command("biome format ."),
            Inference::FormatVerify
        );
        // `--write` is the standard write flag; `--fix` is its v2 alias.
        assert_eq!(
            infer_from_command("biome format --write"),
            Inference::Canonical(CanonicalCommand::Format)
        );
        assert_eq!(
            infer_from_command("biome format --fix ."),
            Inference::Canonical(CanonicalCommand::Format)
        );
    }

    #[test]
    fn infer_interpreter_prefixes() {
        assert_eq!(
            infer_from_command("php vendor/bin/phpstan analyse").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("node jest").canonical(),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            infer_from_command("npx eslint .").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("bunx vitest").canonical(),
            Some(CanonicalCommand::Test)
        );
    }

    #[test]
    fn infer_unknown_commands() {
        assert_eq!(infer_from_command("some-custom-script"), Inference::Unknown);
        assert_eq!(infer_from_command(""), Inference::Unknown);
        assert_eq!(infer_from_command("php"), Inference::Unknown);
    }

    #[test]
    fn map_script_name_and_content_agree() {
        assert_eq!(
            map_script("test", "phpunit"),
            Some((CanonicalCommand::Test, 10))
        );
        assert_eq!(
            map_script("lint", "eslint ."),
            Some((CanonicalCommand::Lint, 10))
        );
        assert_eq!(
            map_script("typecheck", "tsc --noEmit"),
            Some((CanonicalCommand::Typecheck, 10))
        );
        assert_eq!(
            map_script("e2e", "playwright test"),
            Some((CanonicalCommand::E2e, 10))
        );
    }

    #[test]
    fn test_named_script_running_playwright_is_e2e() {
        // A script named "test" that runs an e2e suite reclassifies to E2e at
        // the disagree priority; `letme test` must not trigger e2e runs.
        assert_eq!(
            map_script("test", "playwright test"),
            Some((CanonicalCommand::E2e, 3))
        );
    }

    #[test]
    fn map_script_name_and_content_disagree() {
        assert_eq!(
            map_script("format", "eslint ."),
            Some((CanonicalCommand::Lint, 3))
        );
    }

    #[test]
    fn php_cs_fixer_dry_run_is_unclassified() {
        // php-cs-fixer with --dry-run is a format-verify, not a lint and not a
        // (mutating) format.
        assert_eq!(
            infer_from_command("php-cs-fixer fix --dry-run --diff"),
            Inference::FormatVerify
        );
        // But a script *named* "lint" still resolves to Lint(10): a check-shaped
        // name is compatible with a format-verify, so map_script trusts the name.
        assert_eq!(
            map_script("lint", "php-cs-fixer fix --dry-run --diff"),
            Some((CanonicalCommand::Lint, 10))
        );
        // Without --dry-run it's still Format
        assert_eq!(
            infer_from_command("php-cs-fixer fix"),
            Inference::Canonical(CanonicalCommand::Format)
        );
    }

    #[test]
    fn pint_test_flag_is_unclassified() {
        // pint --test is a format-verify, not a lint.
        assert_eq!(infer_from_command("pint --test"), Inference::FormatVerify);
        assert_eq!(
            infer_from_command("pint"),
            Inference::Canonical(CanonicalCommand::Format)
        );
    }

    #[test]
    fn format_named_script_running_a_check_is_suppressed() {
        // A script named "format" that only verifies cannot satisfy the Format
        // canonical; leave the slot empty so a lower tier can supply a real
        // formatter (e.g. tier-4 `biome format --write`).
        assert_eq!(map_script("format", "biome format"), None);
        assert_eq!(map_script("format", "prettier --check ."), None);
        assert_eq!(map_script("format", "php-cs-fixer fix --dry-run"), None);
        assert_eq!(map_script("fmt", "pint --test"), None);
        // Prefix names too: "format:check" claims Format via prefix match.
        assert_eq!(map_script("format:check", "biome format"), None);
        // Same for a name claiming the other mutating canonical.
        assert_eq!(map_script("fix", "biome format"), None);
        // But a check-shaped name is fine, since it promises no mutation.
        assert_eq!(
            map_script("lint", "biome format"),
            Some((CanonicalCommand::Lint, 10))
        );
        assert_eq!(
            map_script("check", "prettier --check ."),
            Some((CanonicalCommand::Lint, 10))
        );
        // And a real write still resolves normally.
        assert_eq!(
            map_script("format", "biome format --write"),
            Some((CanonicalCommand::Format, 10))
        );
    }

    #[test]
    fn map_script_content_only() {
        // "analyse" is a name alias, so it gets the full name-match priority.
        assert_eq!(
            map_script("analyse", "phpstan analyse"),
            Some((CanonicalCommand::Lint, 10))
        );
        // "cs" matches no name; the content alone carries it at priority 7.
        assert_eq!(
            map_script("cs", "prettier --write ."),
            Some((CanonicalCommand::Format, 7))
        );
    }

    #[test]
    fn map_script_neither_matches() {
        assert_eq!(map_script("dev", "next dev"), None);
        assert_eq!(map_script("start", "node server.js"), None);
    }

    #[test]
    fn map_script_prefix_match() {
        assert_eq!(
            map_script("test:unit", "jest --unit"),
            Some((CanonicalCommand::Test, 5))
        );
    }

    #[test]
    fn map_script_name_only_no_content() {
        assert_eq!(
            map_script("test", "some-custom-runner"),
            Some((CanonicalCommand::Test, 10))
        );
    }

    #[test]
    fn check_missing_binaries_reports_missing() {
        let groups = vec![group(mock_with_binaries(
            "fake-tool",
            Tier::Tier2,
            Ecosystem::TaskRunner,
            vec![(CanonicalCommand::Test, "fake test".into(), 10)],
            &["__nonexistent_binary_letme_test__"],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let missing = check_missing_binaries(&groups, dir.path());

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].binary, "__nonexistent_binary_letme_test__");
        assert_eq!(missing[0].detector_name, "fake-tool");
    }

    #[test]
    fn check_missing_binaries_empty_when_available() {
        let groups = vec![group(mock_with_binaries(
            "shell",
            Tier::Tier4,
            Ecosystem::Rust,
            vec![(CanonicalCommand::Test, "sh -c test".into(), 10)],
            &["sh"],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let missing = check_missing_binaries(&groups, dir.path());

        assert!(missing.is_empty());
    }

    #[test]
    fn resolve_all_skips_detector_with_missing_binary() {
        let groups = vec![group(mock_with_binaries(
            "fake-tool",
            Tier::Tier4,
            Ecosystem::Rust,
            vec![(CanonicalCommand::Test, "fake test".into(), 10)],
            &["__nonexistent_binary_letme_test__"],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert!(result.is_empty());
    }

    #[test]
    fn script_name_fix_prefix() {
        assert_eq!(
            map_script_name("fix:lint"),
            Some((CanonicalCommand::Fix, ScriptMatchKind::Prefix))
        );
    }

    #[test]
    fn infer_eslint_fix() {
        assert_eq!(
            infer_from_command("eslint --fix .").canonical(),
            Some(CanonicalCommand::Fix)
        );
        // Without --fix, still Lint
        assert_eq!(
            infer_from_command("eslint .").canonical(),
            Some(CanonicalCommand::Lint)
        );
    }

    #[test]
    fn infer_biome_fix() {
        assert_eq!(
            infer_from_command("biome check --fix .").canonical(),
            Some(CanonicalCommand::Fix)
        );
        assert_eq!(
            infer_from_command("biome lint --apply .").canonical(),
            Some(CanonicalCommand::Fix)
        );
        assert_eq!(
            infer_from_command("biome lint --write .").canonical(),
            Some(CanonicalCommand::Fix)
        );
        // Without fix flags, still Lint
        assert_eq!(
            infer_from_command("biome check .").canonical(),
            Some(CanonicalCommand::Lint)
        );
    }

    #[test]
    fn map_script_fix_with_fix_content() {
        assert_eq!(
            map_script("fix", "eslint --fix ."),
            Some((CanonicalCommand::Fix, 10))
        );
    }

    #[test]
    fn split_compound_simple() {
        assert_eq!(split_compound_command("eslint ."), vec!["eslint ."]);
    }

    #[test]
    fn split_compound_and() {
        assert_eq!(
            split_compound_command("prettier --check . && eslint ."),
            vec!["prettier --check .", "eslint ."]
        );
    }

    #[test]
    fn split_compound_or() {
        assert_eq!(split_compound_command("cmd1 || cmd2"), vec!["cmd1", "cmd2"]);
    }

    #[test]
    fn split_compound_semicolon() {
        assert_eq!(split_compound_command("cmd1; cmd2"), vec!["cmd1", "cmd2"]);
    }

    #[test]
    fn split_compound_pipe_does_not_split() {
        // Pipes are part of a single logical command
        assert_eq!(
            split_compound_command("eslint . | tee output.log"),
            vec!["eslint . | tee output.log"]
        );
    }

    #[test]
    fn split_compound_respects_quotes() {
        assert_eq!(
            split_compound_command(r#"echo "a && b" && cmd2"#),
            vec![r#"echo "a && b""#, "cmd2"]
        );
        assert_eq!(
            split_compound_command("echo 'a && b' && cmd2"),
            vec!["echo 'a && b'", "cmd2"]
        );
    }

    #[test]
    fn infer_compound_all_agree() {
        assert_eq!(
            infer_from_command("prettier --write . && prettier --write src/").canonical(),
            Some(CanonicalCommand::Format)
        );
        assert_eq!(
            infer_from_command("eslint . && phpstan analyse").canonical(),
            Some(CanonicalCommand::Lint)
        );
    }

    #[test]
    fn infer_compound_prettier_check_and_eslint() {
        // prettier --check is a format-verify; a recognized canonical wins over
        // one, so only eslint's Lint carries.
        assert_eq!(
            infer_from_command("prettier --check . && eslint .").canonical(),
            Some(CanonicalCommand::Lint)
        );
    }

    #[test]
    fn infer_compound_all_format_verify() {
        // No canonical anywhere, but every recognized part is a format-verify, so
        // the compound is itself a format-verify and a "format" name is suppressed.
        assert_eq!(
            infer_from_command("prettier --check . && biome format"),
            Inference::FormatVerify
        );
        assert_eq!(
            map_script("format", "prettier --check . && biome format"),
            None
        );
    }

    #[test]
    fn infer_compound_disagree_returns_none() {
        // prettier --write is Format, eslint is Lint; they disagree.
        assert_eq!(
            infer_from_command("prettier --write . && eslint ."),
            Inference::Unknown
        );
    }

    #[test]
    fn infer_compound_with_unknown_parts() {
        // Only eslint is recognized, so its Lint carries.
        assert_eq!(
            infer_from_command("custom-tool && eslint .").canonical(),
            Some(CanonicalCommand::Lint)
        );
        // Nothing recognized at all.
        assert_eq!(
            infer_from_command("custom-tool && another-tool"),
            Inference::Unknown
        );
    }

    #[test]
    fn infer_single_command_unchanged() {
        // Regression: single commands still work
        assert_eq!(
            infer_from_command("eslint .").canonical(),
            Some(CanonicalCommand::Lint)
        );
        assert_eq!(
            infer_from_command("prettier --write .").canonical(),
            Some(CanonicalCommand::Format)
        );
        assert_eq!(infer_from_command(""), Inference::Unknown);
    }

    #[test]
    fn map_script_compound_lint_prettier_and_eslint() {
        // The format-verify part is invisible, so content agrees with the name.
        assert_eq!(
            map_script("lint", "prettier --check . && eslint"),
            Some((CanonicalCommand::Lint, 10))
        );
    }

    #[test]
    fn map_script_compound_lint_npx_prettier_and_eslint() {
        // Same with npx prefix
        assert_eq!(
            map_script("lint", "npx prettier . --check && eslint"),
            Some((CanonicalCommand::Lint, 10))
        );
    }
}
