use std::path::PathBuf;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;

use crate::config::Config;
use crate::repo::database::{Bucket, Database};
use crate::{term, utils};

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,

    pub create_time: u64,

    pub version: u32,

    pub bucket: Bucket,

    #[serde(skip)]
    pub path: PathBuf,
}

impl Snapshot {
    pub fn take(cfg: &Config, db: Database, name: String) -> Snapshot {
        let path = cfg
            .get_meta_dir()
            .join("snapshot")
            .join(format!("{}.json", name));
        Snapshot {
            name,
            create_time: cfg.now(),
            version: Bucket::VERSION,
            bucket: db.close(),
            path,
        }
    }

    pub fn load(cfg: &Config, name: String) -> Result<Snapshot> {
        let path = cfg
            .get_meta_dir()
            .join("snapshot")
            .join(format!("{}.json", name));
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                bail!("could not find snapshot '{}'", name)
            }
            Err(err) => {
                return Err(err).with_context(|| format!("read snapshot path '{}'", path.display()))
            }
        };

        serde_json::from_slice(&data).context("decode snapshot data")
    }

    pub fn list(cfg: &Config) -> Result<Vec<String>> {
        let dir = cfg.get_meta_dir().join("snapshot");
        match fs::read_dir(&dir) {
            Ok(dir_read) => {
                let mut snapshots = Vec::new();
                for entry in dir_read {
                    let entry =
                        entry.with_context(|| format!("Read sub dir for '{}'", dir.display()))?;
                    let name = entry.file_name();
                    let name = match name.to_str() {
                        Some(s) => s,
                        None => continue,
                    };
                    let meta = entry.metadata().with_context(|| {
                        format!("Read metadata for '{}'", dir.join(name).display())
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
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(vec![]),
            Err(err) => Err(err).with_context(|| format!("Read snapshot dir '{}'", dir.display())),
        }
    }

    pub fn save(&self, pretty: bool) -> Result<()> {
        let formatter = PrettyFormatter::with_indent(b"  ");
        let data = if pretty {
            let mut buf = Vec::new();
            let mut ser = Serializer::with_formatter(&mut buf, formatter);
            self.serialize(&mut ser).context("serialize snapshot")?;
            buf
        } else {
            serde_json::to_vec(self).context("serialize snapshot")?
        };
        utils::write_file(&self.path, &data)
    }

    pub fn restore(self, mut db: Database) -> Result<()> {
        db.set_bucket(self.bucket);
        db.save()
    }

    pub fn display(&self, json: bool) -> Result<()> {
        if json {
            return term::show_json(self);
        }
        let remote_count = self.bucket.data.len();
        let mut owner_count: usize = 0;
        let mut repo_count: usize = 0;
        for (_, remote_bucket) in self.bucket.data.iter() {
            owner_count += remote_bucket.len();
            for (_, owner_bucket) in remote_bucket.iter() {
                repo_count += owner_bucket.len();
            }
        }

        println!("Snapshot '{}'", self.name);
        println!("Create Time:  {}", utils::format_time(self.create_time)?);
        println!("Version:      {}", self.version);
        println!("Remote Count: {}", remote_count);
        println!("Owner Count:  {}", owner_count);
        println!("Repo Count:   {}", repo_count);

        Ok(())
    }
}
