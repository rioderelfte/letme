use std::collections::BTreeMap;
use std::path::Path;

use crate::detect::*;
use crate::detectors::js::JsPackageManager;

/// Tier 3 detector for package.json scripts, parameterized by the package
/// manager that drives them.
pub struct PackageJsonDetector {
    manager: JsPackageManager,
    binaries: [&'static str; 1],
}

impl PackageJsonDetector {
    pub fn new(manager: JsPackageManager) -> Self {
        Self {
            manager,
            binaries: [manager.binary()],
        }
    }
}

impl Detector for PackageJsonDetector {
    fn name(&self) -> &str {
        self.manager.binary()
    }

    fn tier(&self) -> Tier {
        Tier::Tier3
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn required_binaries(&self) -> &[&str] {
        &self.binaries
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("package.json").exists() {
            return false;
        }
        // npm is the fallback manager: a bare package.json is enough. Group
        // ordering keeps it from claiming pnpm/yarn projects.
        self.manager == JsPackageManager::Npm || dir.join(self.manager.lockfile()).exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let mut commands = Vec::new();

        commands.push(self.make_command(
            CanonicalCommand::Install,
            self.manager.install_command().into(),
            10,
        ));
        commands.push(self.make_command(CanonicalCommand::Clean, "rm -rf node_modules".into(), 10));

        if let Some(scripts) = read_package_json_scripts(dir) {
            for (name, value) in &scripts {
                if let Some((canonical, priority)) = map_script(name, value) {
                    let cmd = format!("{} {name}", self.manager.run_prefix());
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
    use JsPackageManager::{Npm, Pnpm, Yarn};

    fn detector(manager: JsPackageManager) -> PackageJsonDetector {
        PackageJsonDetector::new(manager)
    }

    #[test]
    fn npm_detects_with_bare_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        assert!(detector(Npm).detect(dir.path()));
    }

    #[test]
    fn npm_detects_even_with_yarn_lock() {
        // Group ordering keeps npm from claiming yarn projects, not detect() itself.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        assert!(detector(Npm).detect(dir.path()));
    }

    #[test]
    fn lockfile_managers_require_their_lockfile() {
        for manager in [Pnpm, Yarn] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
            assert!(!detector(manager).detect(dir.path()));

            std::fs::write(dir.path().join(manager.lockfile()), "").unwrap();
            assert!(detector(manager).detect(dir.path()));
        }
    }

    #[test]
    fn nothing_detects_without_package_json() {
        for manager in [Pnpm, Yarn, Npm] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(manager.lockfile()), "").unwrap();

            assert!(!detector(manager).detect(dir.path()));
        }
    }

    #[test]
    fn each_manager_uses_its_own_commands() {
        let cases = [
            (Npm, "npm run test", "npm install"),
            (Yarn, "yarn run test", "yarn install"),
            (Pnpm, "pnpm run test", "pnpm install"),
        ];
        for (manager, run, install) in cases {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("package.json"),
                r#"{"scripts": {"test": "jest"}}"#,
            )
            .unwrap();

            let commands = detector(manager).resolve_commands(dir.path());

            let test_cmd = commands
                .iter()
                .find(|c| c.canonical == CanonicalCommand::Test)
                .unwrap();
            assert_eq!(test_cmd.cmd, run);

            let install_cmd = commands
                .iter()
                .find(|c| c.canonical == CanonicalCommand::Install)
                .unwrap();
            assert_eq!(install_cmd.cmd, install);

            let clean_cmd = commands
                .iter()
                .find(|c| c.canonical == CanonicalCommand::Clean)
                .unwrap();
            assert_eq!(clean_cmd.cmd, "rm -rf node_modules");
        }
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

    #[test]
    fn exact_match_gets_higher_priority() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "jest", "test:unit": "jest --unit"}}"#,
        )
        .unwrap();

        let commands = detector(Npm).resolve_commands(dir.path());

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

        let commands = detector(Npm).resolve_commands(dir.path());

        // The lint name carries it; the content is only a format-verify.
        let lint_cmd = commands.iter().find(|c| c.cmd == "npm run lint").unwrap();
        assert_eq!(lint_cmd.canonical, CanonicalCommand::Lint);
        assert_eq!(lint_cmd.priority, 10);

        let format_cmd = commands.iter().find(|c| c.cmd == "npm run format").unwrap();
        assert_eq!(format_cmd.canonical, CanonicalCommand::Format);
        assert_eq!(format_cmd.priority, 10);
    }

    #[test]
    fn check_only_format_script_yields_no_format_command() {
        // `"format": "biome format"` only reports; it never writes, so pnpm
        // must not claim the Format slot. With no tier-3 candidate, resolution
        // falls through to the tier-4 biome detector's `biome format --write`.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"format": "biome format", "lint": "biome lint", "test": "vitest run"}}"#,
        )
        .unwrap();

        let commands = detector(Pnpm).resolve_commands(dir.path());

        assert!(
            !commands
                .iter()
                .any(|c| c.canonical == CanonicalCommand::Format),
            "pnpm should not offer a Format command for a check-only script"
        );
        // Sibling scripts are unaffected.
        assert!(commands.iter().any(|c| c.cmd == "pnpm run lint"));
        assert!(commands.iter().any(|c| c.cmd == "pnpm run test"));
    }

    #[test]
    fn yarn_beats_npm_via_group_exclusion() {
        use crate::detect::{self, CanonicalCommand};

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "jest"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        // Both detect individually (npm does not check for yarn.lock)
        assert!(detector(Npm).detect(dir.path()));
        assert!(detector(Yarn).detect(dir.path()));

        // But in an exclusive group, yarn wins and npm is skipped
        let groups = vec![detect::DetectorGroup::new(vec![
            Box::new(AssumeInstalled(detector(Yarn))),
            Box::new(AssumeInstalled(detector(Npm))),
        ])];
        let result = detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].detector_name, "yarn");
        assert_eq!(result[0].cmd, "yarn run test");
    }

    #[test]
    fn pnpm_beats_yarn_and_npm_via_group_exclusion() {
        use crate::detect::{self, CanonicalCommand};

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "vitest run"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        // All three detect individually
        assert!(detector(Pnpm).detect(dir.path()));
        assert!(detector(Yarn).detect(dir.path()));
        assert!(detector(Npm).detect(dir.path()));

        // But in an exclusive group, pnpm wins and the rest are skipped
        let groups = vec![detect::DetectorGroup::new(vec![
            Box::new(AssumeInstalled(detector(Pnpm))),
            Box::new(AssumeInstalled(detector(Yarn))),
            Box::new(AssumeInstalled(detector(Npm))),
        ])];
        let result = detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::Test], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].detector_name, "pnpm");
        assert_eq!(result[0].cmd, "pnpm run test");
    }

    #[test]
    fn typecheck_and_type_check_scripts_yield_one_command() {
        // Both names map to Typecheck at exact priority. The ecosystem slot is
        // single-valued and ties keep the first candidate, which by BTreeMap
        // script order is "type-check" ('-' sorts before 'c'). Same behavior as
        // a "lint" + "check" collision.
        use crate::detect::{self, CanonicalCommand};

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"typecheck": "tsc --noEmit", "type-check": "tsc --noEmit"}}"#,
        )
        .unwrap();

        let groups = vec![detect::DetectorGroup::new(vec![
            Box::new(AssumeInstalled(detector(Pnpm))) as Box<dyn Detector>,
        ])];
        let result =
            detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::Typecheck], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "pnpm run type-check");
    }

    #[test]
    fn typecheck_script_beats_tsc_convention() {
        // Tier 3 script and tier 4 tsc convention are both JavaScript; the
        // script wins on tier, so the convention never double-runs.
        use crate::detect::{self, CanonicalCommand};
        use crate::detectors::node_conventions::TscDetector;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"typecheck": "tsc --noEmit"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        let bin = dir.path().join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tsc"), "").unwrap();

        assert!(TscDetector.detect(dir.path()));

        let groups = vec![
            detect::DetectorGroup::new(vec![
                Box::new(AssumeInstalled(detector(Pnpm))) as Box<dyn Detector>
            ]),
            detect::DetectorGroup::new(vec![Box::new(TscDetector) as Box<dyn Detector>]),
        ];
        let result =
            detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::Typecheck], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "pnpm run typecheck");
        assert_eq!(result[0].detector_name, "pnpm");
    }

    #[test]
    fn e2e_script_beats_playwright_convention() {
        // Tier 3 script and tier 4 playwright convention are both JavaScript;
        // the script wins on tier, so the convention never double-runs.
        use crate::detect::{self, CanonicalCommand};
        use crate::detectors::node_conventions::PlaywrightDetector;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"e2e": "playwright test"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("playwright.config.ts"), "").unwrap();
        let bin = dir.path().join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("playwright"), "").unwrap();

        assert!(PlaywrightDetector.detect(dir.path()));

        let groups = vec![
            detect::DetectorGroup::new(vec![
                Box::new(AssumeInstalled(detector(Pnpm))) as Box<dyn Detector>
            ]),
            detect::DetectorGroup::new(vec![Box::new(PlaywrightDetector) as Box<dyn Detector>]),
        ];
        let result = detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::E2e], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "pnpm run e2e");
        assert_eq!(result[0].detector_name, "pnpm");
    }
}
