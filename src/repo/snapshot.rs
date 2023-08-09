use std::{fs, io::ErrorKind, path::PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::{config, utils};

pub struct Snapshot {
    pub name: String,
    pub items: Vec<Item>,

    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    pub remote: String,
    pub owner: String,
    pub name: String,

    pub branch: String,

    pub path: Option<String>,

    pub last_accessed: u64,
    pub acceseed: f64,
}

impl Snapshot {
    pub fn new(name: String, items: Vec<Item>) -> Snapshot {
        let path = PathBuf::from(&config::base().metadir)
            .join("snapshot")
            .join(format!("{}.json", name));
        Snapshot { name, items, path }
    }

    pub fn read(name: String) -> Result<Snapshot> {
        let path = PathBuf::from(&config::base().metadir)
            .join("snapshot")
            .join(format!("{}.json", name));
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                bail!("Could not find snapshot {}", name)
            }
            Err(err) => {
                return Err(err).with_context(|| format!("Read snapshot path {}", path.display()))
            }
        };

        let items: Vec<Item> = serde_json::from_slice(&data).context("Decode snapshot data")?;

        Ok(Snapshot { name, items, path })
    }

    pub fn list() -> Result<Vec<String>> {
        let dir = PathBuf::from(&config::base().metadir).join("snapshot");
        match fs::read_dir(&dir) {
            Ok(dir_read) => {
                let mut snapshots = Vec::new();
                for entry in dir_read {
                    let entry =
                        entry.with_context(|| format!("Read subdir for {}", dir.display()))?;
                    let name = entry.file_name();
                    let name = match name.to_str() {
                        Some(s) => s,
                        None => continue,
                    };
                    let meta = entry.metadata().with_context(|| {
                        format!("Read metadata for {}", dir.join(name).display())
                    })?;
                    if !meta.is_file() {
                        continue;
                    }
                    if name.ends_with(".json") {
                        let name = name.strip_suffix(".json").unwrap();
                        snapshots.push(name.to_string());
                    }
                }
                Ok(snapshots)
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(vec![]),
            Err(err) => Err(err).with_context(|| format!("Read snapshot dir {}", dir.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let formatter = PrettyFormatter::with_indent(b"  ");
        let mut buf = Vec::new();
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        self.items
            .serialize(&mut ser)
            .context("Serialize snapshot")?;
        utils::write_file(&self.path, &buf)
    }
}
