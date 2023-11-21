use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::api::types::{ApiRepo, MergeOptions, Provider};
use crate::config;
use crate::utils::{self, Lock};

pub struct Cache {
    dir: PathBuf,

    expire: Duration,

    upstream: Box<dyn Provider>,

    force: bool,

    _lock: Lock,
}

impl Provider for Cache {
    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let path = self.get_repo_path(owner, name);
        if !self.force {
            if let Some(repo) = self.read(&path)? {
                return Ok(repo);
            }
        }
        let repo = self.upstream.get_repo(owner, name)?;
        self.write(&repo, &path)?;
        Ok(repo)
    }

    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = self.list_repos_path(owner);
        if !self.force {
            if let Some(repos) = self.read(&path)? {
                return Ok(repos);
            }
        }
        let repos = self.upstream.list_repos(owner)?;
        self.write(&repos, &path)?;
        Ok(repos)
    }

    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
        self.upstream.get_merge(merge)
    }

    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String> {
        self.upstream.create_merge(merge, title, body)
    }

    fn search_repos(&self, query: &str) -> Result<Vec<String>> {
        let path = self.search_repo_path(query);
        if !self.force {
            if let Some(repos) = self.read(&path)? {
                return Ok(repos);
            }
        }
        let repos = self.upstream.search_repos(query)?;
        self.write(&repos, &path)?;
        Ok(repos)
    }
}

impl Cache {
    pub fn new(
        dir: PathBuf,
        hours: u64,
        p: Box<dyn Provider>,
        force: bool,
    ) -> Result<Box<dyn Provider>> {
        let lock = Lock::acquire("cache")?;
        let expire = Duration::from_secs(hours * 3600);
        Ok(Box::new(Cache {
            dir,
            expire,
            upstream: p,
            force,
            _lock: lock,
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

    fn search_repo_path(&self, query: &str) -> PathBuf {
        let query = query.replace("/", ".");
        self.dir.join(format!("search.{query}"))
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
        if config::now() >= &expire_duration {
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
        let now = config::now_secs();
        let buffer_size =
            bincode::serialized_size(&now).unwrap() + bincode::serialized_size(value).unwrap();
        let mut buffer = Vec::with_capacity(buffer_size as usize);

        bincode::serialize_into(&mut buffer, &now).context("Encode now")?;
        bincode::serialize_into(&mut buffer, value).context("Encode cache data")?;

        utils::write_file(path, &buffer)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::api::types::tests::StaticProvider;

    use super::*;

    const CACHE_PATH: &str = "/tmp/test-roxide-cache";

    #[test]
    #[serial]
    fn test_cache_normal() {
        let upstream = StaticProvider::mock();
        let expect_repos = upstream.list_repos("fioncat").unwrap();

        let mut cache = Cache {
            dir: PathBuf::from(CACHE_PATH),
            expire: Duration::from_secs(20),
            force: true,
            upstream,
            _lock: Lock::acquire("test-cache-01").unwrap(),
        };
        assert_eq!(cache.list_repos("fioncat").unwrap(), expect_repos);

        let upstream = StaticProvider::new(vec![("fioncat", vec!["hello0", "hello1", "hello2"])]);
        cache.upstream = upstream;
        cache.force = false;

        assert_eq!(cache.list_repos("fioncat").unwrap(), expect_repos);
    }

    #[test]
    #[serial]
    fn test_cache_expire() {
        let upstream = StaticProvider::mock();
        let expect_repos = upstream.list_repos("kubernetes").unwrap();

        let mut cache = Cache {
            dir: PathBuf::from(CACHE_PATH),
            expire: Duration::from_secs(1),
            force: true,
            upstream,
            _lock: Lock::acquire("test-cache-02").unwrap(),
        };
        assert_eq!(cache.list_repos("kubernetes").unwrap(), expect_repos);

        let fake_now = utils::current_time().unwrap() + Duration::from_secs(200);
        config::set_now(fake_now);

        let expect_repos = vec!["hello0", "hello1", "hello2"];
        let upstream = StaticProvider::new(vec![("kubernetes", expect_repos.clone())]);
        cache.upstream = upstream;
        cache.force = false;

        assert_eq!(cache.list_repos("kubernetes").unwrap(), expect_repos);
    }
}
