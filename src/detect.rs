use std::fmt;
use std::path::Path;

use crate::theme::sanitize;

/// Canonical commands that letme understands. Declaration order is display
/// order, for the help page and the info view alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CanonicalCommand {
    Install,
    Test,
    E2e,
    Lint,
    Typecheck,
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

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Test => "test",
            Self::E2e => "e2e",
            Self::Lint => "lint",
            Self::Typecheck => "typecheck",
            Self::Format => "format",
            Self::Build => "build",
            Self::Clean => "clean",
        }
    }

    /// Parse an exact name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all().iter().copied().find(|c| c.as_str() == name)
    }
}

impl fmt::Display for CanonicalCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A modifier that refines a canonical command's detection key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Modifier {
    Check,
    Fix,
}

impl Modifier {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Check => "--check",
            Self::Fix => "--fix",
        }
    }
}

/// The detection key: a canonical command plus at most one modifier.
///
/// Ordering is canonical declaration order, then plain before variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandKey {
    pub canonical: CanonicalCommand,
    pub modifier: Option<Modifier>,
}

impl CommandKey {
    /// Every (canonical, modifier) pair letme understands: what detectors can
    /// resolve, and what the CLI offers as a flag on the canonical's name.
    pub const VARIANTS: &'static [CommandKey] = &[
        CanonicalCommand::Lint.with(Modifier::Fix),
        CanonicalCommand::Format.with(Modifier::Check),
    ];

    /// Every valid detection key: each canonical's plain key, then the
    /// variants. Feeds the info view.
    pub fn all() -> Vec<CommandKey> {
        CanonicalCommand::all()
            .iter()
            .copied()
            .map(CommandKey::from)
            .chain(Self::VARIANTS.iter().copied())
            .collect()
    }

    /// The variants of one canonical, in [`VARIANTS`](Self::VARIANTS) order.
    pub fn variants_of(canonical: CanonicalCommand) -> impl Iterator<Item = CommandKey> {
        Self::VARIANTS
            .iter()
            .copied()
            .filter(move |k| k.canonical == canonical)
    }
}

impl From<CanonicalCommand> for CommandKey {
    fn from(canonical: CanonicalCommand) -> Self {
        Self {
            canonical,
            modifier: None,
        }
    }
}

impl CanonicalCommand {
    /// This canonical with a modifier attached, e.g. `Format.with(Check)` for
    /// `format --check`.
    pub const fn with(self, modifier: Modifier) -> CommandKey {
        CommandKey {
            canonical: self,
            modifier: Some(modifier),
        }
    }
}

impl fmt::Display for CommandKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical)?;
        if let Some(m) = self.modifier {
            write!(f, " {}", m.flag())?;
        }
        Ok(())
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
    pub key: CommandKey,
    pub cmd: String,
    pub label: String,
    pub tier: Tier,
    pub ecosystem: Ecosystem,
    pub detector_name: String,
    pub priority: u32,
    /// Covered by the plain key of this canonical, same detector (`cargo check`
    /// covered by `lint`, not by `lint --fix`).
    pub covered_by: Option<CanonicalCommand>,
    /// Provenance for a command that isn't simply "what the detector found",
    /// e.g. a synthesized variant. Shown by the verbose `resolved →` trace.
    pub note: Option<String>,
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
    ///
    /// `Self: Sized` keeps this generic method out of the vtable.
    fn make_command(
        &self,
        key: impl Into<CommandKey>,
        cmd: String,
        priority: u32,
    ) -> ResolvedCommand
    where
        Self: Sized,
    {
        ResolvedCommand {
            key: key.into(),
            label: cmd.clone(),
            cmd,
            tier: self.tier(),
            ecosystem: self.ecosystem(),
            detector_name: self.name().into(),
            priority,
            covered_by: None,
            note: None,
        }
    }

    /// Helper for a command another canonical command of this detector already
    /// covers, like `cargo check` under `cargo clippy`.
    fn make_covered_command(
        &self,
        key: impl Into<CommandKey>,
        cmd: String,
        priority: u32,
        covered_by: CanonicalCommand,
    ) -> ResolvedCommand
    where
        Self: Sized,
    {
        ResolvedCommand {
            covered_by: Some(covered_by),
            ..self.make_command(key, cmd, priority)
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

/// Render a `ResolvedCommand`'s note as a verbose-trace suffix, e.g.
/// `" (synthesized from 'format' script)"`. The note contains repo-controlled
/// text, so it is sanitized.
fn verbose_note_suffix(note: Option<&str>) -> String {
    note.map(|n| format!(" ({})", sanitize(n)))
        .unwrap_or_default()
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
    commands: &[CommandKey],
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

    // Variants never resolve below the best tier of their plain key. With no
    // plain entry anywhere for this canonical, the variant is unrestricted.
    let plain_tier = |canonical: CanonicalCommand| -> Option<Tier> {
        all.iter()
            .filter(|r| r.key.canonical == canonical && r.key.modifier.is_none())
            .map(|r| r.tier)
            .min()
    };

    let mut result = Vec::new();

    for &key in commands {
        let matches: Vec<&ResolvedCommand> = all.iter().filter(|r| r.key == key).collect();
        if matches.is_empty() {
            continue;
        }

        let best_tier = if key.modifier.is_none() {
            matches.iter().map(|r| r.tier).min()
        } else {
            match plain_tier(key.canonical) {
                Some(base) => {
                    let restricted = matches
                        .iter()
                        .filter(|r| r.tier <= base)
                        .map(|r| r.tier)
                        .min();
                    if restricted.is_none() && verbose {
                        let owner = all.iter().find(|r| {
                            r.key.canonical == key.canonical
                                && r.key.modifier.is_none()
                                && r.tier == base
                        });
                        if let Some(owner) = owner {
                            let word = key
                                .modifier
                                .map(|m| m.flag().trim_start_matches("--"))
                                .unwrap_or_default();
                            eprintln!(
                                "[verbose] {key}: only detected below {}'s tier ({base}, {}); not detected. Declare a {}-{word} recipe/task or {}:{word} script at that tier",
                                key.canonical, owner.detector_name, key.canonical, key.canonical
                            );
                        }
                    }
                    restricted
                }
                None => matches.iter().map(|r| r.tier).min(),
            }
        };

        let Some(best_tier) = best_tier else {
            continue;
        };

        if verbose && matches.iter().any(|r| r.tier != best_tier) {
            eprintln!("[verbose] {key}: tier {best_tier} overrides lower-priority tiers");
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
                            "[verbose] {key}: '{}' (priority {}) beaten by '{}' (priority {}) in {}",
                            sanitize(&r.cmd),
                            r.priority,
                            sanitize(&existing.cmd),
                            existing.priority,
                            r.ecosystem
                        );
                    }
                }
                _ => {
                    if verbose && let Some(old) = seen_ecosystems.get(&r.ecosystem) {
                        eprintln!(
                            "[verbose] {key}: '{}' (priority {}) replaces '{}' (priority {}) in {}",
                            sanitize(&r.cmd),
                            r.priority,
                            sanitize(&old.cmd),
                            old.priority,
                            r.ecosystem
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
                let note = verbose_note_suffix(r.note.as_deref());
                eprintln!(
                    "[verbose] {key}: resolved → '{}' [{}]{note}",
                    sanitize(&r.cmd),
                    r.detector_name
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
    /// Recognized as a specific detection key.
    Canonical(CommandKey),
    /// Recognized as a check-only formatter run (`prettier --check`,
    /// `php-cs-fixer --dry-run`, `pint --test`, `biome format` without a write
    /// flag). Unlike `Canonical`, a check-shaped name (`lint`, `check`,
    /// `format:check`) wins over it; see [`combine`].
    FormatVerify,
    /// Not recognized.
    Unknown,
}

impl Inference {
    /// The key, if one was recognized outright. A format-verify yields `None`
    /// here: its key depends on the name it is combined with.
    pub fn key(self) -> Option<CommandKey> {
        match self {
            Self::Canonical(key) => Some(key),
            Self::FormatVerify | Self::Unknown => None,
        }
    }
}

/// Launchers that resolve a tool themselves; the tool is the token after one
/// of these, not the token itself.
pub const INTERPRETERS: &[&str] = &["php", "node", "npx", "bunx"];

/// Index of the tool token in `tokens`, skipping one interpreter prefix.
/// `None` when there is no token left to name a tool.
pub fn tool_index(tokens: &[&str]) -> Option<usize> {
    let start = usize::from(INTERPRETERS.contains(&basename(tokens.first()?)));
    (start < tokens.len()).then_some(start)
}

/// Map a name to a detection key using exact matching only.
///
/// Covers all standard aliases: "test", "lint", "check", "format", "fmt", etc.
pub fn map_canonical_name(name: &str) -> Option<CommandKey> {
    match name {
        "test" => Some(CanonicalCommand::Test.into()),
        "e2e" | "test:e2e" => Some(CanonicalCommand::E2e.into()),
        "fix" | "lint:fix" | "lint-fix" => Some(CanonicalCommand::Lint.with(Modifier::Fix)),
        "lint" | "check" | "analyse" | "analyze" => Some(CanonicalCommand::Lint.into()),
        "typecheck" | "type-check" => Some(CanonicalCommand::Typecheck.into()),
        "format" | "fmt" => Some(CanonicalCommand::Format.into()),
        "format:check" | "format-check" | "fmt:check" | "fmt-check" => {
            Some(CanonicalCommand::Format.with(Modifier::Check))
        }
        "build" => Some(CanonicalCommand::Build.into()),
        "install" => Some(CanonicalCommand::Install.into()),
        "clean" => Some(CanonicalCommand::Clean.into()),
        _ => None,
    }
}

/// Map a script name to a detection key using exact + prefix matching.
///
/// Matches: "test" and "test:unit" map to Test; "e2e", "test:e2e" and
/// "test:e2e:*" map to E2e (e2e suites must not hide behind `letme test`).
/// Does NOT match: "contest", "testing".
/// Returns the match kind so callers can assign different priorities.
pub fn map_script_name(name: &str) -> Option<(CommandKey, ScriptMatchKind)> {
    if let Some(key) = map_canonical_name(name) {
        return Some((key, ScriptMatchKind::Exact));
    }

    // Prefix matches (name starts with "test:", "lint:", etc.)
    // "test:e2e:" must be claimed before the generic "test:" arm.
    if name.starts_with("test:e2e:") || name.starts_with("e2e:") {
        return Some((CanonicalCommand::E2e.into(), ScriptMatchKind::Prefix));
    }
    if name.starts_with("test:") {
        return Some((CanonicalCommand::Test.into(), ScriptMatchKind::Prefix));
    }
    if name.starts_with("fix:") {
        return Some((
            CanonicalCommand::Lint.with(Modifier::Fix),
            ScriptMatchKind::Prefix,
        ));
    }
    // "lint:fix:"/"lint-fix:" must be claimed before the generic "lint:" arm.
    if name.starts_with("lint:fix:") || name.starts_with("lint-fix:") {
        return Some((
            CanonicalCommand::Lint.with(Modifier::Fix),
            ScriptMatchKind::Prefix,
        ));
    }
    if name.starts_with("lint:") {
        return Some((CanonicalCommand::Lint.into(), ScriptMatchKind::Prefix));
    }
    if name.starts_with("typecheck:") || name.starts_with("type-check:") {
        return Some((CanonicalCommand::Typecheck.into(), ScriptMatchKind::Prefix));
    }
    // "format:check:"/"fmt:check:"/"format-check:"/"fmt-check:" must be
    // claimed before the generic "format:"/"fmt:" arm.
    if name.starts_with("format:check:")
        || name.starts_with("fmt:check:")
        || name.starts_with("format-check:")
        || name.starts_with("fmt-check:")
    {
        return Some((
            CanonicalCommand::Format.with(Modifier::Check),
            ScriptMatchKind::Prefix,
        ));
    }
    if name.starts_with("format:") || name.starts_with("fmt:") {
        return Some((CanonicalCommand::Format.into(), ScriptMatchKind::Prefix));
    }
    if name.starts_with("build:") {
        return Some((CanonicalCommand::Build.into(), ScriptMatchKind::Prefix));
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

    let mut result: Option<CommandKey> = None;
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
    let Some(start) = tool_index(&parts) else {
        return Inference::Unknown;
    };

    let tool = basename(parts[start]);
    // Check for biome/playwright/cypress subcommands
    let subcommand = parts.get(start + 1).copied();

    let args = &parts[start + 1..];

    match tool {
        // Test
        "phpunit" | "pest" | "jest" | "vitest" | "mocha" => {
            Inference::Canonical(CanonicalCommand::Test.into())
        }
        // E2e: only an actual suite run counts; `playwright install`/`codegen`
        // and the interactive `cypress open` stay unclassified
        "playwright" => match subcommand {
            Some("test") => Inference::Canonical(CanonicalCommand::E2e.into()),
            _ => Inference::Unknown,
        },
        "cypress" => match subcommand {
            Some("run") => Inference::Canonical(CanonicalCommand::E2e.into()),
            _ => Inference::Unknown,
        },
        // Lint / Fix
        "phpstan" | "psalm" | "phpcs" => Inference::Canonical(CanonicalCommand::Lint.into()),
        "eslint" | "oxlint" => {
            if args.contains(&"--fix") {
                Inference::Canonical(CanonicalCommand::Lint.with(Modifier::Fix))
            } else {
                Inference::Canonical(CanonicalCommand::Lint.into())
            }
        }
        // Format (with dry-run detection for check-only mode)
        "php-cs-fixer" => {
            if args.contains(&"--dry-run") {
                Inference::FormatVerify
            } else {
                Inference::Canonical(CanonicalCommand::Format.into())
            }
        }
        "pint" => {
            if args.contains(&"--test") {
                Inference::FormatVerify
            } else {
                Inference::Canonical(CanonicalCommand::Format.into())
            }
        }
        "prettier" => {
            if args.contains(&"--check") {
                Inference::FormatVerify
            } else {
                Inference::Canonical(CanonicalCommand::Format.into())
            }
        }
        "phpcbf" => Inference::Canonical(CanonicalCommand::Format.into()),
        // Build / Typecheck: bare tsc emits JS, --noEmit only checks types
        "tsc" => {
            if args.contains(&"--noEmit") {
                Inference::Canonical(CanonicalCommand::Typecheck.into())
            } else {
                Inference::Canonical(CanonicalCommand::Build.into())
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
                    Inference::Canonical(CanonicalCommand::Lint.with(Modifier::Fix))
                } else {
                    Inference::Canonical(CanonicalCommand::Lint.into())
                }
            }
            Some("format") => {
                let biome_args = &parts[start + 2..];
                // `--write` is Biome's standard write flag; `--fix` is its v2
                // alias. `--apply` (accepted by check/lint) was never valid
                // for `format`. Bare `biome format` only reports.
                if biome_args.iter().any(|a| *a == "--write" || *a == "--fix") {
                    Inference::Canonical(CanonicalCommand::Format.into())
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
    split_compound_command_with_ops(cmd).0
}

/// One operator joining two parts of a compound command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    And,
    Or,
    Semicolon,
}

impl CompoundOp {
    fn as_str(self) -> &'static str {
        match self {
            Self::And => "&&",
            Self::Or => "||",
            Self::Semicolon => ";",
        }
    }
}

/// Like [`split_compound_command`], but also returns the operator between
/// each pair of segments, so the whole can be rejoined faithfully
/// (`ops.len() == parts.len() - 1` for any non-empty result).
fn split_compound_command_with_ops(cmd: &str) -> (Vec<&str>, Vec<CompoundOp>) {
    fn push_segment<'a>(
        cmd: &'a str,
        start: usize,
        end: usize,
        op: Option<CompoundOp>,
        parts: &mut Vec<&'a str>,
        ops: &mut Vec<CompoundOp>,
    ) {
        let segment = cmd[start..end].trim();
        if !segment.is_empty() {
            parts.push(segment);
            if let Some(op) = op {
                ops.push(op);
            }
        }
    }

    let mut parts = Vec::new();
    let mut ops = Vec::new();
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
                push_segment(cmd, start, i, Some(CompoundOp::And), &mut parts, &mut ops);
                i += 2;
                start = i;
            }
            b'|' if !in_single_quote && !in_double_quote && i + 1 < len && bytes[i + 1] == b'|' => {
                push_segment(cmd, start, i, Some(CompoundOp::Or), &mut parts, &mut ops);
                i += 2;
                start = i;
            }
            b';' if !in_single_quote && !in_double_quote => {
                push_segment(
                    cmd,
                    start,
                    i,
                    Some(CompoundOp::Semicolon),
                    &mut parts,
                    &mut ops,
                );
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    push_segment(cmd, start, len, None, &mut parts, &mut ops);

    // A trailing separator (`"prettier -w .;"`) pushes an operator whose
    // right-hand segment turned out empty and was dropped; without this, the
    // invariant below would be off by one.
    ops.truncate(parts.len().saturating_sub(1));

    (parts, ops)
}

/// Extract the basename (filename) from a potentially path-like string.
pub fn basename(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// Whether a key claims to write (mutate) files: plain `format`, or `lint
/// --fix`.
fn is_write_claiming(key: CommandKey) -> bool {
    key == CommandKey::from(CanonicalCommand::Format)
        || key == CanonicalCommand::Lint.with(Modifier::Fix)
}

/// Combine a name match with content inspection into a key and priority.
/// When they agree the name's priority applies (exact 10, prefix 5); content
/// wins a disagreement at 3 and resolves on its own at 7.
pub fn combine(
    name_match: Option<(CommandKey, ScriptMatchKind)>,
    content_match: Inference,
) -> Option<(CommandKey, u32)> {
    match (name_match, content_match) {
        // A check-only format run cannot satisfy a name that claims to write,
        // so it rehomes to the check variant. A check-shaped name (`lint`,
        // `check`, `format:check`) is trusted by the arm below.
        (Some((key, _)), Inference::FormatVerify) if is_write_claiming(key) => {
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
        }
        // Name matches, content unclassified: trust the name
        (Some((name_key, kind)), Inference::FormatVerify | Inference::Unknown) => {
            Some((name_key, kind.priority()))
        }
        // Name and content agree: trust the name
        (Some((name_key, kind)), Inference::Canonical(content_key)) if name_key == content_key => {
            Some((name_key, kind.priority()))
        }
        // Name and content disagree: trust the content
        (Some(_), Inference::Canonical(content_key)) => Some((content_key, 3)),
        // No name match, content matches a canonical
        (None, Inference::Canonical(content_key)) => Some((content_key, 7)),
        // No name match, content is a check-only format run: still real signal
        (None, Inference::FormatVerify) => {
            Some((CanonicalCommand::Format.with(Modifier::Check), 7))
        }
        // Neither matches
        (None, Inference::Unknown) => None,
    }
}

/// Map a script by combining name matching with content inspection.
pub fn map_script(name: &str, command: &str) -> Option<(CommandKey, u32)> {
    combine(map_script_name(name), infer_from_command(command))
}

/// A tier-3 variant synthesized from a plain script's body by rewriting the
/// tool invocation itself, rather than relying on an explicit variant script.
pub struct SynthesizedVariant {
    pub key: CommandKey,
    parts: Vec<String>,
    ops: Vec<CompoundOp>,
}

impl SynthesizedVariant {
    /// The synthesized parts, in order.
    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    /// Join the parts with their original operators, passing each part
    /// through `f` first (e.g. to prefix it with a package manager's exec
    /// command).
    pub fn render(&self, mut f: impl FnMut(&str) -> String) -> String {
        let mut out = String::new();
        for (i, part) in self.parts.iter().enumerate() {
            if i > 0 {
                out.push(' ');
                out.push_str(self.ops[i - 1].as_str());
                out.push(' ');
            }
            out.push_str(&f(part));
        }
        out
    }
}

/// Synthesize `target` from a plain script's body by rewriting the tool
/// invocation itself, e.g. `format: "prettier -w src"` synthesizes
/// `format --check` running `prettier --check src`.
///
/// Every compound part must be eligible (see [`eligible_part`]) and at least
/// one part must actually be transformed, or the whole body is declined.
pub fn synthesize_variant(target: CommandKey, body: &str) -> Option<SynthesizedVariant> {
    let (segments, ops) = split_compound_command_with_ops(body);

    let mut parts = Vec::with_capacity(segments.len());
    let mut any_transformed = false;

    for segment in &segments {
        match eligible_part(target, segment)? {
            PartOutcome::Transformed(rendered) => {
                any_transformed = true;
                parts.push(rendered);
            }
            PartOutcome::PassThrough => parts.push((*segment).to_string()),
        }
    }

    if !any_transformed {
        return None;
    }

    Some(SynthesizedVariant {
        key: target,
        parts,
        ops,
    })
}

enum PartOutcome {
    Transformed(String),
    PassThrough,
}

/// How one compound part takes part in synthesizing `target`: rewritten if it
/// is the plain form of `target`'s canonical and has a known rewrite, passed
/// through if it doesn't write files. `None` declines the whole body.
fn eligible_part(target: CommandKey, segment: &str) -> Option<PartOutcome> {
    let inference = infer_single_command(segment);

    if inference == Inference::Canonical(target) {
        return None;
    }

    if inference == Inference::Canonical(CommandKey::from(target.canonical))
        && let Some(rendered) = transform(target, segment)
    {
        return Some(PartOutcome::Transformed(rendered));
    }

    let passes_through = inference == Inference::FormatVerify
        || inference == Inference::Canonical(CanonicalCommand::Lint.into())
        || inference == Inference::Canonical(CanonicalCommand::Typecheck.into());

    if passes_through {
        return Some(PartOutcome::PassThrough);
    }

    None
}

/// Token-level rewrite of one plain-content part into `target`, for the tools
/// with a known transformation. Preserves the part's own paths/flags;
/// inserted flags land right after the tool token (and its subcommand, where
/// one is required), so shapes match the tier-4 emissions.
fn transform(target: CommandKey, segment: &str) -> Option<String> {
    let tokens: Vec<&str> = segment.split_whitespace().collect();
    let start = tool_index(&tokens)?;
    let tool = basename(tokens[start]);

    if target == CanonicalCommand::Format.with(Modifier::Check) {
        match tool {
            "prettier" => {
                let mut out: Vec<String> = tokens
                    .iter()
                    .filter(|t| **t != "--write" && **t != "-w")
                    .map(|t| (*t).to_string())
                    .collect();
                out.insert((start + 1).min(out.len()), "--check".to_string());
                Some(out.join(" "))
            }
            "biome" => {
                let out: Vec<String> = tokens
                    .iter()
                    .filter(|t| **t != "--write" && **t != "--fix")
                    .map(|t| (*t).to_string())
                    .collect();
                Some(out.join(" "))
            }
            "php-cs-fixer" => {
                // Search from after the tool token so a `fix` argument
                // (rather than the subcommand) can't be mistaken for it.
                let fix_pos = start + 1 + tokens[start + 1..].iter().position(|t| *t == "fix")?;
                let mut out: Vec<String> = tokens.iter().map(|t| (*t).to_string()).collect();
                out.splice(
                    fix_pos + 1..fix_pos + 1,
                    ["--dry-run".to_string(), "--diff".to_string()],
                );
                Some(out.join(" "))
            }
            "pint" => {
                let mut out: Vec<String> = tokens.iter().map(|t| (*t).to_string()).collect();
                out.push("--test".to_string());
                Some(out.join(" "))
            }
            _ => None,
        }
    } else if target == CanonicalCommand::Lint.with(Modifier::Fix) {
        match tool {
            // eslint/oxlint take a bare `--fix`; biome's fix flag lands right
            // after its subcommand (`check`/`lint`, guaranteed present here
            // since that's the only shape content inference classifies as
            // plain Lint).
            "eslint" | "oxlint" => {
                let mut out: Vec<String> = tokens.iter().map(|t| (*t).to_string()).collect();
                out.insert((start + 1).min(out.len()), "--fix".to_string());
                Some(out.join(" "))
            }
            "biome" => {
                let mut out: Vec<String> = tokens.iter().map(|t| (*t).to_string()).collect();
                out.insert((start + 2).min(out.len()), "--fix".to_string());
                Some(out.join(" "))
            }
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDetector {
        name: &'static str,
        tier: Tier,
        ecosystem: Ecosystem,
        commands: Vec<(CommandKey, String, u32)>,
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
                .map(|(key, cmd, priority)| self.make_command(*key, cmd.clone(), *priority))
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
        mock_keyed(
            name,
            tier,
            ecosystem,
            commands
                .into_iter()
                .map(|(c, cmd, p)| (c.into(), cmd, p))
                .collect(),
        )
    }

    /// Like [`mock`], but for commands keyed by a full (possibly variant)
    /// `CommandKey`.
    fn mock_keyed(
        name: &'static str,
        tier: Tier,
        ecosystem: Ecosystem,
        commands: Vec<(CommandKey, String, u32)>,
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
            commands: commands
                .into_iter()
                .map(|(c, cmd, p)| (c.into(), cmd, p))
                .collect(),
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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

        assert_eq!(result.len(), 2);
        // Results sorted by ecosystem (JavaScript < Php < Rust < TaskRunner)
        assert_eq!(result[0].cmd, "vendor/bin/phpunit");
        assert_eq!(result[1].cmd, "cargo test");
    }

    #[test]
    fn verbose_note_suffix_sanitizes_hostile_text() {
        // A note is built from repo-controlled text (a script name); a
        // hostile one must not smuggle terminal escapes onto stderr.
        let note = verbose_note_suffix(Some("synthesized from 'fmtx\x1b[31mEVIL\x1b[0m' script"));
        assert!(!note.contains('\u{1b}'), "got: {note}");
        assert!(note.contains('\u{FFFD}'), "got: {note}");
    }

    #[test]
    fn verbose_note_suffix_is_empty_for_none() {
        assert_eq!(verbose_note_suffix(None), "");
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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

        assert!(result.is_empty());
    }

    #[test]
    fn variant_below_the_plain_keys_tier_is_suppressed() {
        // Justfile owns plain `format` at tier 2, so a tier-4 convention's
        // `format --check` must not fill the variant.
        let groups = vec![
            group(mock(
                "justfile",
                Tier::Tier2,
                Ecosystem::TaskRunner,
                vec![(CanonicalCommand::Format, "just format".into(), 10)],
            )),
            group(mock_keyed(
                "cargo",
                Tier::Tier4,
                Ecosystem::Rust,
                vec![(
                    CanonicalCommand::Format.with(Modifier::Check),
                    "cargo fmt --check".into(),
                    10,
                )],
            )),
        ];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(
            &groups,
            dir.path(),
            &[CanonicalCommand::Format.with(Modifier::Check)],
            false,
        );

        assert!(result.is_empty());
    }

    #[test]
    fn variant_at_or_above_the_plain_keys_tier_competes_normally() {
        // Both plain `format` and `format --check` come from the same tier;
        // the variant resolves normally, uninhibited by the base-tier rule.
        let groups = vec![group(mock_keyed(
            "cargo",
            Tier::Tier4,
            Ecosystem::Rust,
            vec![
                (CanonicalCommand::Format.into(), "cargo fmt".into(), 10),
                (
                    CanonicalCommand::Format.with(Modifier::Check),
                    "cargo fmt --check".into(),
                    10,
                ),
            ],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(
            &groups,
            dir.path(),
            &[CanonicalCommand::Format.with(Modifier::Check)],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "cargo fmt --check");
    }

    #[test]
    fn variant_with_no_plain_entry_anywhere_resolves_unrestricted() {
        // A variant-only project (no plain `format` detected at all) is not
        // restricted by a base tier that doesn't exist.
        let groups = vec![group(mock_keyed(
            "cargo",
            Tier::Tier4,
            Ecosystem::Rust,
            vec![(
                CanonicalCommand::Format.with(Modifier::Check),
                "cargo fmt --check".into(),
                10,
            )],
        ))];

        let dir = tempfile::tempdir().unwrap();
        let result = resolve_all(
            &groups,
            dir.path(),
            &[CanonicalCommand::Format.with(Modifier::Check)],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "cargo fmt --check");
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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "yarn run test");
        assert_eq!(result[0].detector_name, "yarn");
    }

    #[test]
    fn canonical_command_from_name() {
        assert_eq!(
            CanonicalCommand::from_name("test"),
            Some(CanonicalCommand::Test)
        );
        assert_eq!(
            CanonicalCommand::from_name("install"),
            Some(CanonicalCommand::Install)
        );
        assert_eq!(CanonicalCommand::from_name("unknown"), None);
        assert_eq!(CanonicalCommand::from_name("fix"), None);
    }

    #[test]
    fn command_key_all_lists_plain_keys_then_variants() {
        let all = CommandKey::all();
        assert_eq!(all.len(), CanonicalCommand::all().len() + 2);
        assert_eq!(all[0], CanonicalCommand::Install.into());
        assert!(
            all[..CanonicalCommand::all().len()]
                .iter()
                .all(|k| k.modifier.is_none())
        );
        assert_eq!(&all[CanonicalCommand::all().len()..], CommandKey::VARIANTS);
    }

    #[test]
    fn variants_of_filters_by_canonical() {
        let format: Vec<_> = CommandKey::variants_of(CanonicalCommand::Format).collect();
        assert_eq!(format, vec![CanonicalCommand::Format.with(Modifier::Check)]);
        assert_eq!(CommandKey::variants_of(CanonicalCommand::Test).count(), 0);
    }

    #[test]
    fn keys_sort_plain_before_variant_in_declaration_order() {
        let mut keys = [
            CanonicalCommand::Format.with(Modifier::Check),
            CommandKey::from(CanonicalCommand::Build),
            CommandKey::from(CanonicalCommand::Format),
            CanonicalCommand::Lint.with(Modifier::Fix),
        ];
        keys.sort();
        assert_eq!(
            keys.iter().map(CommandKey::to_string).collect::<Vec<_>>(),
            ["lint --fix", "format", "format --check", "build"]
        );
    }

    #[test]
    fn tool_index_skips_one_interpreter() {
        assert_eq!(tool_index(&["eslint", "."]), Some(0));
        assert_eq!(tool_index(&["npx", "eslint", "."]), Some(1));
        assert_eq!(tool_index(&["/usr/bin/php", "vendor/bin/pest"]), Some(1));
        // An interpreter with nothing to launch names no tool.
        assert_eq!(tool_index(&["npx"]), None);
        assert_eq!(tool_index(&[]), None);
    }

    #[test]
    fn canonical_name_exact_matches() {
        assert_eq!(
            map_canonical_name("test"),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            map_canonical_name("e2e"),
            Some(CanonicalCommand::E2e.into())
        );
        assert_eq!(
            map_canonical_name("test:e2e"),
            Some(CanonicalCommand::E2e.into())
        );
        assert_eq!(
            map_canonical_name("lint"),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            map_canonical_name("check"),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            map_canonical_name("analyse"),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            map_canonical_name("analyze"),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            map_canonical_name("typecheck"),
            Some(CanonicalCommand::Typecheck.into())
        );
        assert_eq!(
            map_canonical_name("type-check"),
            Some(CanonicalCommand::Typecheck.into())
        );
        assert_eq!(
            map_canonical_name("fix"),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        assert_eq!(
            map_canonical_name("lint:fix"),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        assert_eq!(
            map_canonical_name("lint-fix"),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        assert_eq!(
            map_canonical_name("format"),
            Some(CanonicalCommand::Format.into())
        );
        assert_eq!(
            map_canonical_name("fmt"),
            Some(CanonicalCommand::Format.into())
        );
        for name in ["format:check", "format-check", "fmt:check", "fmt-check"] {
            assert_eq!(
                map_canonical_name(name),
                Some(CanonicalCommand::Format.with(Modifier::Check)),
                "{name}"
            );
        }
        assert_eq!(
            map_canonical_name("build"),
            Some(CanonicalCommand::Build.into())
        );
        assert_eq!(
            map_canonical_name("install"),
            Some(CanonicalCommand::Install.into())
        );
        assert_eq!(
            map_canonical_name("clean"),
            Some(CanonicalCommand::Clean.into())
        );
        assert_eq!(map_canonical_name("unknown"), None);
    }

    #[test]
    fn script_name_exact_matches() {
        assert_eq!(
            map_script_name("test"),
            Some((CanonicalCommand::Test.into(), ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("lint"),
            Some((CanonicalCommand::Lint.into(), ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("check"),
            Some((CanonicalCommand::Lint.into(), ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("format"),
            Some((CanonicalCommand::Format.into(), ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("fmt"),
            Some((CanonicalCommand::Format.into(), ScriptMatchKind::Exact))
        );
        assert_eq!(
            map_script_name("build"),
            Some((CanonicalCommand::Build.into(), ScriptMatchKind::Exact))
        );
    }

    #[test]
    fn script_name_prefix_matches() {
        assert_eq!(
            map_script_name("test:unit"),
            Some((CanonicalCommand::Test.into(), ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("lint:fix"),
            Some((
                CanonicalCommand::Lint.with(Modifier::Fix),
                ScriptMatchKind::Exact
            ))
        );
        // "format:check" is an exact name for the variant itself.
        assert_eq!(
            map_script_name("format:check"),
            Some((
                CanonicalCommand::Format.with(Modifier::Check),
                ScriptMatchKind::Exact
            ))
        );
        // "format:check:*"/"fmt:check:*"/"format-check:*"/"fmt-check:*" are
        // claimed as prefixes of the variant, before the generic "format:" arm.
        for name in [
            "format:check:strict",
            "fmt:check:strict",
            "format-check:strict",
            "fmt-check:strict",
        ] {
            assert_eq!(
                map_script_name(name),
                Some((
                    CanonicalCommand::Format.with(Modifier::Check),
                    ScriptMatchKind::Prefix
                )),
                "{name}"
            );
        }
        assert_eq!(
            map_script_name("typecheck:ci"),
            Some((CanonicalCommand::Typecheck.into(), ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("type-check:strict"),
            Some((CanonicalCommand::Typecheck.into(), ScriptMatchKind::Prefix))
        );
        assert_eq!(
            map_script_name("e2e:ui"),
            Some((CanonicalCommand::E2e.into(), ScriptMatchKind::Prefix))
        );
        // "test:e2e:*" is claimed by E2e before the generic "test:" arm...
        assert_eq!(
            map_script_name("test:e2e:chrome"),
            Some((CanonicalCommand::E2e.into(), ScriptMatchKind::Prefix))
        );
        // ...while other "test:*" names still map to Test.
        assert_eq!(
            map_script_name("test:unit"),
            Some((CanonicalCommand::Test.into(), ScriptMatchKind::Prefix))
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
            infer_from_command("phpunit").key(),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            infer_from_command("vendor/bin/pest").key(),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            infer_from_command("phpstan analyse").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("vendor/bin/phpstan analyse --level=max").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("psalm").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("phpcs").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("php-cs-fixer fix").key(),
            Some(CanonicalCommand::Format.into())
        );
        // --dry-run is a format-verify, so no canonical (see php_cs_fixer_dry_run_is_unclassified)
        assert_eq!(
            infer_from_command("php-cs-fixer fix --dry-run --diff").key(),
            None
        );
        assert_eq!(
            infer_from_command("vendor/bin/pint").key(),
            Some(CanonicalCommand::Format.into())
        );
        assert_eq!(
            infer_from_command("phpcbf").key(),
            Some(CanonicalCommand::Format.into())
        );
    }

    #[test]
    fn infer_js_tools() {
        assert_eq!(
            infer_from_command("jest").key(),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            infer_from_command("vitest run").key(),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            infer_from_command("mocha --reporter spec").key(),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            infer_from_command("eslint .").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("prettier --write .").key(),
            Some(CanonicalCommand::Format.into())
        );
        assert_eq!(
            infer_from_command("tsc --noEmit").key(),
            Some(CanonicalCommand::Typecheck.into())
        );
        assert_eq!(
            infer_from_command("tsc").key(),
            Some(CanonicalCommand::Build.into())
        );
        assert_eq!(
            infer_from_command("tsc -p tsconfig.json").key(),
            Some(CanonicalCommand::Build.into())
        );
    }

    #[test]
    fn infer_e2e_tools() {
        assert_eq!(
            infer_from_command("playwright test").key(),
            Some(CanonicalCommand::E2e.into())
        );
        assert_eq!(
            infer_from_command("cypress run").key(),
            Some(CanonicalCommand::E2e.into())
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
            Inference::Canonical(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("biome lint ."),
            Inference::Canonical(CanonicalCommand::Lint.into())
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
            Inference::Canonical(CanonicalCommand::Format.into())
        );
        assert_eq!(
            infer_from_command("biome format --fix ."),
            Inference::Canonical(CanonicalCommand::Format.into())
        );
    }

    #[test]
    fn infer_interpreter_prefixes() {
        assert_eq!(
            infer_from_command("php vendor/bin/phpstan analyse").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("node jest").key(),
            Some(CanonicalCommand::Test.into())
        );
        assert_eq!(
            infer_from_command("npx eslint .").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("bunx vitest").key(),
            Some(CanonicalCommand::Test.into())
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
            Some((CanonicalCommand::Test.into(), 10))
        );
        assert_eq!(
            map_script("lint", "eslint ."),
            Some((CanonicalCommand::Lint.into(), 10))
        );
        assert_eq!(
            map_script("typecheck", "tsc --noEmit"),
            Some((CanonicalCommand::Typecheck.into(), 10))
        );
        assert_eq!(
            map_script("e2e", "playwright test"),
            Some((CanonicalCommand::E2e.into(), 10))
        );
    }

    #[test]
    fn test_named_script_running_playwright_is_e2e() {
        // A script named "test" that runs an e2e suite reclassifies to E2e at
        // the disagree priority; `letme test` must not trigger e2e runs.
        assert_eq!(
            map_script("test", "playwright test"),
            Some((CanonicalCommand::E2e.into(), 3))
        );
    }

    #[test]
    fn map_script_name_and_content_disagree() {
        assert_eq!(
            map_script("format", "eslint ."),
            Some((CanonicalCommand::Lint.into(), 3))
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
            Some((CanonicalCommand::Lint.into(), 10))
        );
        // Without --dry-run it's still Format
        assert_eq!(
            infer_from_command("php-cs-fixer fix"),
            Inference::Canonical(CanonicalCommand::Format.into())
        );
    }

    #[test]
    fn pint_test_flag_is_unclassified() {
        // pint --test is a format-verify, not a lint.
        assert_eq!(infer_from_command("pint --test"), Inference::FormatVerify);
        assert_eq!(
            infer_from_command("pint"),
            Inference::Canonical(CanonicalCommand::Format.into())
        );
    }

    #[test]
    fn format_named_script_running_a_check_is_rehomed_to_the_variant() {
        // A script named "format" that only verifies cannot satisfy plain
        // Format, but the content really is a check: rehome to the variant
        // instead of dropping it, at the disagree-with-name priority.
        assert_eq!(
            map_script("format", "biome format"),
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
        );
        assert_eq!(
            map_script("format", "prettier --check ."),
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
        );
        assert_eq!(
            map_script("format", "php-cs-fixer fix --dry-run"),
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
        );
        assert_eq!(
            map_script("fmt", "pint --test"),
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
        );
        // "format:check" is an exact name for the variant itself, so name and
        // content agree at the full exact priority.
        assert_eq!(
            map_script("format:check", "biome format"),
            Some((CanonicalCommand::Format.with(Modifier::Check), 10))
        );
        // "fix" also claims to write (via lint --fix); a check-only body
        // rehomes there too, same as plain "format".
        assert_eq!(
            map_script("fix", "biome format"),
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
        );
        // A check-shaped name is fine either way, since it promises no mutation.
        assert_eq!(
            map_script("lint", "biome format"),
            Some((CanonicalCommand::Lint.into(), 10))
        );
        assert_eq!(
            map_script("check", "prettier --check ."),
            Some((CanonicalCommand::Lint.into(), 10))
        );
        // And a real write still resolves normally.
        assert_eq!(
            map_script("format", "biome format --write"),
            Some((CanonicalCommand::Format.into(), 10))
        );
    }

    #[test]
    fn map_script_content_only() {
        // "analyse" is a name alias, so it gets the full name-match priority.
        assert_eq!(
            map_script("analyse", "phpstan analyse"),
            Some((CanonicalCommand::Lint.into(), 10))
        );
        // "cs" matches no name; the content alone carries it at priority 7.
        assert_eq!(
            map_script("cs", "prettier --write ."),
            Some((CanonicalCommand::Format.into(), 7))
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
            Some((CanonicalCommand::Test.into(), 5))
        );
    }

    #[test]
    fn map_script_name_only_no_content() {
        assert_eq!(
            map_script("test", "some-custom-runner"),
            Some((CanonicalCommand::Test.into(), 10))
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
        let result = resolve_all(&groups, dir.path(), &[CanonicalCommand::Test.into()], false);

        assert!(result.is_empty());
    }

    #[test]
    fn script_name_fix_prefix() {
        assert_eq!(
            map_script_name("fix:lint"),
            Some((
                CanonicalCommand::Lint.with(Modifier::Fix),
                ScriptMatchKind::Prefix
            ))
        );
    }

    #[test]
    fn script_name_lint_fix_prefix() {
        for name in ["lint:fix:strict", "lint-fix:strict"] {
            assert_eq!(
                map_script_name(name),
                Some((
                    CanonicalCommand::Lint.with(Modifier::Fix),
                    ScriptMatchKind::Prefix
                )),
                "{name}"
            );
        }
    }

    #[test]
    fn infer_eslint_fix() {
        assert_eq!(
            infer_from_command("eslint --fix .").key(),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        // Without --fix, still Lint
        assert_eq!(
            infer_from_command("eslint .").key(),
            Some(CanonicalCommand::Lint.into())
        );
    }

    #[test]
    fn infer_oxlint_fix() {
        assert_eq!(
            infer_from_command("oxlint --fix").key(),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        // Without --fix, still Lint
        assert_eq!(
            infer_from_command("oxlint").key(),
            Some(CanonicalCommand::Lint.into())
        );
    }

    #[test]
    fn infer_biome_fix() {
        assert_eq!(
            infer_from_command("biome check --fix .").key(),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        assert_eq!(
            infer_from_command("biome lint --apply .").key(),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        assert_eq!(
            infer_from_command("biome lint --write .").key(),
            Some(CanonicalCommand::Lint.with(Modifier::Fix))
        );
        // Without fix flags, still Lint
        assert_eq!(
            infer_from_command("biome check .").key(),
            Some(CanonicalCommand::Lint.into())
        );
    }

    #[test]
    fn map_script_fix_with_fix_content() {
        assert_eq!(
            map_script("fix", "eslint --fix ."),
            Some((CanonicalCommand::Lint.with(Modifier::Fix), 10))
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
    fn split_compound_with_ops_trailing_separator_keeps_the_invariant() {
        // A dangling operator (its right-hand segment is empty) is dropped,
        // so `ops.len() == parts.len() - 1` holds even here.
        let (parts, ops) = split_compound_command_with_ops("prettier -w .;");
        assert_eq!(parts, vec!["prettier -w ."]);
        assert_eq!(ops.len(), parts.len() - 1);
        assert!(ops.is_empty());
    }

    #[test]
    fn split_compound_with_ops_matches_parts_for_a_normal_chain() {
        let (parts, ops) = split_compound_command_with_ops("a && b || c");
        assert_eq!(parts, vec!["a", "b", "c"]);
        assert_eq!(ops, vec![CompoundOp::And, CompoundOp::Or]);
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
            infer_from_command("prettier --write . && prettier --write src/").key(),
            Some(CanonicalCommand::Format.into())
        );
        assert_eq!(
            infer_from_command("eslint . && phpstan analyse").key(),
            Some(CanonicalCommand::Lint.into())
        );
    }

    #[test]
    fn infer_compound_prettier_check_and_eslint() {
        // prettier --check is a format-verify; a recognized canonical wins over
        // one, so only eslint's Lint carries.
        assert_eq!(
            infer_from_command("prettier --check . && eslint .").key(),
            Some(CanonicalCommand::Lint.into())
        );
    }

    #[test]
    fn infer_compound_all_format_verify() {
        // No canonical anywhere, but every recognized part is a format-verify, so
        // the compound is itself a format-verify, which rehomes a plain
        // "format" name to the check variant.
        assert_eq!(
            infer_from_command("prettier --check . && biome format"),
            Inference::FormatVerify
        );
        assert_eq!(
            map_script("format", "prettier --check . && biome format"),
            Some((CanonicalCommand::Format.with(Modifier::Check), 3))
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
            infer_from_command("custom-tool && eslint .").key(),
            Some(CanonicalCommand::Lint.into())
        );
        // Nothing recognized at all.
        assert_eq!(
            infer_from_command("custom-tool && another-tool"),
            Inference::Unknown
        );
    }

    #[test]
    fn infer_single_command_unchanged() {
        assert_eq!(
            infer_from_command("eslint .").key(),
            Some(CanonicalCommand::Lint.into())
        );
        assert_eq!(
            infer_from_command("prettier --write .").key(),
            Some(CanonicalCommand::Format.into())
        );
        assert_eq!(infer_from_command(""), Inference::Unknown);
    }

    #[test]
    fn map_script_compound_lint_prettier_and_eslint() {
        // The format-verify part is invisible, so content agrees with the name.
        assert_eq!(
            map_script("lint", "prettier --check . && eslint"),
            Some((CanonicalCommand::Lint.into(), 10))
        );
    }

    #[test]
    fn map_script_compound_lint_npx_prettier_and_eslint() {
        // Same with npx prefix
        assert_eq!(
            map_script("lint", "npx prettier . --check && eslint"),
            Some((CanonicalCommand::Lint.into(), 10))
        );
    }

    fn synth(body: &str) -> Option<String> {
        synthesize_variant(CanonicalCommand::Format.with(Modifier::Check), body)
            .map(|v| v.render(|part| part.to_string()))
    }

    #[test]
    fn synthesize_prettier_preserves_paths_and_flags() {
        assert_eq!(
            synth("prettier -w src"),
            Some("prettier --check src".to_string())
        );
        assert_eq!(synth("prettier"), Some("prettier --check".to_string()));
    }

    #[test]
    fn synthesize_pass_through_lets_a_plain_lint_part_ride_along() {
        assert_eq!(
            synth("prettier -w . && eslint ."),
            Some("prettier --check . && eslint .".to_string())
        );
    }

    #[test]
    fn synthesize_declines_a_writing_foreign_part() {
        assert_eq!(synth("prettier -w . && eslint --fix ."), None);
    }

    #[test]
    fn synthesize_declines_when_nothing_is_transformed() {
        // "phpstan analyse" is plain Lint, which passes through, but there is
        // no Format part to transform.
        assert_eq!(synth("phpstan analyse"), None);
    }

    #[test]
    fn synthesize_declines_unknown_content() {
        assert_eq!(synth("next lint"), None);
    }

    #[test]
    fn synthesize_declines_a_standalone_check_with_nothing_to_transform() {
        assert_eq!(synth("prettier --check ."), None);
    }

    #[test]
    fn synthesize_biome_format_drops_the_write_flag() {
        assert_eq!(
            synth("biome format --write"),
            Some("biome format".to_string())
        );
        assert_eq!(
            synth("biome format --fix ."),
            Some("biome format .".to_string())
        );
    }

    #[test]
    fn synthesize_php_cs_fixer_inserts_dry_run_and_diff_after_fix() {
        assert_eq!(
            synth("vendor/bin/php-cs-fixer fix"),
            Some("vendor/bin/php-cs-fixer fix --dry-run --diff".to_string())
        );
    }

    #[test]
    fn synthesize_php_cs_fixer_finds_fix_after_an_interpreter_prefix() {
        // The subcommand search starts after the tool token, so it still
        // finds "fix" when the tool itself is interpreter-prefixed.
        assert_eq!(
            synth("php vendor/bin/php-cs-fixer fix"),
            Some("php vendor/bin/php-cs-fixer fix --dry-run --diff".to_string())
        );
    }

    #[test]
    fn synthesize_php_cs_fixer_declines_without_a_fix_subcommand() {
        assert_eq!(synth("vendor/bin/php-cs-fixer"), None);
    }

    #[test]
    fn synthesize_pint_appends_test_flag() {
        assert_eq!(synth("pint"), Some("pint --test".to_string()));
    }

    #[test]
    fn synthesize_declines_a_composite_that_is_already_the_variant() {
        // Every part already checks: nothing to transform, so this declines
        // the same way a fully-satisfied body does.
        assert_eq!(synth("prettier --check . && pint --test"), None);
    }

    fn synth_fix(body: &str) -> Option<String> {
        synthesize_variant(CanonicalCommand::Lint.with(Modifier::Fix), body)
            .map(|v| v.render(|part| part.to_string()))
    }

    #[test]
    fn synthesize_lint_fix_mirrors_the_script() {
        // No "." added: the script is mirrored exactly, matching tier 4's
        // own template only when the script already has one.
        assert_eq!(synth_fix("eslint"), Some("eslint --fix".to_string()));
        assert_eq!(synth_fix("oxlint"), Some("oxlint --fix".to_string()));
    }

    #[test]
    fn synthesize_lint_fix_pass_through_lets_a_format_verify_part_ride_along() {
        assert_eq!(
            synth_fix("eslint . && prettier --check ."),
            Some("eslint --fix . && prettier --check .".to_string())
        );
    }

    #[test]
    fn synthesize_lint_fix_pass_through_lets_typecheck_ride_along() {
        assert_eq!(
            synth_fix("eslint . && tsc --noEmit"),
            Some("eslint --fix . && tsc --noEmit".to_string())
        );
    }

    #[test]
    fn synthesize_lint_fix_declines_a_writing_foreign_part() {
        // "format" (plain, writing) is not eligible to ride along under
        // Lint→Fix, unlike under Format→Check where Lint rides along.
        assert_eq!(synth_fix("eslint . && prettier -w ."), None);
    }

    #[test]
    fn synthesize_biome_lint_fix_inserts_after_the_subcommand() {
        assert_eq!(
            synth_fix("biome check ."),
            Some("biome check --fix .".to_string())
        );
        assert_eq!(
            synth_fix("biome lint"),
            Some("biome lint --fix".to_string())
        );
    }

    #[test]
    fn synthesize_lint_fix_declines_cross_binary_rewrites() {
        assert_eq!(synth_fix("phpstan analyse"), None);
        assert_eq!(synth_fix("next lint"), None);
        assert_eq!(synth_fix("eslint --fix ."), None);
    }
}
