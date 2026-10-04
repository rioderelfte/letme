use std::path::Path;

use super::util::has_file_with_prefix;
use crate::detect::*;

pub struct PestDetector;
pub struct PhpunitDetector;
pub struct PhpstanDetector;
pub struct PhpCsFixerDetector;

impl Detector for PestDetector {
    fn name(&self) -> &str {
        "pest"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Php
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("vendor/bin/pest").exists()
    }

    fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
        vec![self.make_command(CanonicalCommand::Test, "vendor/bin/pest".into(), 10)]
    }
}

impl Detector for PhpunitDetector {
    fn name(&self) -> &str {
        "phpunit"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Php
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("vendor/bin/phpunit").exists()
    }

    fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
        vec![self.make_command(CanonicalCommand::Test, "vendor/bin/phpunit".into(), 5)]
    }
}

impl Detector for PhpstanDetector {
    fn name(&self) -> &str {
        "phpstan"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Php
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("vendor/bin/phpstan").exists() {
            return false;
        }
        // Needs phpstan.neon or phpstan.neon.dist
        has_file_with_prefix(dir, "phpstan.neon")
    }

    fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
        vec![self.make_command(
            CanonicalCommand::Lint,
            "vendor/bin/phpstan analyse".into(),
            10,
        )]
    }
}

impl Detector for PhpCsFixerDetector {
    fn name(&self) -> &str {
        "php-cs-fixer"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Php
    }

    fn detect(&self, dir: &Path) -> bool {
        if !dir.join("vendor/bin/php-cs-fixer").exists() {
            return false;
        }
        has_file_with_prefix(dir, ".php-cs-fixer")
    }

    fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
        vec![
            self.make_command(
                CanonicalCommand::Format,
                "vendor/bin/php-cs-fixer fix".into(),
                10,
            ),
            self.make_command(
                CanonicalCommand::Format.with(Modifier::Check),
                "vendor/bin/php-cs-fixer fix --dry-run --diff".into(),
                10,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vendor_bin(dir: &Path, name: &str) {
        let bin = dir.join("vendor/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(name), "").unwrap();
    }

    #[test]
    fn pest_detects_with_binary() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "pest");

        assert!(PestDetector.detect(dir.path()));
    }

    #[test]
    fn pest_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!PestDetector.detect(dir.path()));
    }

    #[test]
    fn phpunit_detects_with_binary() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "phpunit");

        assert!(PhpunitDetector.detect(dir.path()));
    }

    #[test]
    fn phpunit_lower_priority_than_pest() {
        let dir = tempfile::tempdir().unwrap();
        let pest_cmds = PestDetector.resolve_commands(dir.path());
        let phpunit_cmds = PhpunitDetector.resolve_commands(dir.path());

        assert!(pest_cmds[0].priority > phpunit_cmds[0].priority);
    }

    #[test]
    fn phpstan_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "phpstan");
        std::fs::write(dir.path().join("phpstan.neon"), "").unwrap();

        assert!(PhpstanDetector.detect(dir.path()));
    }

    #[test]
    fn phpstan_detects_with_dist_config() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "phpstan");
        std::fs::write(dir.path().join("phpstan.neon.dist"), "").unwrap();

        assert!(PhpstanDetector.detect(dir.path()));
    }

    #[test]
    fn phpstan_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "phpstan");

        assert!(!PhpstanDetector.detect(dir.path()));
    }

    #[test]
    fn phpstan_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("phpstan.neon"), "").unwrap();

        assert!(!PhpstanDetector.detect(dir.path()));
    }

    #[test]
    fn php_cs_fixer_detects_with_binary_and_config() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "php-cs-fixer");
        std::fs::write(dir.path().join(".php-cs-fixer.dist.php"), "").unwrap();

        assert!(PhpCsFixerDetector.detect(dir.path()));
    }

    #[test]
    fn php_cs_fixer_does_not_detect_without_config() {
        let dir = tempfile::tempdir().unwrap();
        make_vendor_bin(dir.path(), "php-cs-fixer");

        assert!(!PhpCsFixerDetector.detect(dir.path()));
    }

    #[test]
    fn php_cs_fixer_resolves_format_and_format_check() {
        let dir = tempfile::tempdir().unwrap();
        let commands = PhpCsFixerDetector.resolve_commands(dir.path());

        let format = commands
            .iter()
            .find(|c| c.key == CanonicalCommand::Format.into())
            .unwrap();
        assert_eq!(format.cmd, "vendor/bin/php-cs-fixer fix");

        let check = commands
            .iter()
            .find(|c| c.key == CanonicalCommand::Format.with(Modifier::Check))
            .unwrap();
        assert_eq!(check.cmd, "vendor/bin/php-cs-fixer fix --dry-run --diff");
        assert_eq!(check.priority, 10);

        // The check form above is written by hand, and tier-3 synthesis
        // rewrites the same tool from its own table. They must agree, or
        // `letme format --check` would differ by tier.
        let target = CanonicalCommand::Format.with(Modifier::Check);
        let derived = synthesize_variant(target, &format.cmd).map(|v| v.render(|p| p.to_string()));
        assert_eq!(derived.as_deref(), Some(check.cmd.as_str()));
    }

    #[test]
    fn php_cs_fixer_does_not_detect_without_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".php-cs-fixer.php"), "").unwrap();

        assert!(!PhpCsFixerDetector.detect(dir.path()));
    }
}
