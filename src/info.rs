use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::config::Config;
use crate::detect::{self, CanonicalCommand, DetectorGroup, ResolvedCommand};
use crate::theme::Theme;

pub fn show(
    dir: &Path,
    groups: &[DetectorGroup],
    verbose: bool,
    theme: &Theme,
    config: &Config,
) -> Result<()> {
    // Find which detectors match (respecting exclusive groups)
    let mut detected: Vec<(&str, detect::Ecosystem)> = Vec::new();
    for group in groups {
        for detector in &group.0 {
            if detector.detect(dir) {
                detected.push((detector.name(), detector.ecosystem()));
                break; // first match in group wins
            }
        }
    }

    if detected.is_empty() {
        println!(
            "{}",
            "No ecosystems detected in this directory.".style(theme.muted)
        );
        return Ok(());
    }

    println!("{}", "Detected ecosystems:".style(theme.header));
    let mut ecosystems: Vec<(detect::Ecosystem, Vec<&str>)> = Vec::new();
    for &(name, ecosystem) in &detected {
        match ecosystems.iter_mut().find(|(e, _)| *e == ecosystem) {
            Some((_, names)) => names.push(name),
            None => ecosystems.push((ecosystem, vec![name])),
        }
    }
    for (ecosystem, names) in &ecosystems {
        println!(
            "  {} {} {}",
            "•".style(theme.accent),
            ecosystem.to_string().style(theme.primary),
            format!("({})", names.join(", ")).style(theme.muted),
        );
    }

    let missing = detect::check_missing_binaries(groups, dir);
    for m in &missing {
        eprintln!(
            "{} {} detected but {} is not installed",
            "Warning:".style(theme.warning),
            m.detector_name.style(theme.primary),
            format!("`{}`", m.binary).style(theme.error),
        );
    }

    println!();

    let resolved = detect::resolve_all(groups, dir, CanonicalCommand::all(), verbose);

    let mut by_command: BTreeMap<String, Vec<&ResolvedCommand>> = BTreeMap::new();
    for cmd in &resolved {
        by_command
            .entry(cmd.canonical.to_string())
            .or_default()
            .push(cmd);
    }

    if by_command.is_empty() {
        println!("{}", "No canonical commands resolved.".style(theme.muted));
        return Ok(());
    }

    println!("{}", "Available commands:".style(theme.header));
    for (name, cmds) in &by_command {
        println!(
            "  {} {}",
            "letme".style(theme.muted),
            name.style(theme.command)
        );
        for cmd in cmds {
            if cmd.label != cmd.cmd {
                println!(
                    "    {} {} {} {}",
                    "→".style(theme.accent),
                    cmd.cmd.style(theme.info),
                    format!("({})", cmd.label).style(theme.muted),
                    format!("[{}, {}]", cmd.tier, cmd.detector_name).style(theme.muted),
                );
            } else {
                println!(
                    "    {} {} {}",
                    "→".style(theme.accent),
                    cmd.cmd.style(theme.info),
                    format!("[{}, {}]", cmd.tier, cmd.detector_name).style(theme.muted),
                );
            }
        }
    }

    let aliases = config.effective_aliases();
    if !aliases.is_empty() {
        println!();
        println!("{}", "Aliases:".style(theme.header));
        let mut sorted: Vec<_> = aliases.iter().collect();
        sorted.sort_by_key(|(k, _)| (*k).clone());
        for (name, expansion) in sorted {
            let styled_commands: Vec<String> = expansion
                .iter()
                .map(|cmd| {
                    if by_command.contains_key(cmd.as_str()) {
                        format!("{}", cmd.style(theme.info))
                    } else {
                        format!("{}", cmd.style(theme.disabled))
                    }
                })
                .collect();
            println!(
                "  {} {} {} {}",
                "letme".style(theme.muted),
                name.style(theme.command),
                "\u{2192}".style(theme.accent),
                styled_commands.join(&format!("{}", ", ".style(theme.info))),
            );
        }
    }

    Ok(())
}
