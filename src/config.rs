use anyhow::{Context, Result};
use etcetera::base_strategy::BaseStrategy;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::detect::{CanonicalCommand, CommandKey};
use crate::grammar::{self, NameTarget, RawSegment, Request};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub palette: Option<String>,
    #[serde(default)]
    pub aliases: HashMap<String, Vec<String>>,
}

/// What a name resolved to: a user alias (still needing expansion) or an
/// exact target (canonical or standalone command).
pub enum Resolved {
    Alias(String),
    Name(NameTarget),
}

impl Config {
    /// Resolve a top-level (CLI) name.
    ///
    /// Resolution order:
    /// 1. Exact alias key match
    /// 2. Exact name match (canonical and standalone commands)
    /// 3. Unambiguous prefix match against the union of alias keys +
    ///    canonical names (standalone names never match by prefix)
    pub fn resolve(&self, input: &str) -> Result<Resolved, String> {
        if self.aliases.contains_key(input) {
            return Ok(Resolved::Alias(input.to_string()));
        }

        if let Some(target) = grammar::lookup_exact(input) {
            return Ok(Resolved::Name(target));
        }

        let mut matches: Vec<String> = Vec::new();
        for canonical in CanonicalCommand::all() {
            let name = canonical.as_str();
            if name.starts_with(input) {
                matches.push(name.to_string());
            }
        }
        for key in self.aliases.keys() {
            if key.starts_with(input) && !matches.contains(key) {
                matches.push(key.clone());
            }
        }

        match matches.len() {
            1 => {
                let name = matches.into_iter().next().unwrap();
                Ok(match grammar::lookup_exact(&name) {
                    Some(target) => Resolved::Name(target),
                    None => Resolved::Alias(name),
                })
            }
            0 => Err(format!(
                "unknown command: {input}. Valid commands: {}",
                grammar::valid_names()
            )),
            _ => {
                matches.sort();
                Err(format!(
                    "ambiguous command: {input}. Could be: {}",
                    matches.join(", ")
                ))
            }
        }
    }

    /// Resolve a name found inside an alias value: exact alias key or exact
    /// name only. Prefixes only resolve on the command line.
    fn resolve_exact(&self, input: &str, alias: &str) -> Result<Resolved, String> {
        if self.aliases.contains_key(input) {
            return Ok(Resolved::Alias(input.to_string()));
        }
        if let Some(target) = grammar::lookup_exact(input) {
            return Ok(Resolved::Name(target));
        }
        Err(format!(
            "unknown command: {input} (in alias '{alias}'). Valid commands: {}",
            grammar::valid_names()
        ))
    }

    /// Turn top-level CLI segments into a [`Request`]. No segments is the info
    /// view. A standalone command first gets everything after it as its own
    /// arguments. Otherwise segments expand into a deduplicated, ordered list
    /// of command keys (first occurrence wins). Aliases expand recursively;
    /// their values may reference other aliases, and cycles are an error.
    pub fn expand(&self, segments: &[RawSegment]) -> Result<Request, String> {
        let Some((first, rest)) = segments.split_first() else {
            return Ok(Request::Info);
        };
        let resolved = self.resolve(&first.head)?;
        if let Resolved::Name(NameTarget::Standalone(standalone)) = resolved {
            let args: Vec<String> = first
                .flags
                .iter()
                .chain(rest.iter().flat_map(RawSegment::tokens))
                .cloned()
                .collect();
            return standalone.parse(&args);
        }

        let mut expansion = Expansion::default();
        self.expand_resolved(resolved, &first.flags, None, &mut expansion)?;
        for segment in rest {
            let resolved = self.resolve(&segment.head)?;
            self.expand_resolved(resolved, &segment.flags, None, &mut expansion)?;
        }
        Ok(Request::Run(expansion.result))
    }

    fn expand_resolved(
        &self,
        resolved: Resolved,
        flags: &[String],
        alias_context: Option<&str>,
        expansion: &mut Expansion,
    ) -> Result<(), String> {
        match resolved {
            Resolved::Alias(name) => {
                if !flags.is_empty() {
                    return Err(format!(
                        "'{name}' is an alias and cannot take flags. Give the flag to one of its commands in config.toml instead{}",
                        context_suffix(alias_context)
                    ));
                }
                let visiting = &mut expansion.visiting;
                if visiting.iter().any(|v| v == &name) {
                    return Err(if visiting.last().is_some_and(|v| v == &name) {
                        format!("alias '{name}' references itself")
                    } else {
                        format!("alias cycle: {} -> {name}", visiting.join(" -> "))
                    });
                }
                visiting.push(name.clone());
                for element in &self.aliases[&name] {
                    let tokens: Vec<&str> = element.split_whitespace().collect();
                    let Some((head, elem_flags)) = tokens.split_first() else {
                        continue;
                    };
                    if head.starts_with('-') {
                        return Err(format!(
                            "flag '{head}' found before any command name (in alias '{name}')"
                        ));
                    }
                    // One element is one command and its flags; a bare word
                    // after the name is a second command in the wrong place.
                    if let Some(word) = elem_flags.iter().find(|t| !t.starts_with('-')) {
                        return Err(format!(
                            "unexpected '{word}' after '{head}' (in alias '{name}'). Give each command its own entry: [\"{head}\", \"{word}\"]"
                        ));
                    }
                    let elem_flags: Vec<String> =
                        elem_flags.iter().map(|s| (*s).to_string()).collect();
                    let resolved = self.resolve_exact(head, &name)?;
                    self.expand_resolved(resolved, &elem_flags, Some(&name), expansion)?;
                }
                expansion.visiting.pop();
                Ok(())
            }
            Resolved::Name(NameTarget::Standalone(standalone)) => Err(match alias_context {
                Some(alias) => format!(
                    "{} can't be part of an alias (in alias '{alias}')",
                    standalone.name()
                ),
                None => format!("{} can't be chained with other commands", standalone.name()),
            }),
            Resolved::Name(NameTarget::Canonical(canonical)) => {
                let modifier = grammar::parse_segment_flags(canonical, flags)
                    .map_err(|e| format!("{e}{}", context_suffix(alias_context)))?;
                let key = CommandKey {
                    canonical,
                    modifier,
                };
                if expansion.seen.insert(key) {
                    expansion.result.push(key);
                }
                Ok(())
            }
        }
    }
}

/// Mutable state threaded through alias expansion: the alias chain being
/// expanded (for cycle detection) and the deduplicated output.
#[derive(Default)]
struct Expansion {
    visiting: Vec<String>,
    seen: HashSet<CommandKey>,
    result: Vec<CommandKey>,
}

fn context_suffix(alias: Option<&str>) -> String {
    match alias {
        Some(name) => format!(" (in alias '{name}')"),
        None => String::new(),
    }
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

    fn config_with(aliases: &[(&str, &[&str])]) -> Config {
        Config {
            aliases: aliases
                .iter()
                .map(|(name, expansion)| {
                    (
                        (*name).to_string(),
                        expansion.iter().map(|c| (*c).to_string()).collect(),
                    )
                })
                .collect(),
            ..Default::default()
        }
    }

    fn segs(heads: &[&str]) -> Vec<RawSegment> {
        heads
            .iter()
            .map(|h| RawSegment {
                head: (*h).to_string(),
                flags: Vec::new(),
            })
            .collect()
    }

    fn cmd(c: CanonicalCommand) -> CommandKey {
        CommandKey::from(c)
    }

    fn keys(request: Request) -> Vec<CommandKey> {
        match request {
            Request::Run(keys) => keys,
            other => panic!("expected a run, got {other:?}"),
        }
    }

    #[test]
    fn expand_simple_alias() {
        let config = config_with(&[("ci", &["format", "lint", "typecheck", "test"])]);
        let result = keys(config.expand(&segs(&["ci"])).unwrap());
        assert_eq!(
            result,
            vec![
                cmd(CanonicalCommand::Format),
                cmd(CanonicalCommand::Lint),
                cmd(CanonicalCommand::Typecheck),
                cmd(CanonicalCommand::Test),
            ]
        );
    }

    #[test]
    fn expand_deduplicates_preserving_order() {
        let config = config_with(&[("ci", &["format", "lint", "typecheck", "test"])]);
        let result = keys(config.expand(&segs(&["ci", "test"])).unwrap());
        assert_eq!(
            result,
            vec![
                cmd(CanonicalCommand::Format),
                cmd(CanonicalCommand::Lint),
                cmd(CanonicalCommand::Typecheck),
                cmd(CanonicalCommand::Test),
            ]
        );
    }

    #[test]
    fn expand_command_then_alias_preserves_first_occurrence() {
        let config = config_with(&[("ci", &["format", "lint", "typecheck", "test"])]);
        let result = keys(config.expand(&segs(&["test", "ci"])).unwrap());
        assert_eq!(
            result,
            vec![
                cmd(CanonicalCommand::Test),
                cmd(CanonicalCommand::Format),
                cmd(CanonicalCommand::Lint),
                cmd(CanonicalCommand::Typecheck),
            ]
        );
    }

    #[test]
    fn expand_non_alias_passes_through() {
        let config = Config::default();
        let result = keys(config.expand(&segs(&["build"])).unwrap());
        assert_eq!(result, vec![cmd(CanonicalCommand::Build)]);
    }

    #[test]
    fn expand_nested_alias() {
        let config = config_with(&[
            ("ci", &["format", "lint", "typecheck", "test"]),
            ("full", &["ci", "build"]),
        ]);
        let result = keys(config.expand(&segs(&["full"])).unwrap());
        assert_eq!(
            result,
            vec![
                cmd(CanonicalCommand::Format),
                cmd(CanonicalCommand::Lint),
                cmd(CanonicalCommand::Typecheck),
                cmd(CanonicalCommand::Test),
                cmd(CanonicalCommand::Build),
            ]
        );
    }

    #[test]
    fn expand_nested_alias_deduplicates() {
        let config = config_with(&[
            ("ci", &["format", "lint", "typecheck", "test"]),
            ("full", &["test", "ci"]),
        ]);
        let result = keys(config.expand(&segs(&["full"])).unwrap());
        assert_eq!(
            result,
            vec![
                cmd(CanonicalCommand::Test),
                cmd(CanonicalCommand::Format),
                cmd(CanonicalCommand::Lint),
                cmd(CanonicalCommand::Typecheck),
            ]
        );
    }

    #[test]
    fn doctor_in_an_alias_value_errors() {
        let config = config_with(&[("checkup", &["doctor", "test"]), ("dr", &["doctor"])]);
        let err = config.expand(&segs(&["checkup"])).unwrap_err();
        assert_eq!(err, "doctor can't be part of an alias (in alias 'checkup')");
        let err = config.expand(&segs(&["dr"])).unwrap_err();
        assert_eq!(err, "doctor can't be part of an alias (in alias 'dr')");
    }

    #[test]
    fn doctor_alone_is_its_own_request() {
        let config = Config::default();
        assert_eq!(config.expand(&segs(&["doctor"])).unwrap(), Request::Doctor);
    }

    #[test]
    fn standalone_gets_the_rest_of_argv_as_arguments() {
        let config = Config::default();
        let mut segments = segs(&["doctor", "test"]);
        segments[1].flags.push("--json".to_string());
        let err = config.expand(&segments).unwrap_err();
        assert_eq!(err, "doctor takes no arguments (got test)");

        let mut segments = segs(&["doctor"]);
        segments[0].flags.push("--json".to_string());
        let err = config.expand(&segments).unwrap_err();
        assert_eq!(err, "doctor takes no arguments (got --json)");
    }

    #[test]
    fn standalone_after_another_segment_cannot_chain() {
        let config = config_with(&[("ok", &["lint", "test"])]);
        for heads in [&["test", "doctor"][..], &["ok", "doctor"]] {
            let err = config.expand(&segs(heads)).unwrap_err();
            assert_eq!(
                err, "doctor can't be chained with other commands",
                "for {heads:?}"
            );
        }
    }

    #[test]
    fn standalone_names_do_not_match_by_prefix() {
        let config = Config::default();
        for input in ["d", "doc", "docto"] {
            let err = config.expand(&segs(&[input])).unwrap_err();
            assert!(err.contains("unknown command"), "for {input}: {err}");
        }
    }

    #[test]
    fn no_segments_is_the_info_view() {
        assert_eq!(Config::default().expand(&[]).unwrap(), Request::Info);
    }

    #[test]
    fn self_referencing_alias_errors() {
        let config = config_with(&[("foo", &["foo", "e2e"])]);
        let err = config.expand(&segs(&["foo"])).unwrap_err();
        assert_eq!(err, "alias 'foo' references itself");
    }

    #[test]
    fn alias_cycle_errors() {
        let config = config_with(&[("a", &["b"]), ("b", &["a"])]);
        let err = config.expand(&segs(&["a"])).unwrap_err();
        assert_eq!(err, "alias cycle: a -> b -> a");
    }

    #[test]
    fn prefix_in_alias_value_errors() {
        // Alias values must be exact names; prefixes only resolve on the command line
        let config = config_with(&[("ci", &["te"])]);
        let err = config.expand(&segs(&["ci"])).unwrap_err();
        assert!(err.contains("unknown command: te"), "got: {err}");
        assert!(err.contains("in alias 'ci'"), "got: {err}");
    }

    #[test]
    fn expand_invalid_value_errors() {
        let config = config_with(&[("bad", &["nonexistent"])]);
        let result = config.expand(&segs(&["bad"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown command"));
    }

    #[test]
    fn expand_invalid_non_alias_errors() {
        let config = Config::default();
        let result = config.expand(&segs(&["nonexistent"]));
        assert!(result.is_err());
    }

    #[test]
    fn prefix_i_resolves_to_install() {
        let config = Config::default();
        let result = keys(config.expand(&segs(&["i"])).unwrap());
        assert_eq!(result, vec![cmd(CanonicalCommand::Install)]);
    }

    #[test]
    fn prefix_t_is_ambiguous() {
        // "t" matches both "test" and "typecheck"
        let config = Config::default();
        let result = config.expand(&segs(&["t"]));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("ambiguous command: t"), "got: {err}");
        assert!(err.contains("test"), "got: {err}");
        assert!(err.contains("typecheck"), "got: {err}");
    }

    #[test]
    fn prefix_te_resolves_to_test() {
        let config = Config::default();
        let result = keys(config.expand(&segs(&["te"])).unwrap());
        assert_eq!(result, vec![cmd(CanonicalCommand::Test)]);
    }

    #[test]
    fn prefix_ty_resolves_to_typecheck() {
        let config = Config::default();
        let result = keys(config.expand(&segs(&["ty"])).unwrap());
        assert_eq!(result, vec![cmd(CanonicalCommand::Typecheck)]);
    }

    #[test]
    fn prefix_f_resolves_to_format() {
        let config = Config::default();
        let result = keys(config.expand(&segs(&["f"])).unwrap());
        assert_eq!(result, vec![cmd(CanonicalCommand::Format)]);
    }

    #[test]
    fn prefix_resolves_to_alias() {
        let config = config_with(&[("verify", &["lint", "test"])]);
        let result = keys(config.expand(&segs(&["v"])).unwrap());
        assert_eq!(
            result,
            vec![cmd(CanonicalCommand::Lint), cmd(CanonicalCommand::Test)]
        );
    }

    #[test]
    fn prefix_ambiguous_errors() {
        // With a user alias "ci" next to the canonical "clean", "c" is ambiguous
        let config = config_with(&[("ci", &["lint", "test"])]);
        let result = config.expand(&segs(&["c"]));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("ambiguous command: c"), "got: {err}");
        assert!(err.contains("ci"), "got: {err}");
        assert!(err.contains("clean"), "got: {err}");
    }

    #[test]
    fn prefix_unknown_errors() {
        let config = Config::default();
        let result = config.expand(&segs(&["z"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown command: z"));
    }

    #[test]
    fn prefix_chaining_works() {
        let config = Config::default();
        let result = keys(config.expand(&segs(&["te", "l"])).unwrap());
        assert_eq!(
            result,
            vec![cmd(CanonicalCommand::Test), cmd(CanonicalCommand::Lint)]
        );
    }

    #[test]
    fn alias_segment_with_flags_errors() {
        let config = config_with(&[("ok", &["lint", "test"])]);
        let mut segments = segs(&["ok"]);
        segments[0].flags.push("--fix".to_string());
        let err = config.expand(&segments).unwrap_err();
        assert_eq!(
            err,
            "'ok' is an alias and cannot take flags. Give the flag to one of its commands in config.toml instead"
        );
    }

    #[test]
    fn alias_value_stray_word_names_the_word_not_a_flag() {
        // "lint test" is two commands crammed into one entry, not a flag.
        let config = config_with(&[("bad", &["lint test"])]);
        let err = config.expand(&segs(&["bad"])).unwrap_err();
        assert_eq!(
            err,
            "unexpected 'test' after 'lint' (in alias 'bad'). Give each command its own entry: [\"lint\", \"test\"]"
        );
    }

    #[test]
    fn alias_value_stray_word_after_a_flag_errors() {
        let config = config_with(&[("bad", &["lint --fix extra"])]);
        let err = config.expand(&segs(&["bad"])).unwrap_err();
        assert!(
            err.contains("unexpected 'extra' after 'lint'"),
            "got: {err}"
        );
    }

    #[test]
    fn alias_value_flag_before_name_errors() {
        let config = config_with(&[("bad", &["lint", "--fix"])]);
        let err = config.expand(&segs(&["bad"])).unwrap_err();
        assert_eq!(
            err,
            "flag '--fix' found before any command name (in alias 'bad')"
        );
    }

    #[test]
    fn dedup_identity_is_the_full_key_not_just_the_canonical() {
        // "lint" and "lint --fix" are different keys, so both run.
        let config = Config::default();
        let mut segments = segs(&["lint", "lint"]);
        segments[1].flags.push("--fix".to_string());

        let result = keys(config.expand(&segments).unwrap());

        assert_eq!(
            result,
            vec![
                cmd(CanonicalCommand::Lint),
                CanonicalCommand::Lint.with(crate::detect::Modifier::Fix),
            ]
        );
    }

    #[test]
    fn alias_value_can_carry_a_flag() {
        let config = config_with(&[("lf", &["lint --fix"])]);
        let result = keys(config.expand(&segs(&["lf"])).unwrap());
        assert_eq!(
            result,
            vec![CanonicalCommand::Lint.with(crate::detect::Modifier::Fix)]
        );
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
    }
}
