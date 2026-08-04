use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::detect::{self, CanonicalCommand, ResolvedCommand};
use crate::detectors;
use crate::doctor;
use crate::theme::Theme;

/// Error indicating a subprocess exited with a specific code.
#[derive(Debug)]
pub struct CommandExit(pub i32);

impl std::fmt::Display for CommandExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "process exited with code {}", self.0)
    }
}

impl std::error::Error for CommandExit {}

pub fn run(
    dir: &Path,
    names: &[String],
    interactive: bool,
    verbose: bool,
    theme: &Theme,
    config: &Config,
) -> Result<()> {
    let all_detectors = detectors::all_detectors();

    let expanded = config
        .expand_aliases(names)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let canonicals: Vec<CanonicalCommand> = expanded
        .iter()
        .filter(|n| *n != "doctor")
        .filter_map(|n| n.parse().ok())
        .collect();

    // Resolve all canonical commands in one pass (avoids redundant detect() calls)
    let resolved = detect::resolve_all(&all_detectors, dir, &canonicals, verbose);

    // Execute in original request order
    let chained = expanded.len() > 1;
    for name in &expanded {
        if name == "doctor" {
            let all_passed = doctor::run(dir, theme)?;
            if !all_passed {
                return Err(CommandExit(1).into());
            }
            continue;
        }

        let canonical: CanonicalCommand = name.parse().unwrap();
        let cmds: Vec<&ResolvedCommand> = resolved
            .iter()
            .filter(|r| r.canonical == canonical)
            .collect();

        if cmds.is_empty() {
            if chained {
                eprintln!(
                    "  {} {}",
                    "⊘".style(theme.muted),
                    format!("no {canonical} command detected, skipping").style(theme.muted),
                );
                continue;
            }
            bail!("No {canonical} command detected.");
        }

        execute_resolved(&cmds, interactive, theme, dir)?;
    }

    Ok(())
}

fn execute_resolved(
    commands: &[&ResolvedCommand],
    interactive: bool,
    theme: &Theme,
    dir: &Path,
) -> Result<()> {
    for cmd in commands {
        if interactive {
            let prompt = format!("Run `{}`?", cmd.cmd);
            match inquire::Confirm::new(&prompt).with_default(true).prompt() {
                Ok(true) => {}
                Ok(false) => {
                    println!(
                        "  {} {} {}",
                        "⊘".style(theme.muted),
                        cmd.label.style(theme.muted),
                        "(skipped)".style(theme.muted)
                    );
                    continue;
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
            cmd.cmd.style(theme.command)
        );

        let status = Command::new("sh")
            .arg("-c")
            .arg(&cmd.cmd)
            .current_dir(dir)
            .status()?;

        if !status.success() {
            let code = status.code().unwrap_or(1);
            return Err(CommandExit(code).into());
        }
    }

    Ok(())
}
