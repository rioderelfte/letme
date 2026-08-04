use std::path::Path;

use crate::detect::*;
use crate::detectors::npm::read_package_json_scripts;

pub struct YarnDetector;

impl Detector for YarnDetector {
    fn name(&self) -> &str {
        "yarn"
    }

    fn tier(&self) -> Tier {
        Tier::Tier3
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn required_binaries(&self) -> &[&str] {
        &["yarn"]
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("yarn.lock").exists() && dir.join("package.json").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let mut commands = Vec::new();

        commands.push(self.make_command(CanonicalCommand::Install, "yarn install".into(), 10));
        commands.push(self.make_command(CanonicalCommand::Clean, "rm -rf node_modules".into(), 10));

        if let Some(scripts) = read_package_json_scripts(dir) {
            for (name, value) in &scripts {
                if let Some((canonical, priority)) = map_script(name, value) {
                    let cmd = format!("yarn run {name}");
                    let mut rc = self.make_command(canonical, cmd, priority);
                    rc.label = value.clone();
                    commands.push(rc);
                }
            }
        }

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_with_yarn_lock_and_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let detector = YarnDetector;
        assert!(detector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_yarn_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        let detector = YarnDetector;
        assert!(!detector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let detector = YarnDetector;
        assert!(!detector.detect(dir.path()));
    }

    #[test]
    fn parses_scripts_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "jest", "build": "tsc"}}"#,
        )
        .unwrap();

        let detector = YarnDetector;
        let commands = detector.resolve_commands(dir.path());

        let test_cmd = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Test);
        assert!(test_cmd.is_some());
        assert_eq!(test_cmd.unwrap().cmd, "yarn run test");
    }

    #[test]
    fn yarn_beats_npm_via_group_exclusion() {
        use crate::detect::{self, CanonicalCommand};
        use crate::detectors::npm::NpmDetector;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "jest"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        // Both detect individually (npm no longer checks for yarn.lock)
        let npm = NpmDetector;
        let yarn = YarnDetector;
        assert!(npm.detect(dir.path()));
        assert!(yarn.detect(dir.path()));

        // But in an exclusive group, yarn wins and npm is skipped
        let groups = vec![detect::DetectorGroup::new(vec![
            Box::new(AssumeInstalled(YarnDetector)),
            Box::new(AssumeInstalled(NpmDetector)),
        ])];
        let result = detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].detector_name, "yarn");
        assert_eq!(result[0].cmd, "yarn run test");
    }
}
