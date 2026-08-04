use anyhow::Result;
use owo_colors::OwoColorize;
use std::path::Path;

use crate::detect;
use crate::detectors;
use crate::detectors::js::{self, JsPackageManager};
use crate::theme::Theme;

struct Check {
    name: String,
    status: CheckStatus,
    message: String,
    fix_command: Option<String>,
}

enum CheckStatus {
    Pass,
    Fail,
}

pub fn run(dir: &Path, theme: &Theme) -> Result<bool> {
    let checks = run_all_checks(dir);

    if checks.is_empty() {
        println!(
            "{}",
            "No health checks applicable for this project.".style(theme.muted)
        );
        return Ok(true);
    }

    let mut has_failure = false;
    for check in &checks {
        match check.status {
            CheckStatus::Pass => {
                println!(
                    "  {} {:<25} {}",
                    "✓".style(theme.success),
                    check.name.style(theme.success),
                    check.message.style(theme.muted),
                );
            }
            CheckStatus::Fail => {
                has_failure = true;
                let fix = check
                    .fix_command
                    .as_ref()
                    .map(|f| format!(" → {f}"))
                    .unwrap_or_default();
                println!(
                    "  {} {:<25} {}{}",
                    "✗".style(theme.error),
                    check.name.style(theme.error),
                    check.message,
                    fix.style(theme.hint),
                );
            }
        }
    }

    Ok(!has_failure)
}

fn run_all_checks(dir: &Path) -> Vec<Check> {
    let mut checks = Vec::new();

    check_required_binaries(dir, &mut checks);
    check_node_modules(dir, &mut checks);
    check_composer_vendor(dir, &mut checks);
    check_env_file(dir, &mut checks);

    checks
}

fn check_required_binaries(dir: &Path, checks: &mut Vec<Check>) {
    let groups = detectors::all_detectors();
    let missing = detect::check_missing_binaries(&groups, dir);
    for m in missing {
        checks.push(Check {
            name: m.binary.clone(),
            status: CheckStatus::Fail,
            message: "not installed".into(),
            fix_command: None,
        });
    }
}

fn check_node_modules(dir: &Path, checks: &mut Vec<Check>) {
    let manager = js::detect_manager(dir);
    let lock = dir.join(manager.lockfile());
    if !lock.exists() {
        // detect_manager falls back to npm even without a lockfile, and
        // without one there is nothing to compare against.
        return;
    }

    if !dir.join("node_modules").exists() {
        checks.push(Check {
            name: "node_modules".into(),
            status: CheckStatus::Fail,
            message: "missing".into(),
            fix_command: Some(manager.install_command().into()),
        });
        return;
    }

    let marker = dir.join(install_marker(dir, manager));
    if !marker.exists() {
        return; // can't tell how node_modules was produced, skip
    }

    if is_stale(&lock, &marker) {
        checks.push(Check {
            name: "node_modules".into(),
            status: CheckStatus::Fail,
            message: "out of date".into(),
            fix_command: Some(manager.install_command().into()),
        });
    } else {
        checks.push(Check {
            name: "node_modules".into(),
            status: CheckStatus::Pass,
            message: "up to date".into(),
            fix_command: None,
        });
    }
}

/// The file the package manager rewrites inside node_modules on every install,
/// used as the "installed at" timestamp to compare against the lockfile.
fn install_marker(dir: &Path, manager: JsPackageManager) -> &'static str {
    match manager {
        JsPackageManager::Pnpm => "node_modules/.modules.yaml",
        JsPackageManager::Npm => "node_modules/.package-lock.json",
        JsPackageManager::Yarn => {
            if dir.join("node_modules/.yarn-state.yml").exists() {
                "node_modules/.yarn-state.yml" // berry
            } else {
                "node_modules/.yarn-integrity" // classic
            }
        }
    }
}

fn check_composer_vendor(dir: &Path, checks: &mut Vec<Check>) {
    let lock = dir.join("composer.lock");
    let marker = dir.join("vendor/composer/installed.json");

    if !dir.join("composer.json").exists() {
        return;
    }

    if !dir.join("vendor").exists() {
        checks.push(Check {
            name: "vendor/".into(),
            status: CheckStatus::Fail,
            message: "missing".into(),
            fix_command: Some("composer install".into()),
        });
        return;
    }

    if lock.exists() && marker.exists() {
        if is_stale(&lock, &marker) {
            checks.push(Check {
                name: "vendor/".into(),
                status: CheckStatus::Fail,
                message: "out of date".into(),
                fix_command: Some("composer install".into()),
            });
        } else {
            checks.push(Check {
                name: "vendor/".into(),
                status: CheckStatus::Pass,
                message: "up to date".into(),
                fix_command: None,
            });
        }
    }
}

fn check_env_file(dir: &Path, checks: &mut Vec<Check>) {
    let env = dir.join(".env");
    let example = dir.join(".env.example");

    if !example.exists() {
        return;
    }

    if env.exists() {
        checks.push(Check {
            name: ".env".into(),
            status: CheckStatus::Pass,
            message: "present".into(),
            fix_command: None,
        });
    } else {
        checks.push(Check {
            name: ".env".into(),
            status: CheckStatus::Fail,
            message: "missing".into(),
            fix_command: Some("cp .env.example .env".into()),
        });
    }
}

/// Check if `source` has been modified more recently than `marker`.
fn is_stale(source: &Path, marker: &Path) -> bool {
    let source_mtime = std::fs::metadata(source).and_then(|m| m.modified()).ok();
    let marker_mtime = std::fs::metadata(marker).and_then(|m| m.modified()).ok();

    match (source_mtime, marker_mtime) {
        (Some(src), Some(mrk)) => src > mrk,
        (Some(_), None) => true, // marker missing counts as stale
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    fn backdate(path: &Path) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
    }

    fn node_modules_checks(dir: &Path) -> Vec<Check> {
        let mut checks = Vec::new();
        check_node_modules(dir, &mut checks);
        checks
    }

    #[test]
    fn missing_node_modules_suggests_install() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let checks = node_modules_checks(dir.path());

        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, CheckStatus::Fail));
        assert_eq!(checks[0].message, "missing");
        assert_eq!(checks[0].fix_command.as_deref(), Some("pnpm install"));
    }

    #[test]
    fn no_lockfile_yields_no_check() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        assert!(node_modules_checks(dir.path()).is_empty());
    }

    #[test]
    fn lockfile_newer_than_install_is_out_of_date() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        let marker = dir.path().join("node_modules/.modules.yaml");
        fs::write(&marker, "").unwrap();
        backdate(&marker);
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let checks = node_modules_checks(dir.path());

        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, CheckStatus::Fail));
        assert_eq!(checks[0].message, "out of date");
    }

    #[test]
    fn fresh_install_passes() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("pnpm-lock.yaml");
        fs::write(&lock, "").unwrap();
        backdate(&lock);
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/.modules.yaml"), "").unwrap();

        let checks = node_modules_checks(dir.path());

        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, CheckStatus::Pass));
    }

    #[test]
    fn unrecognized_node_modules_is_skipped() {
        // node_modules exists but carries no marker for the detected manager,
        // so there is no way to tell when it was installed. No verdict.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();

        assert!(node_modules_checks(dir.path()).is_empty());
    }

    #[test]
    fn yarn_berry_marker_wins_over_classic() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();

        assert_eq!(
            install_marker(dir.path(), JsPackageManager::Yarn),
            "node_modules/.yarn-integrity"
        );

        fs::write(dir.path().join("node_modules/.yarn-state.yml"), "").unwrap();
        assert_eq!(
            install_marker(dir.path(), JsPackageManager::Yarn),
            "node_modules/.yarn-state.yml"
        );
    }

    #[test]
    fn is_stale_when_marker_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("some.lock");
        fs::write(&lock, "").unwrap();

        assert!(is_stale(&lock, &dir.path().join("missing")));
    }
}
