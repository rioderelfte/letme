//! Binary-level tests for what main.rs wires together: help routing, exit
//! codes and request dispatch. The grammar itself is unit-tested in
//! grammar.rs and config.rs. Each test runs the compiled binary against an
//! empty project directory with an isolated `XDG_CONFIG_HOME`, so behavior
//! here never depends on detection or the user's own config.toml.

use std::path::Path;
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str], cwd: &Path, config_home: &Path) -> Out {
    let output = Command::new(env!("CARGO_BIN_EXE_letme"))
        .args(args)
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run letme binary");

    Out {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn env_dirs() -> (tempfile::TempDir, tempfile::TempDir) {
    (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap())
}

fn with_config(config_home: &Path, contents: &str) {
    let dir = config_home.join("letme");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), contents).unwrap();
}

#[test]
fn every_rejection_uses_the_same_exit_code() {
    // Only a command letme ran can exit with something other than 1.
    let (cwd, cfg) = env_dirs();
    with_config(
        cfg.path(),
        "[aliases]\nok = [\"lint\"]\ndr = [\"doctor\"]\n",
    );
    for args in [
        vec!["--"],                    // reserved token
        vec!["-"],                     // lone dash
        vec!["--check"],               // flag before any name
        vec!["--interactive=x"],       // global with a value
        vec!["format", "--check=all"], // segment flag with a value
        vec!["test", "--check"],       // flag the name doesn't take
        vec!["ok", "--fix"],           // alias with a flag
        vec!["dr"],                    // standalone inside an alias
        vec!["doctor", "test"],        // argument a standalone doesn't take
        vec!["test", "doctor"],        // standalone after another command
        vec!["nonsense"],              // unknown name
        vec!["t"],                     // ambiguous prefix
        vec!["test"],                  // nothing detected here
    ] {
        let out = run(&args, cwd.path(), cfg.path());
        assert_eq!(out.code, 1, "for {args:?}: {}", out.stderr);
        assert!(
            out.stderr.starts_with("Error: "),
            "for {args:?}: {}",
            out.stderr
        );
    }
}

#[test]
fn prefix_resolves_before_flag_validation() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["te", "--check"], cwd.path(), cfg.path());
    assert_eq!(out.code, 1);
    assert!(
        out.stderr
            .contains("test does not take flags (got --check)"),
        "got: {}",
        out.stderr
    );
}

#[test]
fn lint_fix_reaches_detection() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["lint", "--fix"], cwd.path(), cfg.path());
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("no lint --fix command detected"),
        "got: {}",
        out.stderr
    );
}

#[test]
fn lint_help_shows_lints_own_page() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["lint", "--help"], cwd.path(), cfg.path());
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.starts_with("Usage: letme lint [--fix]"),
        "got: {}",
        out.stdout
    );
}

#[test]
fn prefix_resolves_before_help_routing() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["te", "--help"], cwd.path(), cfg.path());
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.starts_with("Usage: letme test"),
        "got: {}",
        out.stdout
    );
}

#[test]
fn top_level_help_shows_the_command_block() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["--help"], cwd.path(), cfg.path());
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("Canonical commands (chainable):"));
    assert!(out.stdout.contains("Standalone commands:"));
}

#[test]
fn version_flag_prints_the_crate_version() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["--version"], cwd.path(), cfg.path());
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, format!("letme {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn doctor_runs_alone() {
    let (cwd, cfg) = env_dirs();
    let out = run(&["doctor"], cwd.path(), cfg.path());
    assert_eq!(out.code, 0, "got: {}", out.stderr);
    assert!(
        out.stdout.contains("No health checks applicable"),
        "got: {}",
        out.stdout
    );
}

#[test]
fn no_arguments_shows_info_view() {
    let (cwd, cfg) = env_dirs();
    let out = run(&[], cwd.path(), cfg.path());
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("No ecosystems detected"));
}
