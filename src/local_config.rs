use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

use crate::detect::CanonicalCommand;
use crate::theme::sanitize;

pub const FILE_NAME: &str = ".letme.local.toml";

#[derive(Debug, Default)]
pub struct LocalConfig {
    pub disabled: HashSet<CanonicalCommand>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLocalConfig {
    #[serde(default)]
    disable: Vec<String>,
}

impl LocalConfig {
    pub fn load(dir: &Path) -> Result<Self> {
        match std::fs::read_to_string(dir.join(FILE_NAME)) {
            Ok(contents) => parse(&contents),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(anyhow!("could not read {FILE_NAME}: {e}")),
        }
    }

    pub fn enabled(&self, commands: &[CanonicalCommand], verbose: bool) -> Vec<CanonicalCommand> {
        commands
            .iter()
            .copied()
            .filter(|c| {
                let disabled = self.disabled.contains(c);
                if disabled && verbose {
                    eprintln!("[verbose] {c}: disabled by {FILE_NAME}");
                }
                !disabled
            })
            .collect()
    }
}

fn parse(contents: &str) -> Result<LocalConfig> {
    let raw: RawLocalConfig =
        toml::from_str(contents).map_err(|e| anyhow!("{FILE_NAME}: {}", sanitize(e.message())))?;

    let mut disabled = HashSet::new();
    for name in &raw.disable {
        match name.parse::<CanonicalCommand>() {
            Ok(cmd) => {
                disabled.insert(cmd);
            }
            Err(e) => return Err(anyhow!("{FILE_NAME}: {}", sanitize(&e))),
        }
    }
    Ok(LocalConfig { disabled })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_disable_list() {
        let config = parse(r#"disable = ["format", "e2e"]"#).unwrap();
        assert_eq!(
            config.disabled,
            HashSet::from([CanonicalCommand::Format, CanonicalCommand::E2e])
        );
    }

    #[test]
    fn parse_empty_string_is_default() {
        let config = parse("").unwrap();
        assert!(config.disabled.is_empty());
    }

    #[test]
    fn parse_empty_disable_list() {
        let config = parse("disable = []").unwrap();
        assert!(config.disabled.is_empty());
    }

    #[test]
    fn parse_tolerates_duplicates() {
        let config = parse(r#"disable = ["format", "format"]"#).unwrap();
        assert_eq!(config.disabled, HashSet::from([CanonicalCommand::Format]));
    }

    #[test]
    fn parse_malformed_toml_errors() {
        let err = parse("disable = [").unwrap_err().to_string();
        assert!(err.contains(".letme.local.toml"), "got: {err}");
    }

    #[test]
    fn parse_unknown_command_errors_and_echoes_name() {
        let err = parse(r#"disable = ["prettier"]"#).unwrap_err().to_string();
        assert!(err.contains(".letme.local.toml"), "got: {err}");
        assert!(err.contains("Unknown command: prettier"), "got: {err}");
        assert!(err.contains("format"), "got: {err}");
    }

    #[test]
    fn parse_rejects_doctor() {
        let err = parse(r#"disable = ["doctor"]"#).unwrap_err().to_string();
        assert!(err.contains("Unknown command: doctor"), "got: {err}");
    }

    #[test]
    fn parse_rejects_prefixes() {
        let err = parse(r#"disable = ["form"]"#).unwrap_err().to_string();
        assert!(err.contains("Unknown command: form"), "got: {err}");
    }

    #[test]
    fn parse_rejects_unknown_top_level_key() {
        let err = parse(r#"disabled = ["format"]"#).unwrap_err().to_string();
        assert!(err.contains(".letme.local.toml"), "got: {err}");
        assert!(err.contains("disabled"), "got: {err}");
    }

    #[test]
    fn parse_sanitizes_hostile_name() {
        let err = parse("disable = [\"for\\u001bmat\"]")
            .unwrap_err()
            .to_string();
        assert!(!err.contains('\u{1b}'), "got: {err}");
        assert!(err.contains('\u{FFFD}'), "got: {err}");
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = LocalConfig::load(dir.path()).unwrap();
        assert!(config.disabled.is_empty());
    }

    #[test]
    fn load_reads_file_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), r#"disable = ["format"]"#).unwrap();
        let config = LocalConfig::load(dir.path()).unwrap();
        assert_eq!(config.disabled, HashSet::from([CanonicalCommand::Format]));
    }

    #[test]
    fn enabled_filters_disabled_commands() {
        let config = LocalConfig {
            disabled: HashSet::from([CanonicalCommand::Format]),
        };
        let enabled = config.enabled(
            &[
                CanonicalCommand::Format,
                CanonicalCommand::Lint,
                CanonicalCommand::Test,
            ],
            false,
        );
        assert_eq!(
            enabled,
            vec![CanonicalCommand::Lint, CanonicalCommand::Test]
        );
    }
}
