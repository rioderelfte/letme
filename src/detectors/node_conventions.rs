use std::path::Path;

use super::js;
use super::util::has_file_with_prefix;
use crate::detect::*;

pub struct VitestDetector;
pub struct JestDetector;
pub struct EslintDetector;
pub struct PrettierDetector;
pub struct BiomeDetector;
pub struct TscDetector;
pub struct PlaywrightDetector;
pub struct CypressDetector;

impl Detector for VitestDetector {
    fn name(&self) -> &str {
        "vitest"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("node_modules/.bin/vitest").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![self.make_command(CanonicalCommand::Test, format!("{exec} vitest run"), 10)]
    }
}

impl Detector for JestDetector {
    fn name(&self) -> &str {
        "jest"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("node_modules/.bin/jest").exists() {
            return false;
        }
        // Needs jest.config.* or package.json[jest]
        has_file_with_prefix(dir, "jest.config") || has_jest_key_in_package_json(dir)
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![self.make_command(CanonicalCommand::Test, format!("{exec} jest"), 5)]
    }
}

impl Detector for EslintDetector {
    fn name(&self) -> &str {
        "eslint"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("node_modules/.bin/eslint").exists() {
            return false;
        }
        has_file_with_prefix(dir, "eslint.config")
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![
            self.make_command(CanonicalCommand::Lint, format!("{exec} eslint ."), 5),
            self.make_command(CanonicalCommand::Fix, format!("{exec} eslint --fix ."), 5),
        ]
    }
}

impl Detector for PrettierDetector {
    fn name(&self) -> &str {
        "prettier"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("node_modules/.bin/prettier").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![self.make_command(
            CanonicalCommand::Format,
            format!("{exec} prettier --write ."),
            5,
        )]
    }
}

impl Detector for BiomeDetector {
    fn name(&self) -> &str {
        "biome"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("node_modules/.bin/biome").exists() {
            return false;
        }
        has_file_with_prefix(dir, "biome.json")
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![
            self.make_command(CanonicalCommand::Lint, format!("{exec} biome check"), 10),
            self.make_command(
                CanonicalCommand::Fix,
                format!("{exec} biome lint --fix ."),
                10,
            ),
            self.make_command(
                CanonicalCommand::Format,
                format!("{exec} biome format --write"),
                10,
            ),
        ]
    }
}

impl Detector for TscDetector {
    fn name(&self) -> &str {
        "tsc"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("node_modules/.bin/tsc").exists() && dir.join("tsconfig.json").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![self.make_command(
            CanonicalCommand::Typecheck,
            format!("{exec} tsc --noEmit"),
            10,
        )]
    }
}

impl Detector for PlaywrightDetector {
    fn name(&self) -> &str {
        "playwright"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("node_modules/.bin/playwright").exists() {
            return false;
        }
        has_file_with_prefix(dir, "playwright.config")
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![self.make_command(CanonicalCommand::E2e, format!("{exec} playwright test"), 10)]
    }
}

impl Detector for CypressDetector {
    fn name(&self) -> &str {
        "cypress"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::JavaScript
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("node_modules/.bin/cypress").exists() {
            return false;
        }
        has_file_with_prefix(dir, "cypress.config")
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        vec![self.make_command(CanonicalCommand::E2e, format!("{exec} cypress run"), 5)]
    }
}

fn has_jest_key_in_package_json(dir: &Path) -> bool {
    let path = dir.join("package.json");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };
    json.get("jest").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node_bin(dir: &Path, name: &str) {
        let bin = dir.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(name), "").unwrap();
    }

    #[test]
    fn vitest_detects_with_binary() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "vitest");

        assert!(VitestDetector.detect(dir.path()));
    }

    #[test]
    fn vitest_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!VitestDetector.detect(dir.path()));
    }

    #[test]
    fn jest_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "jest");
        std::fs::write(dir.path().join("jest.config.js"), "").unwrap();

        assert!(JestDetector.detect(dir.path()));
    }

    #[test]
    fn jest_detects_with_binary_and_package_json_jest_key() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "jest");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"jest": {"testEnvironment": "node"}}"#,
        )
        .unwrap();

        assert!(JestDetector.detect(dir.path()));
    }

    #[test]
    fn jest_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "jest");

        assert!(!JestDetector.detect(dir.path()));
    }

    #[test]
    fn jest_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("jest.config.js"), "").unwrap();

        assert!(!JestDetector.detect(dir.path()));
    }

    #[test]
    fn eslint_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "eslint");
        std::fs::write(dir.path().join("eslint.config.js"), "").unwrap();

        assert!(EslintDetector.detect(dir.path()));
    }

    #[test]
    fn eslint_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "eslint");

        assert!(!EslintDetector.detect(dir.path()));
    }

    #[test]
    fn eslint_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("eslint.config.mjs"), "").unwrap();

        assert!(!EslintDetector.detect(dir.path()));
    }

    #[test]
    fn prettier_detects_with_binary() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "prettier");

        assert!(PrettierDetector.detect(dir.path()));
    }

    #[test]
    fn prettier_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!PrettierDetector.detect(dir.path()));
    }

    #[test]
    fn biome_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "biome");
        std::fs::write(dir.path().join("biome.json"), "{}").unwrap();

        assert!(BiomeDetector.detect(dir.path()));
    }

    #[test]
    fn biome_detects_with_jsonc_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "biome");
        std::fs::write(dir.path().join("biome.jsonc"), "{}").unwrap();

        assert!(BiomeDetector.detect(dir.path()));
    }

    #[test]
    fn biome_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "biome");

        assert!(!BiomeDetector.detect(dir.path()));
    }

    #[test]
    fn biome_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("biome.json"), "{}").unwrap();

        assert!(!BiomeDetector.detect(dir.path()));
    }

    #[test]
    fn tsc_detects_with_binary_and_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "tsc");
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        assert!(TscDetector.detect(dir.path()));
    }

    #[test]
    fn tsc_does_not_detect_without_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "tsc");

        assert!(!TscDetector.detect(dir.path()));
    }

    #[test]
    fn tsc_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        assert!(!TscDetector.detect(dir.path()));
    }

    #[test]
    fn tsc_resolves_typecheck_command() {
        let dir = tempfile::tempdir().unwrap();
        let commands = TscDetector.resolve_commands(dir.path());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].canonical, CanonicalCommand::Typecheck);
        assert_eq!(commands[0].cmd, "npx tsc --noEmit");
    }

    #[test]
    fn playwright_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "playwright");
        std::fs::write(dir.path().join("playwright.config.ts"), "").unwrap();

        assert!(PlaywrightDetector.detect(dir.path()));
    }

    #[test]
    fn playwright_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "playwright");

        assert!(!PlaywrightDetector.detect(dir.path()));
    }

    #[test]
    fn playwright_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("playwright.config.ts"), "").unwrap();

        assert!(!PlaywrightDetector.detect(dir.path()));
    }

    #[test]
    fn cypress_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "cypress");
        std::fs::write(dir.path().join("cypress.config.js"), "").unwrap();

        assert!(CypressDetector.detect(dir.path()));
    }

    #[test]
    fn cypress_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "cypress");

        assert!(!CypressDetector.detect(dir.path()));
    }

    #[test]
    fn cypress_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cypress.config.js"), "").unwrap();

        assert!(!CypressDetector.detect(dir.path()));
    }

    #[test]
    fn playwright_beats_cypress_within_ecosystem() {
        use crate::detect;

        let dir = tempfile::tempdir().unwrap();
        make_node_bin(dir.path(), "playwright");
        make_node_bin(dir.path(), "cypress");
        std::fs::write(dir.path().join("playwright.config.ts"), "").unwrap();
        std::fs::write(dir.path().join("cypress.config.js"), "").unwrap();

        let groups = vec![
            detect::DetectorGroup::new(vec![
                Box::new(PlaywrightDetector) as Box<dyn detect::Detector>
            ]),
            detect::DetectorGroup::new(
                vec![Box::new(CypressDetector) as Box<dyn detect::Detector>],
            ),
        ];
        let result = detect::resolve_all(&groups, dir.path(), &[CanonicalCommand::E2e], false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "npx playwright test");
        assert_eq!(result[0].detector_name, "playwright");
    }

    #[test]
    fn eslint_resolves_fix_command() {
        let dir = tempfile::tempdir().unwrap();
        let commands = EslintDetector.resolve_commands(dir.path());
        let fix = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Fix);
        assert!(fix.is_some());
        assert_eq!(fix.unwrap().cmd, "npx eslint --fix .");
    }

    #[test]
    fn biome_resolves_fix_command() {
        let dir = tempfile::tempdir().unwrap();
        let commands = BiomeDetector.resolve_commands(dir.path());
        let fix = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Fix);
        assert!(fix.is_some());
        assert_eq!(fix.unwrap().cmd, "npx biome lint --fix .");
    }

    #[test]
    fn exec_prefix_is_pnpm_with_pnpm_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let commands = EslintDetector.resolve_commands(dir.path());
        let fix = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Fix)
            .unwrap();
        assert_eq!(fix.cmd, "pnpm exec eslint --fix .");
    }

    #[test]
    fn exec_prefix_is_yarn_with_yarn_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let commands = EslintDetector.resolve_commands(dir.path());
        let fix = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Fix)
            .unwrap();
        assert_eq!(fix.cmd, "yarn eslint --fix .");
    }

    #[test]
    fn exec_prefix_falls_back_to_npx() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let commands = EslintDetector.resolve_commands(dir.path());
        let fix = commands
            .iter()
            .find(|c| c.canonical == CanonicalCommand::Fix)
            .unwrap();
        assert_eq!(fix.cmd, "npx eslint --fix .");
    }

    #[test]
    fn exec_prefix_applies_to_all_node_detectors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let all: Vec<String> = [
            VitestDetector.resolve_commands(dir.path()),
            JestDetector.resolve_commands(dir.path()),
            EslintDetector.resolve_commands(dir.path()),
            PrettierDetector.resolve_commands(dir.path()),
            BiomeDetector.resolve_commands(dir.path()),
            TscDetector.resolve_commands(dir.path()),
            PlaywrightDetector.resolve_commands(dir.path()),
            CypressDetector.resolve_commands(dir.path()),
        ]
        .concat()
        .iter()
        .map(|c| c.cmd.clone())
        .collect();

        assert!(!all.is_empty());
        for cmd in &all {
            assert!(cmd.starts_with("pnpm exec "), "not prefixed: {cmd}");
        }
    }
}
