use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::debug;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptsConfig {
    scripts: HashMap<String, String>,
}

impl ScriptsConfig {
    pub fn read(dir: &Path) -> Result<Self> {
        debug!("[config] Read scripts config from {}", dir.display());
        let ents = match fs::read_dir(dir) {
            Ok(d) => {
                debug!("[config] Scripts dir found");
                d
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("[config] Scripts dir not found, returns empty");
                return Ok(Self::default());
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("failed to read scripts dir {}", dir.display()));
            }
        };

        let mut scripts = HashMap::new();
        for ent in ents {
            let ent = ent.context("read script dir entry")?;
            let file_name = ent.file_name();
            let file_name = match file_name.to_str() {
                Some(s) => s,
                None => continue,
            };
            if !file_name.ends_with(".sh") {
                continue;
            }
            let name = file_name.trim_end_matches(".sh").to_string();
            let path = PathBuf::from(dir).join(file_name);
            let path = format!("{}", path.display());
            debug!("[config] Found script: {name}: {path}");
            scripts.insert(name, path);
        }

        Ok(Self { scripts })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.scripts.contains_key(name)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn expect_scripts() -> ScriptsConfig {
        let dir = "src/config/tests/scripts";
        let mut expected = HashMap::new();
        expected.insert("cargo-init".to_string(), format!("{dir}/cargo-init.sh"));
        expected.insert("gomod-init".to_string(), format!("{dir}/gomod-init.sh"));
        ScriptsConfig { scripts: expected }
    }

    #[test]
    fn test_scripts_config() {
        let dir = "src/config/tests/scripts";
        let scripts = ScriptsConfig::read(Path::new(dir)).unwrap();
        assert_eq!(scripts, expect_scripts());
    }

    #[test]
    fn test_default() {
        let dir = "src/config"; // no .sh files here
        let scripts = ScriptsConfig::read(Path::new(dir)).unwrap();
        assert_eq!(scripts, ScriptsConfig::default());
    }
}
