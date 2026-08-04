use std::path::Path;

use crate::detect::*;
use crate::detectors::npm::read_package_json_scripts;

pub struct PnpmDetector;

impl Detector for PnpmDetector {
    fn name(&self) -> &str {
        "pnpm"
    }

    fn tier(&self) -> Tier {
        Tier::Tier3
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn required_binaries(&self) -> &[&str] {
        &["pnpm"]
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("pnpm-lock.yaml").exists() && dir.join("package.json").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let mut commands = Vec::new();

        commands.push(self.make_command(CanonicalCommand::Install, "pnpm install".into(), 10));
        commands.push(self.make_command(CanonicalCommand::Clean, "rm -rf node_modules".into(), 10));

        if let Some(scripts) = read_package_json_scripts(dir) {
            for (name, value) in &scripts {
                if let Some((canonical, priority)) = map_script(name, value) {
                    let cmd = format!("pnpm run {name}");
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
    fn detects_with_pnpm_lock_and_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let detector = PnpmDetector;
        assert!(detector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_pnpm_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        let detector = PnpmDetector;
        assert!(!detector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let detector = PnpmDetector;
        assert!(!detector.detect(dir.path()));
    }

    #[test]
    fn parses_scripts_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "vitest run", "build": "tsc"}}"#,
        )
        .unwrap();

        let detector = PnpmDetector;
        let commands = detector.resolve_commands(dir.path());

        let test_cmd = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Test);
        assert!(test_cmd.is_some());
        assert_eq!(test_cmd.unwrap().cmd, "pnpm run test");
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

        let commands = PnpmDetector.resolve_commands(dir.path());

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
    fn resolves_install_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        let commands = PnpmDetector.resolve_commands(dir.path());

        let install = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Install)
            .unwrap();
        assert_eq!(install.cmd, "pnpm install");

        let clean = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Clean)
            .unwrap();
        assert_eq!(clean.cmd, "rm -rf node_modules");
    }

    #[test]
    fn pnpm_beats_yarn_and_npm_via_group_exclusion() {
        use crate::detect::{self, CanonicalCommand};
        use crate::detectors::npm::NpmDetector;
        use crate::detectors::yarn::YarnDetector;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "vitest run"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        // All three detect individually
        assert!(PnpmDetector.detect(dir.path()));
        assert!(YarnDetector.detect(dir.path()));
        assert!(NpmDetector.detect(dir.path()));

        // But in an exclusive group, pnpm wins and the rest are skipped
        let groups = vec![detect::DetectorGroup::new(vec![
            Box::new(AssumeInstalled(PnpmDetector)),
            Box::new(AssumeInstalled(YarnDetector)),
            Box::new(AssumeInstalled(NpmDetector)),
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
            Box::new(AssumeInstalled(PnpmDetector)) as Box<dyn Detector>,
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
                Box::new(AssumeInstalled(PnpmDetector)) as Box<dyn Detector>
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
                Box::new(AssumeInstalled(PnpmDetector)) as Box<dyn Detector>
            ]),
            detect::DetectorGroup::new(vec![Box::new(PlaywrightDetector) as Box<dyn Detector>]),
        ];
        let result = detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::E2e], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "pnpm run e2e");
        assert_eq!(result[0].detector_name, "pnpm");
    }
}
