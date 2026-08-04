use std::collections::BTreeMap;
use std::path::Path;

use crate::detect::*;

pub struct NpmDetector;

impl Detector for NpmDetector {
    fn name(&self) -> &str {
        "npm"
    }

    fn tier(&self) -> Tier {
        Tier::Tier3
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn required_binaries(&self) -> &[&str] {
        &["npm"]
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("package.json").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let mut commands = Vec::new();

        commands.push(self.make_command(CanonicalCommand::Install, "npm install".into(), 10));
        commands.push(self.make_command(CanonicalCommand::Clean, "rm -rf node_modules".into(), 10));

        if let Some(scripts) = read_package_json_scripts(dir) {
            for (name, value) in &scripts {
                if let Some((canonical, priority)) = map_script(name, value) {
                    let cmd = format!("npm run {name}");
                    let mut rc = self.make_command(canonical, cmd, priority);
                    rc.label = value.clone();
                    commands.push(rc);
                }
            }
        }

        commands
    }
}

pub fn read_package_json_scripts(dir: &Path) -> Option<BTreeMap<String, String>> {
    let path = dir.join("package.json");
    let contents = std::fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let scripts = json.get("scripts")?.as_object()?;

    let mut map = BTreeMap::new();
    for (key, value) in scripts {
        if let Some(val) = value.as_str() {
            map.insert(key.clone(), val.to_string());
        }
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_with_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        let detector = NpmDetector;
        assert!(detector.detect(dir.path()));
    }

    #[test]
    fn detects_even_with_yarn_lock() {
        // Group ordering keeps npm from claiming yarn projects, not detect() itself.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let detector = NpmDetector;
        assert!(detector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_package_json() {
        let dir = tempfile::tempdir().unwrap();

        let detector = NpmDetector;
        assert!(!detector.detect(dir.path()));
    }

    #[test]
    fn parses_scripts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "jest", "lint": "eslint ."}}"#,
        )
        .unwrap();

        let scripts = read_package_json_scripts(dir.path()).unwrap();
        assert_eq!(scripts.get("test").unwrap(), "jest");
        assert_eq!(scripts.get("lint").unwrap(), "eslint .");
    }

    #[test]
    fn exact_match_gets_higher_priority() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "jest", "test:unit": "jest --unit"}}"#,
        )
        .unwrap();

        let detector = NpmDetector;
        let commands = detector.resolve_commands(dir.path());

        let test_commands: Vec<_> = commands
            .iter()
            .filter(|c| c.canonical == CanonicalCommand::Test)
            .collect();

        assert_eq!(test_commands.len(), 2);

        let exact = test_commands
            .iter()
            .find(|c| c.cmd == "npm run test")
            .unwrap();
        let prefix = test_commands
            .iter()
            .find(|c| c.cmd == "npm run test:unit")
            .unwrap();

        assert_eq!(exact.priority, 10);
        assert_eq!(prefix.priority, 5);
    }

    #[test]
    fn content_aware_lint_is_prettier() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"lint": "prettier --check .", "format": "prettier --write ."}}"#,
        )
        .unwrap();

        let detector = NpmDetector;
        let commands = detector.resolve_commands(dir.path());

        // The lint name carries it; the content is only a format-verify.
        let lint_cmd = commands.iter().find(|c| c.cmd == "npm run lint").unwrap();
        assert_eq!(lint_cmd.canonical, CanonicalCommand::Lint);
        assert_eq!(lint_cmd.priority, 10);

        let format_cmd = commands.iter().find(|c| c.cmd == "npm run format").unwrap();
        assert_eq!(format_cmd.canonical, CanonicalCommand::Format);
        assert_eq!(format_cmd.priority, 10);
    }

    #[test]
    fn scripts_in_deterministic_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test:e2e": "cypress", "test:unit": "jest"}}"#,
        )
        .unwrap();

        let scripts = read_package_json_scripts(dir.path()).unwrap();
        let keys: Vec<_> = scripts.keys().collect();
        // BTreeMap ensures alphabetical order
        assert_eq!(keys, vec!["test:e2e", "test:unit"]);
    }
}
