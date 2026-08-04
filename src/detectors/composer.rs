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
                if let Some((canonical, priority)) = result {
                    let cmd = format!("composer run {name}");
                    let mut rc = self.make_command(canonical, cmd, priority);
                    if elements.len() == 1 {
                        rc.label = elements[0].clone();
                    } else {
                        rc.label = elements.join("; ");
                    }
                    commands.push(rc);
                }
            }
        }

        commands
    }
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
/// whether all elements agree on a single canonical command.
///
/// Returns `Some((canonical, priority))` if consistent, `None` if mixed or
/// unresolvable (the composite is skipped).
fn resolve_composite_canonical(
    name: &str,
    elements: &[String],
    scripts: &BTreeMap<String, Vec<String>>,
) -> Option<(CanonicalCommand, u32)> {
    let mut canonicals = Vec::new();
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
        if let Some(canonical) = infer_from_command(cmd).canonical() {
            canonicals.push(canonical);
        }
    }

    if canonicals.is_empty() {
        return None;
    }

    // All must agree on the same canonical
    let first = canonicals[0];
    if canonicals.iter().all(|c| *c == first) {
        let priority = match map_script_name(name) {
            Some((name_cmd, _)) if name_cmd == first => 10,
            Some(_) => 3, // name disagrees with content
            None => 7,
        };
        Some((first, priority))
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
            .filter(|c| c.canonical == CanonicalCommand::Format)
            .collect();
        let format_exact = format_cmds
            .iter()
            .find(|c| c.cmd == "composer run format")
            .unwrap();
        assert_eq!(format_exact.priority, 10);

        // The lint name carries it; the content is only a format-verify.
        let lint_cmds: Vec<_> = commands
            .iter()
            .filter(|c| c.canonical == CanonicalCommand::Lint)
            .collect();
        let lint_reclassified = lint_cmds
            .iter()
            .find(|c| c.cmd == "composer run lint")
            .unwrap();
        assert_eq!(lint_reclassified.priority, 10);

        let lint_cmds: Vec<_> = commands
            .iter()
            .filter(|c| c.canonical == CanonicalCommand::Lint)
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
        assert_eq!(check.canonical, CanonicalCommand::Lint);
        assert_eq!(check.priority, 10);

        let test = commands
            .iter()
            .find(|c| c.cmd == "composer run test")
            .unwrap();
        assert_eq!(test.canonical, CanonicalCommand::Test);
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
        assert_eq!(check.canonical, CanonicalCommand::Lint);
        assert_eq!(check.priority, 10);
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

        // A standalone format-verify is unclassified and does not surface.
        assert!(
            commands.iter().all(|c| c.cmd != "composer run cs-check"),
            "format-verify 'cs-check' should not resolve to any canonical command"
        );

        // The format-verify element is invisible, so the composite is no longer
        // "mixed": it collapses to the remaining Test concern, at the low priority
        // used when the name (check maps to Lint) disagrees with the content
        // (Test). In a real project this is harmless: it's shadowed by the
        // standalone `test`.
        let check = commands
            .iter()
            .find(|c| c.cmd == "composer run check")
            .unwrap();
        assert_eq!(check.canonical, CanonicalCommand::Test);
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
                .any(|c| c.canonical == CanonicalCommand::Install)
        );
        assert!(
            commands
                .iter()
                .any(|c| c.canonical == CanonicalCommand::Clean)
        );
    }
}
