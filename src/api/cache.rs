use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::api::types::{ApiRepo, Merge, Provider};
use crate::utils;

pub struct Cache {
    dir: PathBuf,

    now: Duration,
    expire: Duration,

    upstream: Box<dyn Provider>,
}

impl Provider for Cache {
    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let path = self.get_repo_path(owner, name);
        if let Some(repo) = self.read(&path)? {
            return Ok(repo);
        }
        let repo = self.upstream.get_repo(owner, name)?;
        self.write(&repo, &path)?;
        Ok(repo)
    }

    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = self.list_repos_path(owner);
        if let Some(repos) = self.read(&path)? {
            return Ok(repos);
        }
        let repos = self.upstream.list_repos(owner)?;
        self.write(&repos, &path)?;
        Ok(repos)
    }

    fn get_merge(&self, merge: Merge) -> Result<Option<String>> {
        self.upstream.get_merge(merge)
    }

    fn create_merge(&self, merge: Merge) -> Result<String> {
        self.upstream.create_merge(merge)
    }
}

impl Cache {
    pub fn new(dir: PathBuf, hours: u64, p: Box<dyn Provider>) -> Result<Box<dyn Provider>> {
        let now = utils::current_time()?;
        let expire = Duration::from_secs(hours * 3600);

        Ok(Box::new(Cache {
            dir,
            expire,
            now,
            upstream: p,
        }))
    }

    fn list_repos_path(&self, owner: &str) -> PathBuf {
        let owner = owner.replace("/", ".");
        self.dir.join(format!("list.{owner}"))
    }

    fn get_repo_path(&self, owner: &str, name: &str) -> PathBuf {
        let owner = owner.replace("/", ".");
        let name = name.replace("/", ".");
        self.dir.join(format!("repo.{owner}.{name}"))
    }

    fn read<T>(&self, path: &PathBuf) -> Result<Option<T>>
    where
        T: DeserializeOwned + ?Sized,
    {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(err).with_context(|| format!("Read cache file {}", path.display()))
            }
        };

        let decoder = &mut bincode::options().with_fixint_encoding();
        let mut update_time: u64 = 0;
        let update_time_size = decoder.serialized_size(&update_time).unwrap() as usize;
        if data.len() < update_time_size {
            bail!("Corrupted cache data in {}", path.display());
        }

        let (update_time_data, cache_data) = data.split_at(update_time_size);
        update_time = decoder
            .deserialize(update_time_data)
            .context("Decode cache last_updated data")?;
        let expire_duration = Duration::from_secs(update_time) + self.expire;
        if self.now >= expire_duration {
            fs::remove_file(path)
                .with_context(|| format!("Remove cache file {}", path.display()))?;
            return Ok(None);
        }

        let cache = decoder
            .deserialize::<T>(cache_data)
            .context("Decode cache data")?;
        Ok(Some(cache))
    }

    fn write<T>(&self, value: &T, path: &PathBuf) -> Result<()>
    where
        T: Serialize + ?Sized,
    {
        let now = self.now.as_secs();
        let buffer_size =
            bincode::serialized_size(&now).unwrap() + bincode::serialized_size(value).unwrap();
        let mut buffer = Vec::with_capacity(buffer_size as usize);

        bincode::serialize_into(&mut buffer, &now).context("Encode now")?;
        bincode::serialize_into(&mut buffer, value).context("Encode cache data")?;

        utils::write_file(path, &buffer)
    }
}
