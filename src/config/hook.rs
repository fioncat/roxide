use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::debug;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HooksConfig {
    hooks: HashMap<String, String>,
}

impl HooksConfig {
    pub fn read(dir: &Path) -> Result<Self> {
        debug!("[config] Read hooks config from {}", dir.display());
        let ents = match fs::read_dir(dir) {
            Ok(d) => {
                debug!("[config] Hooks dir found");
                d
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("[config] Hooks dir not found, returns empty");
                return Ok(Self::default());
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("failed to read hooks dir {}", dir.display()));
            }
        };

        let mut hooks = HashMap::new();
        for ent in ents {
            let ent = ent.context("read hooks dir entry")?;
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
            debug!("[config] Found hook: {name}: {path}");
            hooks.insert(name, path);
        }

        Ok(Self { hooks })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.hooks.contains_key(name)
    }

    pub fn get<'a>(&'a self, name: &str) -> Option<&'a String> {
        self.hooks.get(name)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn expect_hooks() -> HooksConfig {
        let dir = "src/config/tests/hooks";
        let dir = fs::canonicalize(dir).unwrap();
        let dir = format!("{}", dir.display());
        let mut expected = HashMap::new();
        expected.insert("cargo-init".to_string(), format!("{dir}/cargo-init.sh"));
        expected.insert("gomod-init".to_string(), format!("{dir}/gomod-init.sh"));
        HooksConfig { hooks: expected }
    }

    #[test]
    fn test_hooks_config() {
        let dir = "src/config/tests/hooks";
        let dir = fs::canonicalize(dir).unwrap();
        let hooks = HooksConfig::read(&dir).unwrap();
        assert_eq!(hooks, expect_hooks());
    }

    #[test]
    fn test_default() {
        let dir = "src/config"; // no .sh files here
        let hooks = HooksConfig::read(Path::new(dir)).unwrap();
        assert_eq!(hooks, HooksConfig::default());
    }
}
