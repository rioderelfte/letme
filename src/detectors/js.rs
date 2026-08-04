//! Shared knowledge about the JavaScript package-manager landscape.
//!
//! Three separate concerns need to answer "which package manager owns this
//! directory?": the tier 3 detector group ordering, the tier 4 exec prefix, and
//! the conflicting-lockfile warning. They all read the precedence table below so
//! the answer can never drift between them. Adding bun later is a single row.

use std::fmt;
use std::path::Path;

/// A JavaScript package manager letme knows how to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsPackageManager {
    Pnpm,
    Yarn,
    Npm,
}

/// Which lockfile belongs to which manager, in precedence order (first match wins).
///
/// This ordering is the same invariant the tier 3 JS detector group encodes in
/// [`crate::detectors::all_detectors`]; keep the two in sync.
const LOCKFILES: &[(&str, JsPackageManager)] = &[
    ("pnpm-lock.yaml", JsPackageManager::Pnpm),
    ("yarn.lock", JsPackageManager::Yarn),
    ("package-lock.json", JsPackageManager::Npm),
];

impl JsPackageManager {
    /// The lockfile that identifies this manager.
    pub fn lockfile(&self) -> &'static str {
        match self {
            Self::Pnpm => "pnpm-lock.yaml",
            Self::Yarn => "yarn.lock",
            Self::Npm => "package-lock.json",
        }
    }

    /// Command that installs dependencies.
    pub fn install_command(&self) -> &'static str {
        match self {
            Self::Pnpm => "pnpm install",
            Self::Yarn => "yarn install",
            Self::Npm => "npm install",
        }
    }

    /// Prefix for running a binary out of `node_modules/.bin`.
    ///
    /// Bare `yarn <bin>` rather than `yarn exec <bin>`, because yarn 1.x has no `exec`
    /// subcommand, while the bare form works under both classic and berry.
    pub fn exec_prefix(&self) -> &'static str {
        match self {
            Self::Pnpm => "pnpm exec",
            Self::Yarn => "yarn",
            Self::Npm => "npx",
        }
    }
}

impl fmt::Display for JsPackageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pnpm => write!(f, "pnpm"),
            Self::Yarn => write!(f, "yarn"),
            Self::Npm => write!(f, "npm"),
        }
    }
}

/// Every manager whose lockfile is present, in precedence order.
pub fn present_managers(dir: &Path) -> Vec<JsPackageManager> {
    LOCKFILES
        .iter()
        .filter(|(lockfile, _)| dir.join(lockfile).exists())
        .map(|(_, manager)| *manager)
        .collect()
}

/// The manager that owns this directory.
///
/// Highest-precedence lockfile wins; a bare `package.json` falls back to npm.
pub fn detect_manager(dir: &Path) -> JsPackageManager {
    present_managers(dir)
        .first()
        .copied()
        .unwrap_or(JsPackageManager::Npm)
}

/// Print a warning if more than one JS lockfile is present.
pub fn warn_conflicting_lockfiles(dir: &Path, theme: &crate::theme::Theme) {
    let managers = present_managers(dir);
    if managers.len() < 2 {
        return;
    }

    use owo_colors::OwoColorize;
    eprintln!(
        "{} {}",
        "Warning:".style(theme.warning),
        format!(
            "{} found; using {}.",
            join_lockfiles(&managers),
            managers[0]
        )
        .style(theme.muted)
    );
}

/// Render lockfile names as a human list: "a and b", "a, b and c".
fn join_lockfiles(managers: &[JsPackageManager]) -> String {
    let names: Vec<&str> = managers.iter().map(|m| m.lockfile()).collect();
    match names.split_last() {
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "").unwrap();
    }

    #[test]
    fn detects_npm_from_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "package-lock.json");

        assert_eq!(detect_manager(dir.path()), JsPackageManager::Npm);
    }

    #[test]
    fn pnpm_wins_over_yarn_and_npm() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "package-lock.json");
        touch(dir.path(), "yarn.lock");
        touch(dir.path(), "pnpm-lock.yaml");

        assert_eq!(detect_manager(dir.path()), JsPackageManager::Pnpm);
    }

    #[test]
    fn yarn_wins_over_npm() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "package-lock.json");
        touch(dir.path(), "yarn.lock");

        assert_eq!(detect_manager(dir.path()), JsPackageManager::Yarn);
    }

    #[test]
    fn falls_back_to_npm_without_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "package.json");

        assert_eq!(detect_manager(dir.path()), JsPackageManager::Npm);
    }

    #[test]
    fn present_managers_lists_all_in_precedence_order() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "package-lock.json");
        touch(dir.path(), "pnpm-lock.yaml");

        assert_eq!(
            present_managers(dir.path()),
            vec![JsPackageManager::Pnpm, JsPackageManager::Npm]
        );
    }

    #[test]
    fn present_managers_is_empty_without_lockfiles() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "package.json");

        assert!(present_managers(dir.path()).is_empty());
    }

    #[test]
    fn joins_two_lockfiles() {
        assert_eq!(
            join_lockfiles(&[JsPackageManager::Pnpm, JsPackageManager::Npm]),
            "pnpm-lock.yaml and package-lock.json"
        );
    }

    #[test]
    fn joins_three_lockfiles() {
        assert_eq!(
            join_lockfiles(&[
                JsPackageManager::Pnpm,
                JsPackageManager::Yarn,
                JsPackageManager::Npm
            ]),
            "pnpm-lock.yaml, yarn.lock and package-lock.json"
        );
    }

    #[test]
    fn lockfile_table_and_accessor_agree() {
        for (lockfile, manager) in LOCKFILES {
            assert_eq!(manager.lockfile(), *lockfile);
        }
    }
}
