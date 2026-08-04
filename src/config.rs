use anyhow::{Context, Result};
use etcetera::base_strategy::BaseStrategy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::detect::CanonicalCommand;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub palette: Option<String>,
    #[serde(default)]
    pub aliases: HashMap<String, Vec<String>>,
}

impl Config {
    /// Return aliases with built-in defaults merged under user overrides.
    /// User entries override built-in entries with the same key.
    pub fn effective_aliases(&self) -> HashMap<String, Vec<String>> {
        let mut merged = default_aliases();
        for (k, v) in &self.aliases {
            merged.insert(k.clone(), v.clone());
        }
        merged
    }

    /// Resolve a user-provided name to an exact command or alias name.
    ///
    /// Resolution order:
    /// 1. Exact alias key match
    /// 2. Exact command name match (canonical commands + "doctor")
    /// 3. Unambiguous prefix match against the union of alias keys + command names
    fn resolve_name(
        &self,
        input: &str,
        aliases: &HashMap<String, Vec<String>>,
    ) -> Result<String, String> {
        // 1. Exact alias match
        if aliases.contains_key(input) {
            return Ok(input.to_string());
        }

        // 2. Exact command name match
        if input == "doctor" || input.parse::<CanonicalCommand>().is_ok() {
            return Ok(input.to_string());
        }

        // 3. Prefix match against all alias keys + command names + "doctor"
        let mut matches = Vec::new();

        for cmd in CanonicalCommand::all() {
            let name = cmd.to_string();
            if name.starts_with(input) {
                matches.push(name);
            }
        }
        if "doctor".starts_with(input) {
            matches.push("doctor".to_string());
        }
        for key in aliases.keys() {
            if key.starts_with(input) && !matches.contains(key) {
                matches.push(key.clone());
            }
        }

        match matches.len() {
            1 => Ok(matches.into_iter().next().unwrap()),
            0 => Err(format!(
                "Unknown command: {input}. Valid commands: {}, doctor",
                CanonicalCommand::all_names()
            )),
            _ => {
                matches.sort();
                Err(format!(
                    "Ambiguous command: {input}. Could be: {}",
                    matches.join(", ")
                ))
            }
        }
    }

    /// Expand a list of command/alias names into canonical command names.
    /// Aliases are expanded recursively and may reference other aliases;
    /// cycles are an error. Duplicates are removed (first occurrence wins).
    /// Supports unambiguous prefix matching (e.g. "te" resolves to "test", "i" to "install").
    pub fn expand_aliases(&self, names: &[String]) -> Result<Vec<String>, String> {
        let aliases = self.effective_aliases();
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for name in names {
            let resolved = self.resolve_name(name, &aliases)?;
            expand_into(&resolved, &aliases, &mut Vec::new(), &mut seen, &mut result)?;
        }

        Ok(result)
    }
}

fn expand_into(
    name: &str,
    aliases: &HashMap<String, Vec<String>>,
    visiting: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    result: &mut Vec<String>,
) -> Result<(), String> {
    if let Some(expansion) = aliases.get(name) {
        if visiting.iter().any(|v| v == name) {
            return Err(if visiting.last().is_some_and(|v| v == name) {
                let hint = if default_aliases().contains_key(name) {
                    " To extend the built-in alias, list its commands explicitly."
                } else {
                    ""
                };
                format!("Alias '{name}' references itself.{hint}")
            } else {
                format!("Alias cycle: {} -> {name}", visiting.join(" -> "))
            });
        }
        visiting.push(name.to_string());
        for element in expansion {
            expand_into(element, aliases, visiting, seen, result)?;
        }
        visiting.pop();
    } else if name == "doctor" || name.parse::<CanonicalCommand>().is_ok() {
        if seen.insert(name.to_string()) {
            result.push(name.to_string());
        }
    } else {
        let context = visiting
            .last()
            .map(|parent| format!(" (in alias '{parent}')"))
            .unwrap_or_default();
        return Err(format!(
            "Unknown command: {name}{context}. Valid commands: {}, doctor",
            CanonicalCommand::all_names()
        ));
    }
    Ok(())
}

fn default_aliases() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    m.insert(
        "ok".into(),
        vec![
            "format".into(),
            "lint".into(),
            "typecheck".into(),
            "test".into(),
        ],
    );
    m
}

pub fn load_config() -> Config {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return Config::default(),
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn config_path() -> Result<PathBuf> {
    Ok(dirs_base()?.join("config.toml"))
}

pub fn dirs_base() -> Result<PathBuf> {
    let strategy =
        etcetera::base_strategy::Xdg::new().context("could not determine home directory")?;
    Ok(strategy.config_dir().join("letme"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_override_replaces_default() {
        let config = Config {
            aliases: HashMap::from([("ok".into(), vec!["lint".into(), "test".into()])]),
            ..Default::default()
        };
        let aliases = config.effective_aliases();
        assert_eq!(
            aliases.get("ok"),
            Some(&vec!["lint".to_string(), "test".to_string()])
        );
    }

    #[test]
    fn user_can_add_new_alias() {
        let config = Config {
            aliases: HashMap::from([("ci".into(), vec!["lint".into(), "test".into()])]),
            ..Default::default()
        };
        let aliases = config.effective_aliases();
        assert!(aliases.contains_key("ok")); // built-in preserved
        assert_eq!(
            aliases.get("ci"),
            Some(&vec!["lint".to_string(), "test".to_string()])
        );
    }

    #[test]
    fn expand_simple_alias() {
        let config = Config::default();
        let result = config.expand_aliases(&["ok".into()]).unwrap();
        assert_eq!(result, vec!["format", "lint", "typecheck", "test"]);
    }

    #[test]
    fn expand_deduplicates_preserving_order() {
        let config = Config::default();
        // ok expands to [format, lint, typecheck, test], then test is a duplicate
        let result = config
            .expand_aliases(&["ok".into(), "test".into()])
            .unwrap();
        assert_eq!(result, vec!["format", "lint", "typecheck", "test"]);
    }

    #[test]
    fn expand_test_then_ok_preserves_first_occurrence() {
        let config = Config::default();
        // test comes first; the trailing test from ok's expansion is deduped
        let result = config
            .expand_aliases(&["test".into(), "ok".into()])
            .unwrap();
        assert_eq!(result, vec!["test", "format", "lint", "typecheck"]);
    }

    #[test]
    fn expand_non_alias_passes_through() {
        let config = Config::default();
        let result = config.expand_aliases(&["build".into()]).unwrap();
        assert_eq!(result, vec!["build"]);
    }

    #[test]
    fn expand_nested_alias() {
        let config = Config {
            aliases: HashMap::from([("full".into(), vec!["ok".into(), "build".into()])]),
            ..Default::default()
        };
        let result = config.expand_aliases(&["full".into()]).unwrap();
        assert_eq!(result, vec!["format", "lint", "typecheck", "test", "build"]);
    }

    #[test]
    fn expand_nested_alias_deduplicates() {
        let config = Config {
            aliases: HashMap::from([("full".into(), vec!["test".into(), "ok".into()])]),
            ..Default::default()
        };
        let result = config.expand_aliases(&["full".into()]).unwrap();
        assert_eq!(result, vec!["test", "format", "lint", "typecheck"]);
    }

    #[test]
    fn expand_doctor_in_alias_value() {
        let config = Config {
            aliases: HashMap::from([("checkup".into(), vec!["doctor".into(), "test".into()])]),
            ..Default::default()
        };
        let result = config.expand_aliases(&["checkup".into()]).unwrap();
        assert_eq!(result, vec!["doctor", "test"]);
    }

    #[test]
    fn self_referencing_alias_errors() {
        let config = Config {
            aliases: HashMap::from([("ok".into(), vec!["ok".into(), "e2e".into()])]),
            ..Default::default()
        };
        let err = config.expand_aliases(&["ok".into()]).unwrap_err();
        assert!(err.contains("Alias 'ok' references itself"), "got: {err}");
        assert!(err.contains("built-in"), "got: {err}");
    }

    #[test]
    fn self_referencing_user_alias_errors_without_builtin_hint() {
        let config = Config {
            aliases: HashMap::from([("foo".into(), vec!["foo".into()])]),
            ..Default::default()
        };
        let err = config.expand_aliases(&["foo".into()]).unwrap_err();
        assert!(err.contains("Alias 'foo' references itself"), "got: {err}");
        assert!(!err.contains("built-in"), "got: {err}");
    }

    #[test]
    fn alias_cycle_errors() {
        let config = Config {
            aliases: HashMap::from([
                ("a".into(), vec!["b".into()]),
                ("b".into(), vec!["a".into()]),
            ]),
            ..Default::default()
        };
        let err = config.expand_aliases(&["a".into()]).unwrap_err();
        assert!(err.contains("Alias cycle: a -> b -> a"), "got: {err}");
    }

    #[test]
    fn prefix_in_alias_value_errors() {
        // Alias values must be exact names; prefixes only resolve on the command line
        let config = Config {
            aliases: HashMap::from([("ci".into(), vec!["te".into()])]),
            ..Default::default()
        };
        let err = config.expand_aliases(&["ci".into()]).unwrap_err();
        assert!(err.contains("Unknown command: te"), "got: {err}");
        assert!(err.contains("in alias 'ci'"), "got: {err}");
    }

    #[test]
    fn expand_invalid_value_errors() {
        let config = Config {
            aliases: HashMap::from([("bad".into(), vec!["nonexistent".into()])]),
            ..Default::default()
        };
        let result = config.expand_aliases(&["bad".into()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown command"));
    }

    #[test]
    fn expand_invalid_non_alias_errors() {
        let config = Config::default();
        let result = config.expand_aliases(&["nonexistent".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn prefix_i_resolves_to_install() {
        let config = Config::default();
        let result = config.expand_aliases(&["i".into()]).unwrap();
        assert_eq!(result, vec!["install"]);
    }

    #[test]
    fn prefix_t_is_ambiguous() {
        // "t" matches both "test" and "typecheck"
        let config = Config::default();
        let result = config.expand_aliases(&["t".into()]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Ambiguous command: t"), "got: {err}");
        assert!(err.contains("test"), "got: {err}");
        assert!(err.contains("typecheck"), "got: {err}");
    }

    #[test]
    fn prefix_te_resolves_to_test() {
        let config = Config::default();
        let result = config.expand_aliases(&["te".into()]).unwrap();
        assert_eq!(result, vec!["test"]);
    }

    #[test]
    fn prefix_ty_resolves_to_typecheck() {
        let config = Config::default();
        let result = config.expand_aliases(&["ty".into()]).unwrap();
        assert_eq!(result, vec!["typecheck"]);
    }

    #[test]
    fn prefix_d_resolves_to_doctor() {
        let config = Config::default();
        let result = config.expand_aliases(&["d".into()]).unwrap();
        assert_eq!(result, vec!["doctor"]);
    }

    #[test]
    fn prefix_o_resolves_to_alias_ok() {
        let config = Config::default();
        let result = config.expand_aliases(&["o".into()]).unwrap();
        assert_eq!(result, vec!["format", "lint", "typecheck", "test"]);
    }

    #[test]
    fn prefix_ambiguous_errors() {
        // With a user alias "ci" next to the built-in "clean", "c" is ambiguous
        let config = Config {
            aliases: HashMap::from([("ci".into(), vec!["lint".into(), "test".into()])]),
            ..Default::default()
        };
        let result = config.expand_aliases(&["c".into()]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Ambiguous command: c"), "got: {err}");
        assert!(err.contains("ci"), "got: {err}");
        assert!(err.contains("clean"), "got: {err}");
    }

    #[test]
    fn prefix_unknown_errors() {
        let config = Config::default();
        let result = config.expand_aliases(&["z".into()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown command: z"));
    }

    #[test]
    fn prefix_chaining_works() {
        let config = Config::default();
        let result = config.expand_aliases(&["te".into(), "l".into()]).unwrap();
        assert_eq!(result, vec!["test", "lint"]);
    }

    #[test]
    fn toml_round_trip() {
        let toml_str = r#"
palette = "dark"

[aliases]
ok = ["lint", "test"]
ci = ["build", "test"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.palette, Some("dark".into()));
        assert_eq!(
            config.aliases.get("ok"),
            Some(&vec!["lint".to_string(), "test".to_string()])
        );
        assert_eq!(
            config.aliases.get("ci"),
            Some(&vec!["build".to_string(), "test".to_string()])
        );

        // effective_aliases should use user's ok, not default
        let effective = config.effective_aliases();
        assert_eq!(
            effective.get("ok"),
            Some(&vec!["lint".to_string(), "test".to_string()])
        );
    }
}
