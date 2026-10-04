use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::config::Config;
use crate::detect::{self, CommandKey, DetectorGroup, ResolvedCommand};
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

    let keys = local.enabled(&CommandKey::all(), verbose);
    let resolved = detect::resolve_all(groups, dir, &keys, verbose);

    let mut by_command: BTreeMap<CommandKey, Vec<&ResolvedCommand>> = BTreeMap::new();
    for cmd in &resolved {
        by_command.entry(cmd.key).or_default().push(cmd);
    }

    // Disabled commands are filtered out before resolution, so they are never
    // in `by_command`; they get their own row from here.
    let disabled: BTreeSet<CommandKey> = local
        .disabled
        .iter()
        .copied()
        .map(CommandKey::from)
        .collect();

    if by_command.is_empty() && disabled.is_empty() {
        println!("{}", "No canonical commands resolved.".style(theme.muted));
        return Ok(());
    }

    println!("{}", "Available commands:".style(theme.header));
    print!("{}", render_commands(&by_command, &disabled, theme));

    show_aliases(&config.aliases, &by_command, theme);

    Ok(())
}

fn render_commands(
    by_command: &BTreeMap<CommandKey, Vec<&ResolvedCommand>>,
    disabled: &BTreeSet<CommandKey>,
    theme: &Theme,
) -> String {
    let keys: BTreeSet<CommandKey> = by_command.keys().chain(disabled.iter()).copied().collect();

    let mut out = String::new();
    for key in keys {
        let name = key.to_string();
        if disabled.contains(&key) {
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
        for cmd in &by_command[&key] {
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
    by_command: &BTreeMap<CommandKey, Vec<&ResolvedCommand>>,
    theme: &Theme,
) {
    if !aliases.is_empty() {
        // Alias values are written the way a key prints ("lint", "lint --fix"),
        // so they're matched against the resolved keys' own spelling.
        let resolved: BTreeSet<String> = by_command.keys().map(CommandKey::to_string).collect();

        println!();
        println!("{}", "Aliases:".style(theme.header));
        let mut sorted: Vec<_> = aliases.iter().collect();
        sorted.sort_by_key(|(k, _)| (*k).clone());
        for (name, expansion) in sorted {
            let styled_commands: Vec<String> = expansion
                .iter()
                .map(|cmd| {
                    if resolved.contains(cmd.as_str()) {
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
    use crate::detect::{CanonicalCommand, Ecosystem, Tier};

    fn rc(canonical: CanonicalCommand, cmd: &str) -> ResolvedCommand {
        ResolvedCommand {
            key: CommandKey::from(canonical),
            cmd: cmd.to_string(),
            label: cmd.to_string(),
            tier: Tier::Tier4,
            ecosystem: Ecosystem::Rust,
            detector_name: "test".to_string(),
            priority: 10,
            covered_by: None,
            note: None,
        }
    }

    #[test]
    fn render_commands_shows_disabled_row_with_source() {
        let test_cmd = rc(CanonicalCommand::Test, "cargo test");
        let by_command =
            BTreeMap::from([(CommandKey::from(CanonicalCommand::Test), vec![&test_cmd])]);
        let disabled = BTreeSet::from([CommandKey::from(CanonicalCommand::Format)]);
        let out = render_commands(&by_command, &disabled, &Theme::plain());

        // Rows follow key order (canonical declaration order), not
        // alphabetical order.
        let expected = "  letme test
    → cargo test [convention, test]
  letme format
    ⊘ disabled (.letme.local.toml)
";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_commands_orders_a_variant_right_after_its_plain_key() {
        let format = rc(CanonicalCommand::Format, "cargo fmt");
        let mut check = rc(CanonicalCommand::Format, "cargo fmt --check");
        check.key = CanonicalCommand::Format.with(crate::detect::Modifier::Check);
        let build = rc(CanonicalCommand::Build, "cargo build");
        let by_command = BTreeMap::from([
            (build.key, vec![&build]),
            (check.key, vec![&check]),
            (format.key, vec![&format]),
        ]);

        let out = render_commands(&by_command, &BTreeSet::new(), &Theme::plain());

        let expected = "  letme format
    → cargo fmt [convention, test]
  letme format --check
    → cargo fmt --check [convention, test]
  letme build
    → cargo build [convention, test]
";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_commands_handles_all_disabled_project() {
        let disabled = BTreeSet::from([CommandKey::from(CanonicalCommand::Format)]);
        let out = render_commands(&BTreeMap::new(), &disabled, &Theme::plain());
        assert!(out.contains("letme format"), "got: {out}");
        assert!(out.contains("disabled (.letme.local.toml)"), "got: {out}");
    }
}
