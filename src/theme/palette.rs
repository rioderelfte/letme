use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Default, Deserialize)]
pub struct PaletteFile {
    pub colors: Option<HashMap<String, String>>,
}

pub fn load_palette(name: &str) -> Option<HashMap<String, String>> {
    let path = palette_path(name)?;
    let contents = std::fs::read_to_string(path).ok()?;
    let file: PaletteFile = toml::from_str(&contents).ok()?;
    file.colors
}

fn palette_path(name: &str) -> Option<PathBuf> {
    Some(
        config::dirs_base()
            .ok()?
            .join("palettes")
            .join(format!("{name}.toml")),
    )
}
