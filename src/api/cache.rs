use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bincode::Options;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::api::*;
use crate::config::{Config, RemoteConfig};
use crate::filelock::FileLock;
use crate::utils;

pub struct Cache {
    dir: PathBuf,

    expire: Duration,

    upstream: Box<dyn Provider>,

    force: bool,

    _lock: FileLock,

    now: u64,
}

impl Provider for Cache {
    fn info(&self) -> Result<ProviderInfo> {
        self.upstream.info()
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

    fn get_action(&self, opts: &ActionOptions) -> Result<Option<Action>> {
        self.upstream.get_action(opts)
    }

    fn logs_job(&self, owner: &str, name: &str, id: u64, dst: &mut dyn Write) -> Result<()> {
        self.upstream.logs_job(owner, name, id, dst)
    }

    fn get_job(&self, owner: &str, name: &str, id: u64) -> Result<ActionJob> {
        self.upstream.get_job(owner, name, id)
    }
}

impl Cache {
    pub fn new(
        cfg: &Config,
        remote_cfg: &RemoteConfig,
        upstream: Box<dyn Provider>,
        force: bool,
    ) -> Result<Cache> {
        let lock = FileLock::acquire(cfg, "cache")?;
        let expire = Duration::from_secs(remote_cfg.cache_hours as u64 * 3600);
        let dir = cfg.get_meta_dir().join("cache").join(remote_cfg.get_name());
        Ok(Cache {
            dir,
            expire,
            upstream,
            force,
            _lock: lock,
            now: cfg.now(),
        })
    }

    fn list_repos_path(&self, owner: &str) -> PathBuf {
        let owner = owner.replace('/', ".");
        self.dir.join(format!("list.{owner}"))
    }

    fn get_repo_path(&self, owner: &str, name: &str) -> PathBuf {
        let owner = owner.replace('/', ".");
        let name = name.replace('/', ".");
        self.dir.join(format!("repo.{owner}.{name}"))
    }

    fn search_repo_path(&self, query: &str) -> PathBuf {
        let query = query.replace('/', ".");
        self.dir.join(format!("search.{query}"))
    }

    fn read<T>(&self, path: &PathBuf) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(err).with_context(|| format!("read cache file {}", path.display()))
            }
        };

        let decoder = &mut bincode::options().with_fixint_encoding();
        let mut update_time: u64 = 0;
        let update_time_size = decoder.serialized_size(&update_time).unwrap() as usize;
        if data.len() < update_time_size {
            bail!("corrupted cache data in {}", path.display());
        }

        let (update_time_data, cache_data) = data.split_at(update_time_size);
        update_time = decoder
            .deserialize(update_time_data)
            .context("decode cache last_updated data")?;
        let expire_duration = Duration::from_secs(update_time) + self.expire;
        if self.now >= expire_duration.as_secs() {
            fs::remove_file(path)
                .with_context(|| format!("remove cache file {}", path.display()))?;
            return Ok(None);
        }

        let cache = decoder
            .deserialize::<T>(cache_data)
            .context("decode cache data")?;
        Ok(Some(cache))
    }

    fn write<T>(&self, value: &T, path: &PathBuf) -> Result<()>
    where
        T: Serialize + ?Sized,
    {
        let now = self.now;
        let buffer_size =
            bincode::serialized_size(&now).unwrap() + bincode::serialized_size(value).unwrap();
        let mut buffer = Vec::with_capacity(buffer_size as usize);

        bincode::serialize_into(&mut buffer, &now).context("encode now")?;
        bincode::serialize_into(&mut buffer, value).context("encode cache data")?;

        utils::write_file(path, &buffer)
    }
}

#[cfg(test)]
mod cache_tests {
    use crate::api::api_tests::StaticProvider;
    use crate::api::cache::*;
    use crate::config::config_tests;

    #[test]
    fn test_cache_normal() {
        let cfg = config_tests::load_test_config("api_cache/normal");
        let upstream = StaticProvider::mock();
        let expect_repos = upstream.list_repos("fioncat").unwrap();
        let remote_cfg = cfg.get_remote("github").unwrap();

        let mut cache = Cache::new(&cfg, &remote_cfg, upstream, true).unwrap();

        assert_eq!(cache.list_repos("fioncat").unwrap(), expect_repos);

        let upstream = StaticProvider::build(vec![("fioncat", vec!["hello0", "hello1", "hello2"])]);
        cache.upstream = upstream;
        cache.force = false;

        assert_eq!(cache.list_repos("fioncat").unwrap(), expect_repos);
    }

    #[test]
    fn test_cache_expire() {
        let cfg = config_tests::load_test_config("api_cache/expire");
        let upstream = StaticProvider::mock();
        let expect_repos = upstream.list_repos("kubernetes").unwrap();
        let remote_cfg = cfg.get_remote("github").unwrap();

        let mut cache = Cache::new(&cfg, &remote_cfg, upstream, true).unwrap();
        // Set a small expire time to simulate expiration.
        cache.expire = Duration::from_secs(100);
        assert_eq!(cache.list_repos("kubernetes").unwrap(), expect_repos);

        let fake_now = cfg.now() + Duration::from_secs(200).as_secs();
        cache.now = fake_now;

        let expect_repos = vec!["hello0", "hello1", "hello2"];
        let upstream = StaticProvider::build(vec![("kubernetes", expect_repos.clone())]);
        cache.upstream = upstream;
        cache.force = false;

        assert_eq!(cache.list_repos("kubernetes").unwrap(), expect_repos);
    }
}
