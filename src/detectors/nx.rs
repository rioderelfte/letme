use std::collections::BTreeSet;
use std::path::Path;

use super::js;
use crate::detect::{
    CanonicalCommand, Detector, Ecosystem, ResolvedCommand, Tier, map_canonical_name,
};

pub struct NxDetector;

impl Detector for NxDetector {
    fn name(&self) -> &str {
        "nx"
    }

    fn tier(&self) -> Tier {
        Tier::Tier2
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::TaskRunner
    }

    fn detect(&self, dir: &Path) -> bool {
        dir.join("nx.json").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let exec = js::detect_manager(dir).exec_prefix();
        let mut commands = Vec::new();

        for target in workspace_targets(dir) {
            let Some(canonical) = map_canonical_name(&target) else {
                continue;
            };
            // install/clean keep their package-manager meaning at tier 3;
            // nx targets with those names do something else.
            if matches!(
                canonical,
                CanonicalCommand::Install | CanonicalCommand::Clean
            ) {
                continue;
            }
            commands.push(self.make_command(
                canonical,
                format!("{exec} nx run-many -t {target} --outputStyle=stream"),
                3,
            ));
        }

        commands
    }
}

fn workspace_targets(dir: &Path) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    collect_nx_json_targets(dir, &mut targets);
    collect_project_json_targets(dir, &mut targets);
    targets
}

fn collect_nx_json_targets(dir: &Path, targets: &mut BTreeSet<String>) {
    let Some(json) = read_json(&dir.join("nx.json")) else {
        return;
    };

    if let Some(defaults) = json.get("targetDefaults").and_then(|v| v.as_object()) {
        targets.extend(defaults.keys().cloned());
    }

    // Inference plugins name the targets they generate in their options:
    // {"plugin": "@nx/vite/plugin", "options": {"testTargetName": "test"}}
    let Some(plugins) = json.get("plugins").and_then(|v| v.as_array()) else {
        return;
    };
    for plugin in plugins {
        let Some(options) = plugin.get("options").and_then(|v| v.as_object()) else {
            continue;
        };
        for (key, value) in options {
            if key.ends_with("TargetName")
                && let Some(name) = value.as_str()
            {
                targets.insert(name.to_string());
            }
        }
    }
}

/// Collect target names from every `project.json` under `root`, skipping
/// node_modules and hidden directories, without following symlinks.
fn collect_project_json_targets(root: &Path, targets: &mut BTreeSet<String>) {
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            if file_type.is_dir() {
                if name != "node_modules" && !name.to_string_lossy().starts_with('.') {
                    pending.push(entry.path());
                }
            } else if name == "project.json"
                && let Some(json) = read_json(&entry.path())
                && let Some(project_targets) = json.get("targets").and_then(|v| v.as_object())
            {
                targets.extend(project_targets.keys().cloned());
            }
        }
    }
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn commands_in(dir: &Path) -> Vec<ResolvedCommand> {
        NxDetector.resolve_commands(dir)
    }

    #[test]
    fn detects_with_nx_json() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");

        assert!(NxDetector.detect(dir.path()));
    }

    #[test]
    fn does_not_detect_without_nx_json() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package.json", "{}");

        assert!(!NxDetector.detect(dir.path()));
    }

    #[test]
    fn resolves_targets_from_project_json_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(
            dir.path(),
            "app/web/project.json",
            r#"{"targets": {"test": {}, "build": {}, "serve": {}}}"#,
        );

        let commands = commands_in(dir.path());

        // No lockfile, so the exec prefix falls back to npx.
        assert!(
            commands
                .iter()
                .any(|c| c.cmd == "npx nx run-many -t test --outputStyle=stream")
        );
        assert!(
            commands
                .iter()
                .any(|c| c.cmd == "npx nx run-many -t build --outputStyle=stream")
        );
        // "serve" has no canonical slot.
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn exec_prefix_follows_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(dir.path(), "pnpm-lock.yaml", "");
        write(
            dir.path(),
            "app/web/project.json",
            r#"{"targets": {"test": {}}}"#,
        );

        let commands = commands_in(dir.path());

        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].cmd,
            "pnpm exec nx run-many -t test --outputStyle=stream"
        );
    }

    #[test]
    fn same_target_across_projects_resolves_once() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(
            dir.path(),
            "app/a/project.json",
            r#"{"targets": {"test": {}}}"#,
        );
        write(
            dir.path(),
            "lib/b/project.json",
            r#"{"targets": {"test": {}}}"#,
        );

        let commands = commands_in(dir.path());

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].canonical, CanonicalCommand::Test);
    }

    #[test]
    fn resolves_target_defaults_keys() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "nx.json",
            r#"{"targetDefaults": {"test": {"cache": true}, "@nx/webpack:webpack": {}}}"#,
        );

        let commands = commands_in(dir.path());

        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].cmd,
            "npx nx run-many -t test --outputStyle=stream"
        );
    }

    #[test]
    fn resolves_plugin_target_name_options() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "nx.json",
            r#"{"plugins": [
                {"plugin": "@nx/vite/plugin",
                 "options": {"testTargetName": "test", "buildTargetName": "build", "serveTargetName": "serve"}},
                "@nx/eslint/plugin"
            ]}"#,
        );

        let commands = commands_in(dir.path());

        // "serve" has no canonical slot; the string-form plugin is skipped.
        assert!(
            commands
                .iter()
                .any(|c| c.canonical == CanonicalCommand::Test)
        );
        assert!(
            commands
                .iter()
                .any(|c| c.canonical == CanonicalCommand::Build)
        );
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn skips_node_modules_and_hidden_directories() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(
            dir.path(),
            "node_modules/dep/project.json",
            r#"{"targets": {"lint": {}}}"#,
        );
        write(
            dir.path(),
            ".nx/cache/project.json",
            r#"{"targets": {"format": {}}}"#,
        );

        assert!(commands_in(dir.path()).is_empty());
    }

    #[test]
    fn install_and_clean_targets_stay_with_the_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(
            dir.path(),
            "app/a/project.json",
            r#"{"targets": {"install": {}, "clean": {}, "test": {}}}"#,
        );

        let commands = commands_in(dir.path());

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].canonical, CanonicalCommand::Test);
    }

    #[test]
    fn e2e_target_maps_but_variants_do_not() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(
            dir.path(),
            "app/web-e2e/project.json",
            r#"{"targets": {"e2e": {}, "e2e-ui": {}}}"#,
        );

        let commands = commands_in(dir.path());

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].canonical, CanonicalCommand::E2e);
        assert_eq!(
            commands[0].cmd,
            "npx nx run-many -t e2e --outputStyle=stream"
        );
    }

    #[test]
    fn priority_stays_below_task_runner_tasks() {
        // just resolves at 10 and mise at 5, both TaskRunner.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "{}");
        write(
            dir.path(),
            "app/a/project.json",
            r#"{"targets": {"test": {}}}"#,
        );

        let commands = commands_in(dir.path());

        assert!(!commands.is_empty());
        assert!(commands.iter().all(|c| c.priority == 3));
    }

    #[test]
    fn malformed_json_resolves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "nx.json", "not json");
        write(dir.path(), "app/a/project.json", "also not json");

        assert!(NxDetector.detect(dir.path()));
        assert!(commands_in(dir.path()).is_empty());
    }
}
