use std::path::Path;

/// Check if any file starting with `prefix` exists in the directory.
pub fn has_file_with_prefix(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
            && let Some(name) = entry.file_name().to_str()
            && name.starts_with(prefix)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_matching_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("jest.config.js"), "").unwrap();

        assert!(has_file_with_prefix(dir.path(), "jest.config"));
    }

    #[test]
    fn returns_false_for_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other.txt"), "").unwrap();

        assert!(!has_file_with_prefix(dir.path(), "jest.config"));
    }

    #[test]
    fn returns_false_for_directory_with_matching_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("jest.config.cache")).unwrap();

        assert!(!has_file_with_prefix(dir.path(), "jest.config"));
    }
}
