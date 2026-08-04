use std::path::Path;

use crate::detect::*;

pub struct CargoDetector;

impl Detector for CargoDetector {
    fn name(&self) -> &str {
        "cargo"
    }

    fn tier(&self) -> Tier {
        Tier::Tier4
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Rust
    }

    fn required_binaries(&self) -> &[&str] {
        &["cargo"]
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("Cargo.toml").exists()
    }

    fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
        vec![
            self.make_command(CanonicalCommand::Test, "cargo test".into(), 10),
            self.make_command(CanonicalCommand::Lint, "cargo clippy".into(), 10),
            self.make_command(
                CanonicalCommand::Fix,
                "cargo clippy --fix --allow-dirty --allow-staged".into(),
                10,
            ),
            self.make_command(CanonicalCommand::Format, "cargo fmt".into(), 10),
            self.make_command(CanonicalCommand::Build, "cargo build".into(), 10),
            self.make_command(CanonicalCommand::Clean, "cargo clean".into(), 10),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_with_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        assert!(CargoDetector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!CargoDetector.detect(dir.path()));
    }

    #[test]
    fn resolve_provides_all_commands() {
        let dir = tempfile::tempdir().unwrap();
        let commands = CargoDetector.resolve_commands(dir.path());

        let canonicals: Vec<_> = commands.iter().map(|c| c.canonical).collect();
        assert!(canonicals.contains(&CanonicalCommand::Test));
        assert!(canonicals.contains(&CanonicalCommand::Lint));
        assert!(canonicals.contains(&CanonicalCommand::Fix));
        assert!(canonicals.contains(&CanonicalCommand::Format));
        assert!(canonicals.contains(&CanonicalCommand::Build));
        assert!(canonicals.contains(&CanonicalCommand::Clean));
        assert_eq!(commands.len(), 6);
    }
}
