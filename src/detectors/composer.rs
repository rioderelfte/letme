use std::collections::BTreeMap;
use std::path::Path;

use crate::detect::*;

pub struct ComposerDetector;

impl Detector for ComposerDetector {
    fn name(&self) -> &str {
        "composer"
    }

    fn tier(&self) -> Tier {
        Tier::Tier3
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Php
    }

    fn required_binaries(&self) -> &[&str] {
        &["composer"]
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("composer.json").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let mut commands = Vec::new();

        commands.push(self.make_command(CanonicalCommand::Install, "composer install".into(), 10));
        commands.push(self.make_command(CanonicalCommand::Clean, "rm -rf vendor".into(), 10));

        if let Some(scripts) = read_composer_scripts(dir) {
            for (name, elements) in &scripts {
                let result = if elements.len() == 1 {
                    map_script(name, &elements[0])
                } else {
                    resolve_composite_canonical(name, elements, &scripts)
                };
                if let Some((key, priority)) = result {
                    let cmd = format!("composer run {name}");
                    let mut rc = self.make_command(key, cmd, priority);
                    if elements.len() == 1 {
                        rc.label = elements[0].clone();
                    } else {
                        rc.label = elements.join("; ");
                    }
                    commands.push(rc);
                }

                // Composites and @-references are too opaque to synthesize
                // from, and bare tool names only resolve inside composer.
                if let [body] = elements.as_slice()
                    && !body.starts_with('@')
                {
                    for &target in CommandKey::VARIANTS {
                        if let Some(variant) = synthesize_variant(target, body)
                            && all_parts_pathed(&variant)
                        {
                            let cmd = variant.render(|part| part.to_string());
                            let mut rc = self.make_command(variant.key, cmd.clone(), 2);
                            rc.label = cmd;
                            rc.note = Some(format!("synthesized from '{name}' script"));
                            commands.push(rc);
                        }
                    }
                }
            }
        }

        commands
    }
}

/// Every part's tool token (after skipping an interpreter prefix) must be
/// path-qualified, e.g. `vendor/bin/php-cs-fixer`. A synthesized command runs
/// outside `composer run-script`, so bare tool names wouldn't resolve.
fn all_parts_pathed(variant: &SynthesizedVariant) -> bool {
    variant.parts().iter().all(|part| {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        tool_index(&tokens).is_some_and(|i| tokens[i].contains('/'))
    })
}

fn read_composer_scripts(dir: &Path) -> Option<BTreeMap<String, Vec<String>>> {
    let path = dir.join("composer.json");
    let contents = std::fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let scripts = json.get("scripts")?.as_object()?;

    let mut map = BTreeMap::new();
    for (key, value) in scripts {
        if let Some(val) = value.as_str() {
            map.insert(key.clone(), vec![val.to_string()]);
        } else if let Some(arr) = value.as_array() {
            let elements: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if !elements.is_empty() {
                map.insert(key.clone(), elements);
            }
        }
    }
    Some(map)
}

/// For composite (multi-element) scripts, resolve `@`-references and check
/// whether all elements agree on a single detection key.
///
/// Returns `Some((key, priority))` if consistent, `None` if mixed or
/// unresolvable (the composite is skipped).
fn resolve_composite_canonical(
    name: &str,
    elements: &[String],
    scripts: &BTreeMap<String, Vec<String>>,
) -> Option<(CommandKey, u32)> {
    let mut keys = Vec::new();
    let mut saw_format_verify = false;
    for element in elements {
        let cmd = if let Some(ref_name) = element.strip_prefix('@') {
            // Resolve @-reference: look up in the scripts map
            if let Some(target) = scripts.get(ref_name) {
                if target.len() == 1 {
                    &target[0]
                } else {
                    // nested composite, can't infer
                    continue;
                }
            } else {
                // unknown reference, skip
                continue;
            }
        } else {
            element.as_str()
        };
        let inference = infer_from_command(cmd);
        if let Some(key) = inference.key() {
            keys.push(key);
        } else if inference == Inference::FormatVerify {
            saw_format_verify = true;
        }
    }

    if keys.is_empty() {
        return if saw_format_verify {
            combine(map_script_name(name), Inference::FormatVerify)
        } else {
            None
        };
    }

    // All must agree on the same key
    let first = keys[0];
    if keys.iter().all(|k| *k == first) {
        combine(map_script_name(name), Inference::Canonical(first))
    } else {
        None // mixed concerns, skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_with_composer_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("composer.json"), r#"{"name": "test/pkg"}"#).unwrap();

        let detector = ComposerDetector;
        assert!(detector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_composer_json() {
        let dir = tempfile::tempdir().unwrap();

        let detector = ComposerDetector;
        assert!(!detector.detect(dir.path()));
    }

    #[test]
    fn parses_string_scripts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{"scripts": {"test": "phpunit", "lint": "phpstan analyse"}}"#,
        )
        .unwrap();

        let scripts = read_composer_scripts(dir.path()).unwrap();
        assert_eq!(scripts.get("test").unwrap(), &vec!["phpunit".to_string()]);
        assert_eq!(
            scripts.get("lint").unwrap(),
            &vec!["phpstan analyse".to_string()]
        );
    }

    #[test]
    fn parses_array_scripts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{"scripts": {"test": ["@phpunit", "@phpstan"]}}"#,
        )
        .unwrap();

        let scripts = read_composer_scripts(dir.path()).unwrap();
        assert_eq!(
            scripts.get("test").unwrap(),
            &vec!["@phpunit".to_string(), "@phpstan".to_string()]
        );
    }

    #[test]
    fn content_aware_script_mapping() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{
                "scripts": {
                    "format": "php-cs-fixer fix",
                    "lint": "php-cs-fixer fix --dry-run --diff",
                    "analyse": "phpstan analyse",
                    "check": ["@lint", "@analyse"],
                    "test": "phpunit"
                }
            }"#,
        )
        .unwrap();

        let detector = ComposerDetector;
        let commands = detector.resolve_commands(dir.path());

        let format_cmds: Vec<_> = commands
            .iter()
            .filter(|c| c.key.canonical == CanonicalCommand::Format)
            .collect();
        let format_exact = format_cmds
            .iter()
            .find(|c| c.cmd == "composer run format")
            .unwrap();
        assert_eq!(format_exact.priority, 10);

        // The lint name carries it; the content is only a format-verify.
        let lint_cmds: Vec<_> = commands
            .iter()
            .filter(|c| c.key.canonical == CanonicalCommand::Lint)
            .collect();
        let lint_reclassified = lint_cmds
            .iter()
            .find(|c| c.cmd == "composer run lint")
            .unwrap();
        assert_eq!(lint_reclassified.priority, 10);

        let lint_cmds: Vec<_> = commands
            .iter()
            .filter(|c| c.key.canonical == CanonicalCommand::Lint)
            .collect();
        let analyse = lint_cmds
            .iter()
            .find(|c| c.cmd == "composer run analyse")
            .unwrap();
        assert_eq!(analyse.priority, 10);

        let check = commands
            .iter()
            .find(|c| c.cmd == "composer run check")
            .unwrap();
        assert_eq!(check.key.canonical, CanonicalCommand::Lint);
        assert_eq!(check.priority, 10);

        let test = commands
            .iter()
            .find(|c| c.cmd == "composer run test")
            .unwrap();
        assert_eq!(test.key.canonical, CanonicalCommand::Test);
        assert_eq!(test.priority, 10);
    }

    #[test]
    fn consistent_composite_maps_correctly() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{
                "scripts": {
                    "phpstan": "phpstan analyse",
                    "phpcs": "phpcs",
                    "lint": ["@phpstan", "@phpcs"]
                }
            }"#,
        )
        .unwrap();

        let detector = ComposerDetector;
        let commands = detector.resolve_commands(dir.path());

        let check = commands
            .iter()
            .find(|c| c.cmd == "composer run lint")
            .unwrap();
        assert_eq!(check.key.canonical, CanonicalCommand::Lint);
        assert_eq!(check.priority, 10);
    }

    #[test]
    fn all_format_verify_composite_resolves_name_dependently() {
        // Every recognized element is a format-verify, so the composite
        // resolves through combine() by its name, like the single-element
        // "lint": "prettier --check ." would.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{
                "scripts": {
                    "cs-check": "php-cs-fixer fix --dry-run --diff",
                    "pint-check": "pint --test",
                    "lint": ["@cs-check", "@pint-check"]
                }
            }"#,
        )
        .unwrap();

        let detector = ComposerDetector;
        let commands = detector.resolve_commands(dir.path());

        let lint = commands
            .iter()
            .find(|c| c.cmd == "composer run lint")
            .unwrap();
        assert_eq!(lint.key, CommandKey::from(CanonicalCommand::Lint));
        assert_eq!(lint.priority, 10);
    }

    #[test]
    fn mixed_composite_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{
                "scripts": {
                    "phpstan": "phpstan analyse",
                    "test": "phpunit",
                    "check": ["@phpstan", "@test"]
                }
            }"#,
        )
        .unwrap();

        let detector = ComposerDetector;
        let commands = detector.resolve_commands(dir.path());

        assert!(
            commands.iter().all(|c| c.cmd != "composer run check"),
            "mixed composite 'check' should be skipped"
        );

        // Individual scripts still resolve
        assert!(commands.iter().any(|c| c.cmd == "composer run test"));
    }

    #[test]
    fn format_verify_in_composite_collapses_to_remaining_concern() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{
                "scripts": {
                    "cs-check": "php-cs-fixer fix --dry-run --diff",
                    "test": "phpunit",
                    "check": ["@cs-check", "@test"]
                }
            }"#,
        )
        .unwrap();

        let detector = ComposerDetector;
        let commands = detector.resolve_commands(dir.path());

        // A standalone format-verify with no matching name rehomes to the
        // check variant at the unmatched-content priority.
        let cs_check = commands
            .iter()
            .find(|c| c.cmd == "composer run cs-check")
            .unwrap();
        assert_eq!(cs_check.key, CanonicalCommand::Format.with(Modifier::Check));
        assert_eq!(cs_check.priority, 7);

        // The composite scan doesn't see the format-verify element, so it isn't
        // "mixed": it collapses to the remaining Test concern, at the low
        // priority used when the name (check maps to Lint) disagrees with the
        // content (Test). In a real project this is harmless: it's shadowed by
        // the standalone `test`.
        let check = commands
            .iter()
            .find(|c| c.cmd == "composer run check")
            .unwrap();
        assert_eq!(check.key.canonical, CanonicalCommand::Test);
        assert_eq!(check.priority, 3);
    }

    #[test]
    fn resolve_provides_install_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("composer.json"), r#"{"name": "test/pkg"}"#).unwrap();

        let detector = ComposerDetector;
        let commands = detector.resolve_commands(dir.path());

        assert!(
            commands
                .iter()
                .any(|c| c.key.canonical == CanonicalCommand::Install)
        );
        assert!(
            commands
                .iter()
                .any(|c| c.key.canonical == CanonicalCommand::Clean)
        );
    }

    #[test]
    fn synthesizes_format_check_from_a_pathed_script() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{"scripts": {"format": "vendor/bin/php-cs-fixer fix"}}"#,
        )
        .unwrap();

        let commands = ComposerDetector.resolve_commands(dir.path());

        let synthesized = commands
            .iter()
            .find(|c| c.key == CanonicalCommand::Format.with(Modifier::Check))
            .unwrap();
        assert_eq!(
            synthesized.cmd,
            "vendor/bin/php-cs-fixer fix --dry-run --diff"
        );
        assert_eq!(synthesized.priority, 2);
        assert_eq!(
            synthesized.note.as_deref(),
            Some("synthesized from 'format' script")
        );
    }

    #[test]
    fn does_not_synthesize_from_a_bare_tool_name() {
        // Composer's own PATH injection is what lets a bare tool name resolve
        // inside `composer run-script`; a synthesized command runs outside
        // that, so it declines rather than guess.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{"scripts": {"format": "php-cs-fixer fix"}}"#,
        )
        .unwrap();

        let commands = ComposerDetector.resolve_commands(dir.path());

        assert!(
            !commands
                .iter()
                .any(|c| c.key == CanonicalCommand::Format.with(Modifier::Check)),
            "a bare (non-pathed) tool name should not synthesize a check variant"
        );
    }

    #[test]
    fn does_not_synthesize_from_a_composite_or_a_bare_reference() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{
                "scripts": {
                    "format": ["vendor/bin/php-cs-fixer fix", "vendor/bin/pint"],
                    "ref-only": ["@format"]
                }
            }"#,
        )
        .unwrap();

        let commands = ComposerDetector.resolve_commands(dir.path());

        let no_synthesized = |name: &str| {
            !commands.iter().any(|c| {
                c.key == CanonicalCommand::Format.with(Modifier::Check)
                    && c.note.as_deref() == Some(&format!("synthesized from '{name}' script"))
            })
        };
        assert!(
            no_synthesized("format"),
            "a multi-element composite script should not synthesize a check variant"
        );
        assert!(
            no_synthesized("ref-only"),
            "a single bare @-reference should not synthesize a check variant"
        );
    }
}
