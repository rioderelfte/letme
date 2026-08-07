use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::config::Config;
use crate::detect::{self, CanonicalCommand, DetectorGroup, ResolvedCommand};
use crate::local_config::{FILE_NAME, LocalConfig};
use crate::theme::{Theme, sanitize};

pub fn show(
    dir: &Path,
    groups: &[DetectorGroup],
    verbose: bool,
    theme: &Theme,
    config: &Config,
    local: &LocalConfig,
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

    let commands = local.enabled(CanonicalCommand::all(), verbose);
    let resolved = detect::resolve_all(groups, dir, &commands, verbose);

    let mut by_command: BTreeMap<String, Vec<&ResolvedCommand>> = BTreeMap::new();
    for cmd in &resolved {
        by_command
            .entry(cmd.canonical.to_string())
            .or_default()
            .push(cmd);
    }

    let disabled: BTreeSet<String> = local.disabled.iter().map(|c| c.to_string()).collect();

    if by_command.is_empty() && disabled.is_empty() {
        println!("{}", "No canonical commands resolved.".style(theme.muted));
        return Ok(());
    }

    println!("{}", "Available commands:".style(theme.header));
    print!("{}", render_commands(&by_command, &disabled, theme));

    let aliases = config.effective_aliases();
    show_aliases(&aliases, &by_command, theme);

    Ok(())
}

fn render_commands(
    by_command: &BTreeMap<String, Vec<&ResolvedCommand>>,
    disabled: &BTreeSet<String>,
    theme: &Theme,
) -> String {
    let names: BTreeSet<&str> = by_command
        .keys()
        .chain(disabled.iter())
        .map(String::as_str)
        .collect();

    let mut out = String::new();
    for name in names {
        if disabled.contains(name) {
            out.push_str(&format!(
                "  {} {}\n    {} {}\n",
                "letme".style(theme.muted),
                name.style(theme.disabled),
                "⊘".style(theme.muted),
                format!("disabled ({FILE_NAME})").style(theme.muted),
            ));
            continue;
        }
        out.push_str(&format!(
            "  {} {}\n",
            "letme".style(theme.muted),
            name.style(theme.command)
        ));
        for cmd in &by_command[name] {
            if cmd.label != cmd.cmd {
                out.push_str(&format!(
                    "    {} {} {} {}\n",
                    "→".style(theme.accent),
                    sanitize(&cmd.cmd).style(theme.info),
                    format!("({})", sanitize(&cmd.label)).style(theme.muted),
                    format!("[{}, {}]", cmd.tier, cmd.detector_name).style(theme.muted),
                ));
            } else {
                out.push_str(&format!(
                    "    {} {} {}\n",
                    "→".style(theme.accent),
                    sanitize(&cmd.cmd).style(theme.info),
                    format!("[{}, {}]", cmd.tier, cmd.detector_name).style(theme.muted),
                ));
            }
        }
    }
    out
}

fn show_aliases(
    aliases: &std::collections::HashMap<String, Vec<String>>,
    by_command: &BTreeMap<String, Vec<&ResolvedCommand>>,
    theme: &Theme,
) {
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

    #[test]
    fn render_commands_shows_disabled_row_with_source() {
        let test_cmd = rc(CanonicalCommand::Test, "cargo test");
        let by_command = BTreeMap::from([("test".to_string(), vec![&test_cmd])]);
        let disabled = BTreeSet::from(["format".to_string()]);
        let out = render_commands(&by_command, &disabled, &Theme::plain());

        let expected = "  letme format
    ⊘ disabled (.letme.local.toml)
  letme test
    → cargo test [convention, test]
";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_commands_handles_all_disabled_project() {
        let disabled = BTreeSet::from(["format".to_string()]);
        let out = render_commands(&BTreeMap::new(), &disabled, &Theme::plain());
        assert!(out.contains("letme format"), "got: {out}");
        assert!(out.contains("disabled (.letme.local.toml)"), "got: {out}");
    }
}
