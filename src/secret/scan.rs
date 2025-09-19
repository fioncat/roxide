use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};

use crate::scan::ignore::Ignore;
use crate::scan::{ScanHandler, scan_files};

pub async fn scan_secret_files(base_dir: &Path) -> Result<Vec<ScanFile>> {
    let rules = read_rules(base_dir)?;
    let handler = ScanFilesHandler {
        base_dir: base_dir.to_path_buf(),
        rules,
        files: Mutex::new(vec![]),
    };

    let handler = scan_files([base_dir], handler, true).await?;
    let mut files = handler.files.into_inner().unwrap();
    files.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFile {
    pub name: String,
    pub source_path: PathBuf,
    pub secret_path: PathBuf,
}

#[derive(Debug)]
struct ScanFilesHandler {
    base_dir: PathBuf,
    rules: Ignore,
    files: Mutex<Vec<ScanFile>>,
}

impl ScanHandler<()> for ScanFilesHandler {
    fn handle_files(&self, files: Vec<(PathBuf, Metadata)>, _: ()) -> Result<()> {
        let mut results = vec![];
        for (path, _) in files {
            let (source_path, secret_path) = if let Some(ext) = path.extension()
                && ext == "secret"
            {
                let source_path = path.with_extension("");
                if !self.rules.matched(&source_path, false) {
                    continue;
                }
                (source_path, path)
            } else {
                if !self.rules.matched(&path, false) {
                    continue;
                }
                let mut secret_path = format!("{}", path.display());
                secret_path.push_str(".secret");
                (path, PathBuf::from(secret_path))
            };
            let Ok(name) = source_path.strip_prefix(&self.base_dir) else {
                continue;
            };
            let name = format!("{}", name.display());
            results.push(ScanFile {
                name,
                source_path,
                secret_path,
            });
        }

        let mut files = self.files.lock().unwrap();
        files.extend(results);
        Ok(())
    }

    fn should_skip(&self, dir: &Path, _: ()) -> Result<bool> {
        if let Some(name) = dir.file_name()
            && name == ".git"
        {
            return Ok(true);
        }
        Ok(false)
    }
}

fn read_rules(base_dir: &Path) -> Result<Ignore> {
    let ignore_path = base_dir.join(".gitignore");
    if !ignore_path.exists() {
        bail!("\".gitignore\" file not found for this repository");
    }

    let data = fs::read_to_string(&ignore_path).context("failed to read .gitignore file")?;
    let lines = data.lines();
    let mut patterns = Vec::new();
    let mut marked = false;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "# rox:secrets" {
            marked = true;
            continue;
        }
        if line == "# rox:end" {
            marked = false;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if marked {
            patterns.push(line.to_string());
        }
    }
    if patterns.is_empty() {
        bail!("no secret patterns found in \".gitignore\" file");
    }

    Ignore::parse(base_dir, &patterns)
        .context("failed to parse secret patterns in \".gitignore\" file")
}

#[cfg(test)]
mod tests {
    use crate::repo::ensure_dir;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_scan_secret_files() {
        let base_dir = "tests/scan_secret_files";
        let _ = fs::remove_dir_all(base_dir);

        let ignore_content = r#"
        src/*.rs

        # rox:secrets
        config/*.yaml
        src/*.toml
        apps/test/secret.json
        # rox:end

        tests/secret.txt
        "#;

        let base_dir = PathBuf::from(base_dir);
        ensure_dir(&base_dir).unwrap();
        ensure_dir(base_dir.join("src")).unwrap();
        ensure_dir(base_dir.join("config")).unwrap();
        ensure_dir(base_dir.join("apps/test")).unwrap();
        ensure_dir(base_dir.join("tests")).unwrap();

        fs::write(base_dir.join(".gitignore"), ignore_content).unwrap();
        fs::write(base_dir.join("src/main.rs"), "").unwrap();
        fs::write(base_dir.join("src/lib.rs"), "").unwrap();
        fs::write(base_dir.join("src/config.toml.secret"), "").unwrap();
        fs::write(base_dir.join("src/test.toml"), "").unwrap();
        fs::write(base_dir.join("config/hello.yaml.secret"), "").unwrap();
        fs::write(base_dir.join("config/test.toml"), "").unwrap();
        fs::write(base_dir.join("apps/test/secret.json"), "").unwrap();
        fs::write(base_dir.join("tests/secret.txt"), "").unwrap();

        let files = scan_secret_files(&base_dir).await.unwrap();
        let expected = vec![
            ScanFile {
                name: "apps/test/secret.json".to_string(),
                source_path: base_dir.join("apps/test/secret.json"),
                secret_path: base_dir.join("apps/test/secret.json.secret"),
            },
            ScanFile {
                name: "config/hello.yaml".to_string(),
                source_path: base_dir.join("config/hello.yaml"),
                secret_path: base_dir.join("config/hello.yaml.secret"),
            },
            ScanFile {
                name: "src/config.toml".to_string(),
                source_path: base_dir.join("src/config.toml"),
                secret_path: base_dir.join("src/config.toml.secret"),
            },
            ScanFile {
                name: "src/test.toml".to_string(),
                source_path: base_dir.join("src/test.toml"),
                secret_path: base_dir.join("src/test.toml.secret"),
            },
        ];
        assert_eq!(files, expected);
    }
}
