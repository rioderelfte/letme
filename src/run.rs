use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::cli::Cli;
use crate::config::Config;
use crate::detect::{self, CanonicalCommand, DetectorGroup, ResolvedCommand};
use crate::doctor;
use crate::local_config::{FILE_NAME, LocalConfig};
use crate::summary::{self, Outcome, SummaryRow};
use crate::theme::{Theme, sanitize};

/// Error indicating a subprocess exited with a specific code.
#[derive(Debug)]
pub struct CommandExit(pub i32);

impl std::fmt::Display for CommandExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "process exited with code {}", self.0)
    }
}

impl std::error::Error for CommandExit {}

/// One unit of work in a run, in request order.
#[derive(Debug)]
enum PlanEntry<'a> {
    Doctor,
    NotDetected(CanonicalCommand),
    Disabled(CanonicalCommand),
    Exec(&'a ResolvedCommand),
}

pub fn run(
    dir: &Path,
    groups: &[DetectorGroup],
    cli: &Cli,
    theme: &Theme,
    config: &Config,
    local: &LocalConfig,
) -> Result<()> {
    let expanded = config
        .expand_aliases(&cli.commands)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let canonicals: Vec<CanonicalCommand> = expanded
        .iter()
        .filter(|n| *n != "doctor")
        .filter_map(|n| n.parse().ok())
        .collect();
    let canonicals = local.enabled(&canonicals, cli.verbose);

    // Resolve all canonical commands in one pass (avoids redundant detect() calls)
    let resolved = detect::resolve_all(groups, dir, &canonicals, cli.verbose);

    let chained = expanded.len() > 1;
    let plan = build_plan(&expanded, &resolved, chained, &local.disabled)?;
    let (rows, failure) = execute_plan(&plan, dir, groups, cli.interactive, theme)?;

    if summary::should_print(&rows) {
        println!();
        print!("{}", summary::render(&rows, theme));
    }

    match failure {
        Some(code) => Err(CommandExit(code).into()),
        None => Ok(()),
    }
}

fn build_plan<'a>(
    expanded: &[String],
    resolved: &'a [ResolvedCommand],
    chained: bool,
    disabled: &HashSet<CanonicalCommand>,
) -> Result<Vec<PlanEntry<'a>>> {
    let mut plan = Vec::new();
    for name in expanded {
        if name == "doctor" {
            plan.push(PlanEntry::Doctor);
            continue;
        }

        let canonical: CanonicalCommand = name.parse().unwrap();

        // Checked before resolution: disabled commands never reach resolve_all,
        // so an empty match must not fall through to "not detected"
        if disabled.contains(&canonical) {
            if chained {
                plan.push(PlanEntry::Disabled(canonical));
                continue;
            }
            bail!("{canonical} is disabled by {FILE_NAME}.");
        }

        let cmds: Vec<&ResolvedCommand> = resolved
            .iter()
            .filter(|r| r.canonical == canonical)
            .collect();

        if cmds.is_empty() {
            if chained {
                plan.push(PlanEntry::NotDetected(canonical));
                continue;
            }
            bail!("No {canonical} command detected.");
        }

        plan.extend(cmds.into_iter().map(PlanEntry::Exec));
    }
    Ok(plan)
}

fn execute_plan(
    plan: &[PlanEntry],
    dir: &Path,
    groups: &[DetectorGroup],
    interactive: bool,
    theme: &Theme,
) -> Result<(Vec<SummaryRow>, Option<i32>)> {
    let mut rows = Vec::new();
    let mut failure: Option<i32> = None;

    for entry in plan {
        match entry {
            PlanEntry::Doctor => {
                let outcome = if failure.is_some() {
                    Outcome::NotRun
                } else {
                    let start = Instant::now();
                    let all_passed = doctor::run(dir, groups, theme)?;
                    let duration = start.elapsed();
                    if all_passed {
                        Outcome::Success { duration }
                    } else {
                        failure = Some(1);
                        Outcome::Failure {
                            duration,
                            code: Some(1),
                        }
                    }
                };
                rows.push(SummaryRow {
                    name: "doctor".to_string(),
                    cmd: Some("health checks".to_string()),
                    outcome,
                });
            }
            PlanEntry::NotDetected(canonical) => {
                if failure.is_none() {
                    eprintln!(
                        "  {} {}",
                        "⊘".style(theme.muted),
                        format!("no {canonical} command detected, skipping").style(theme.muted),
                    );
                }
                rows.push(SummaryRow {
                    name: canonical.to_string(),
                    cmd: None,
                    outcome: Outcome::NotDetected,
                });
            }
            PlanEntry::Disabled(canonical) => {
                if failure.is_none() {
                    eprintln!(
                        "  {} {}",
                        "⊘".style(theme.muted),
                        format!("{canonical} disabled by {FILE_NAME}, skipping").style(theme.muted),
                    );
                }
                rows.push(SummaryRow {
                    name: canonical.to_string(),
                    cmd: None,
                    outcome: Outcome::Disabled,
                });
            }
            PlanEntry::Exec(cmd) => {
                let outcome = if failure.is_some() {
                    Outcome::NotRun
                } else {
                    let outcome = execute_one(cmd, interactive, theme, dir)?;
                    if let Outcome::Failure { code, .. } = outcome {
                        failure = Some(code.unwrap_or(1));
                    }
                    outcome
                };
                rows.push(SummaryRow {
                    name: cmd.canonical.to_string(),
                    cmd: Some(cmd.cmd.clone()),
                    outcome,
                });
            }
        }
    }

    Ok((rows, failure))
}

fn execute_one(
    cmd: &ResolvedCommand,
    interactive: bool,
    theme: &Theme,
    dir: &Path,
) -> Result<Outcome> {
    if interactive {
        let prompt = format!("Run `{}`?", sanitize(&cmd.cmd));
        match inquire::Confirm::new(&prompt).with_default(true).prompt() {
            Ok(true) => {}
            Ok(false) => {
                println!(
                    "  {} {} {}",
                    "⊘".style(theme.muted),
                    sanitize(&cmd.label).style(theme.muted),
                    "(skipped)".style(theme.muted)
                );
                return Ok(Outcome::Declined);
            }
            Err(_) => {
                // User cancelled (Ctrl-C)
                return Err(CommandExit(130).into());
            }
        }
    }

    println!(
        "{} {}",
        "→".style(theme.accent),
        sanitize(&cmd.cmd).style(theme.command)
    );

    let start = Instant::now();
    let status = Command::new("sh")
        .arg("-c")
        .arg(&cmd.cmd)
        .current_dir(dir)
        .status()?;
    let duration = start.elapsed();

    if status.success() {
        Ok(Outcome::Success { duration })
    } else {
        Ok(Outcome::Failure {
            duration,
            code: status.code(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{Ecosystem, Tier};

    fn rc(canonical: CanonicalCommand, cmd: &str) -> ResolvedCommand {
        ResolvedCommand {
            canonical,
            cmd: cmd.to_string(),
            label: cmd.to_string(),
            tier: Tier::Tier4,
            ecosystem: Ecosystem::Rust,
            detector_name: "test".to_string(),
            priority: 10,
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn build_plan_fans_out_multiple_commands_per_name() {
        let resolved = vec![
            rc(CanonicalCommand::Test, "cargo test"),
            rc(CanonicalCommand::Test, "pnpm test"),
            rc(CanonicalCommand::Build, "cargo build"),
        ];
        let plan =
            build_plan(&names(&["test", "build"]), &resolved, true, &HashSet::new()).unwrap();
        let cmds: Vec<&str> = plan
            .iter()
            .map(|entry| match entry {
                PlanEntry::Exec(c) => c.cmd.as_str(),
                _ => panic!("expected only exec entries"),
            })
            .collect();
        assert_eq!(cmds, ["cargo test", "pnpm test", "cargo build"]);
    }

    #[test]
    fn build_plan_marks_undetected_in_chains() {
        let resolved = vec![rc(CanonicalCommand::Test, "cargo test")];
        let plan = build_plan(&names(&["e2e", "test"]), &resolved, true, &HashSet::new()).unwrap();
        assert!(matches!(
            plan[0],
            PlanEntry::NotDetected(CanonicalCommand::E2e)
        ));
        assert!(matches!(plan[1], PlanEntry::Exec(_)));
    }

    #[test]
    fn build_plan_errors_for_single_undetected_name() {
        let err = build_plan(&names(&["e2e"]), &[], false, &HashSet::new()).unwrap_err();
        assert_eq!(err.to_string(), "No e2e command detected.");
    }

    #[test]
    fn build_plan_marks_disabled_in_chains() {
        let resolved = vec![rc(CanonicalCommand::Test, "cargo test")];
        let disabled = HashSet::from([CanonicalCommand::Format]);
        let plan = build_plan(&names(&["format", "test"]), &resolved, true, &disabled).unwrap();
        assert!(matches!(
            plan[0],
            PlanEntry::Disabled(CanonicalCommand::Format)
        ));
        assert!(matches!(plan[1], PlanEntry::Exec(_)));
    }

    #[test]
    fn build_plan_errors_for_single_disabled_name() {
        let disabled = HashSet::from([CanonicalCommand::Format]);
        let err = build_plan(&names(&["format"]), &[], false, &disabled).unwrap_err();
        assert_eq!(err.to_string(), "format is disabled by .letme.local.toml.");
    }

    #[test]
    fn build_plan_disabled_beats_not_detected() {
        // Disabled commands are filtered out before resolution, so they are
        // always absent from `resolved` — the disabled check must win
        let disabled = HashSet::from([CanonicalCommand::Format]);
        let err = build_plan(&names(&["format"]), &[], false, &disabled).unwrap_err();
        assert!(err.to_string().contains("disabled"), "got: {err}");
    }

    #[test]
    fn execute_plan_records_disabled_rows() {
        let resolved = rc(CanonicalCommand::Test, "true");
        let plan = vec![
            PlanEntry::Disabled(CanonicalCommand::Format),
            PlanEntry::Exec(&resolved),
        ];
        let dir = tempfile::tempdir().unwrap();
        let (rows, failure) = execute_plan(&plan, dir.path(), &[], false, &Theme::plain()).unwrap();

        assert_eq!(failure, None);
        assert_eq!(rows[0].name, "format");
        assert_eq!(rows[0].cmd, None);
        assert_eq!(rows[0].outcome, Outcome::Disabled);
        assert!(matches!(rows[1].outcome, Outcome::Success { .. }));
    }

    #[test]
    fn execute_plan_stops_at_first_failure_and_records_the_rest() {
        let resolved = [
            rc(CanonicalCommand::Format, "true"),
            rc(CanonicalCommand::Lint, "exit 7"),
            rc(CanonicalCommand::Test, "true"),
        ];
        let plan: Vec<PlanEntry> = resolved.iter().map(PlanEntry::Exec).collect();
        let dir = tempfile::tempdir().unwrap();
        let (rows, failure) = execute_plan(&plan, dir.path(), &[], false, &Theme::plain()).unwrap();

        assert_eq!(failure, Some(7));
        assert!(matches!(rows[0].outcome, Outcome::Success { .. }));
        assert!(matches!(
            rows[1].outcome,
            Outcome::Failure { code: Some(7), .. }
        ));
        assert!(matches!(rows[2].outcome, Outcome::NotRun));
        assert_eq!(rows[2].cmd.as_deref(), Some("true"));
    }
}
