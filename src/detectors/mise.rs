use std::path::{Path, PathBuf};

use crate::detect::{Detector, Ecosystem, ResolvedCommand, Tier, map_canonical_name};

/// The mise task list, loaded once per invocation in `main` and shared between
/// the untrusted-config warning and [`MiseDetector`].
#[derive(Default)]
pub struct MiseTasks {
    tasks: Vec<String>,
    untrusted: bool,
}

impl MiseTasks {
    /// Run `mise tasks --json` in `dir`, if there is a config and a binary.
    pub fn load(dir: &Path) -> Self {
        if config_path(dir).is_none() || which::which("mise").is_err() {
            return Self::default();
        }

        let Ok(output) = std::process::Command::new("mise")
            .args(["tasks", "--json"])
            .current_dir(dir)
            .output()
        else {
            return Self::default();
        };

        if !output.status.success() {
            return Self {
                tasks: Vec::new(),
                untrusted: is_untrusted_error(&String::from_utf8_lossy(&output.stderr)),
            };
        }

        Self {
            tasks: parse_mise_tasks(&output.stdout),
            untrusted: false,
        }
    }
}

pub struct MiseDetector {
    tasks: Vec<String>,
}

impl MiseDetector {
    pub fn new(mise_tasks: &MiseTasks) -> Self {
        Self {
            tasks: mise_tasks.tasks.clone(),
        }
    }
}

impl Detector for MiseDetector {
    fn name(&self) -> &str {
        "mise"
    }

    fn tier(&self) -> Tier {
        Tier::Tier2
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::TaskRunner
    }

    fn required_binaries(&self) -> &[&str] {
        &["mise"]
    }

    fn detect(&self, dir: &Path) -> bool {
        config_path(dir).is_some()
    }

    fn resolve_commands(&self, _dir: &Path) -> Vec<ResolvedCommand> {
        let mut commands = Vec::new();

        for task in &self.tasks {
            if let Some(canonical) = map_canonical_name(task) {
                commands.push(self.make_command(canonical, format!("mise run {task}"), 5));
            }
        }

        commands
    }
}

/// The mise config file in `dir`, if there is one.
fn config_path(dir: &Path) -> Option<PathBuf> {
    [".mise.toml", "mise.toml"]
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
}

fn parse_mise_tasks(stdout: &[u8]) -> Vec<String> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(stdout) else {
        return Vec::new();
    };

    let Some(arr) = json.as_array() else {
        return Vec::new();
    };

    arr.iter()
        .filter(|task| {
            !task
                .get("global")
                .and_then(|global| global.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|task| task.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect()
}

pub fn warn_untrusted_config(mise_tasks: &MiseTasks, theme: &crate::theme::Theme) {
    if !mise_tasks.untrusted {
        return;
    }

    use owo_colors::OwoColorize;
    eprintln!(
        "{} {} {} {}",
        "Warning:".style(theme.warning),
        "mise config is not trusted; its tasks were not read. Run".style(theme.muted),
        "mise trust".style(theme.command),
        "to include them.".style(theme.muted)
    );
}

fn is_untrusted_error(stderr: &str) -> bool {
    stderr.contains("not trusted") || stderr.contains("mise trust")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mise_tasks_skips_global_tasks() {
        let json = br#"[
            {"name": "test", "global": false},
            {"name": "lint", "global": true},
            {"name": "build", "global": false}
        ]"#;

        assert_eq!(parse_mise_tasks(json), vec!["test", "build"]);
    }

    #[test]
    fn parse_mise_tasks_keeps_tasks_without_global_field() {
        let json = br#"[{"name": "test"}]"#;

        assert_eq!(parse_mise_tasks(json), vec!["test"]);
    }

    #[test]
    fn parse_mise_tasks_handles_malformed_output() {
        assert!(parse_mise_tasks(b"not json").is_empty());
        assert!(parse_mise_tasks(br#"{"tasks": []}"#).is_empty());
        assert!(parse_mise_tasks(br#"[{"description": "no name"}]"#).is_empty());
    }

    #[test]
    fn detector_resolves_only_canonical_task_names() {
        let mise_tasks = MiseTasks {
            tasks: vec!["test".into(), "deploy".into()],
            untrusted: false,
        };

        let commands = MiseDetector::new(&mise_tasks).resolve_commands(Path::new("."));

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].cmd, "mise run test");
    }

    #[test]
    fn is_untrusted_error_matches_mise_wording() {
        let stderr = "mise ERROR error parsing config file: /repo/mise.toml\n\
             mise ERROR Config files in /repo/mise.toml are not trusted.\n\
             Trust them with `mise trust`.";

        assert!(is_untrusted_error(stderr));
    }

    #[test]
    fn is_untrusted_error_ignores_unrelated_failures() {
        assert!(!is_untrusted_error(
            "mise ERROR failed to parse /repo/mise.toml: expected `=`"
        ));
    }
}
