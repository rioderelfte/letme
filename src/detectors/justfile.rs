use std::path::Path;

use crate::detect::{Detector, Ecosystem, ResolvedCommand, Tier, map_canonical_name};

pub struct JustfileDetector;

impl Detector for JustfileDetector {
    fn name(&self) -> &str {
        "justfile"
    }

    fn tier(&self) -> Tier {
        Tier::Tier2
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::TaskRunner
    }

    fn required_binaries(&self) -> &[&str] {
        &["just"]
    }

    fn detect(&self, dir: &Path) -> bool {
        // Check common casings
        dir.join("Justfile").exists()
            || dir.join("justfile").exists()
            || dir.join(".justfile").exists()
    }

    fn resolve_commands(&self, dir: &Path) -> Vec<ResolvedCommand> {
        let recipes = read_just_recipes(dir);
        let mut commands = Vec::new();

        for recipe in &recipes {
            if let Some(canonical) = map_canonical_name(recipe) {
                commands.push(self.make_command(canonical, format!("just {recipe}"), 10));
            }
        }

        commands
    }
}

fn read_just_recipes(dir: &Path) -> Vec<String> {
    let output = std::process::Command::new("just")
        .args(["--dump", "--dump-format", "json"])
        .current_dir(dir)
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };

    let Some(recipes) = json.get("recipes").and_then(|r| r.as_object()) else {
        return Vec::new();
    };

    recipes.keys().cloned().collect()
}
