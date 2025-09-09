use std::borrow::Cow;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::db::Database;
use crate::db::remote_owner::RemoteOwner;
use crate::db::remote_repo::RemoteRepository;
use crate::debug;

use super::{Action, ListPullRequestsOptions, PullRequest, RemoteAPI, RemoteInfo};

pub struct Cache {
    pub upstream: Arc<dyn RemoteAPI>,
    pub db: Arc<Database>,
    pub expire: u64,
    pub force: bool,
    pub now: u64,
}

#[async_trait]
impl RemoteAPI for Cache {
    async fn info(&self) -> Result<RemoteInfo> {
        let mut info = self.upstream.info().await?;
        info.cache = true;
        Ok(info)
    }

    async fn list_repos(&self, remote: &str, owner: &str) -> Result<Vec<String>> {
        debug!("[cache] List repos for {remote}:{owner}");
        let cache = self.db.with_transaction(|tx| {
            let cache = tx.remote_owner().get_optional(remote, owner)?;
            let Some(cache) = cache else {
                debug!("[cache] No cache found");
                return Ok(None);
            };
            if !self.force && cache.expire_at > self.now {
                debug!("[cache] Hit cache");
                return Ok(Some(cache));
            }

            debug!("[cache] Owner cache expired or force mode enabled, remove it");
            tx.remote_owner().delete(&cache)?;
            Ok(None)
        })?;

        if let Some(cache) = cache {
            debug!("[cache] Cache result: {cache:?}");
            return Ok(cache.repos);
        }

        let repos = self.upstream.list_repos(remote, owner).await?;
        let cache = RemoteOwner {
            remote: Cow::Borrowed(remote),
            owner: Cow::Borrowed(owner),
            repos,
            expire_at: self.now + self.expire,
        };

        debug!("[cache] Save owner cache: {cache:?}");
        self.db
            .with_transaction(|tx| tx.remote_owner().insert(&cache))?;
        Ok(cache.repos)
    }

    async fn get_repo(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<RemoteRepository<'static>> {
        debug!("[cache] Get repo for {remote}:{owner}:{name}");
        let cache = self.db.with_transaction(|tx| {
            let cache = tx.remote_repo().get_optional(remote, owner, name)?;
            let Some(cache) = cache else {
                debug!("[cache] No cache found");
                return Ok(None);
            };
            if !self.force && cache.expire_at > self.now {
                debug!("[cache] Hit cache");
                return Ok(Some(cache));
            }
            debug!("[cache] Repo cache expired or force mode enabled, remove it");
            tx.remote_repo().delete(&cache)?;
            Ok(None)
        })?;

        if let Some(cache) = cache {
            debug!("[cache] Cache result: {cache:?}");
            return Ok(cache);
        }

        let mut repo = self.upstream.get_repo(remote, owner, name).await?;
        repo.expire_at = self.now + self.expire;
        debug!("[cache] Save repo cache: {repo:?}");
        self.db
            .with_transaction(|tx| tx.remote_repo().insert(&repo))?;
        Ok(repo)
    }

    async fn create_pull_request(
        &self,
        owner: &str,
        name: &str,
        pr: &PullRequest,
    ) -> Result<String> {
        self.upstream.create_pull_request(owner, name, pr).await
    }

    async fn list_pull_requests(&self, opts: ListPullRequestsOptions) -> Result<Vec<PullRequest>> {
        self.upstream.list_pull_requests(opts).await
    }

    async fn get_action(&self, owner: &str, name: &str, commit: &str) -> Result<Action> {
        self.upstream.get_action(owner, name, commit).await
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;

    use tokio::sync::Mutex;

    use super::*;

    struct MockUpstream {
        repos: Mutex<RefCell<Option<Vec<String>>>>,
        repo: Mutex<RefCell<Option<RemoteRepository<'static>>>>,
        update_repos: Vec<String>,
        update_repo: RemoteRepository<'static>,
        raw_repos: Vec<String>,
        raw_repo: RemoteRepository<'static>,
    }

    impl MockUpstream {
        fn new(
            repos: Vec<String>,
            repo: RemoteRepository<'static>,
            update_repos: Vec<String>,
            update_repo: RemoteRepository<'static>,
        ) -> Self {
            let raw_repos = repos.clone();
            let raw_repo = repo.clone();
            Self {
                repos: Mutex::new(RefCell::new(Some(repos))),
                repo: Mutex::new(RefCell::new(Some(repo))),
                update_repos,
                update_repo,
                raw_repos,
                raw_repo,
            }
        }

        async fn recover(&self) {
            self.repos
                .lock()
                .await
                .replace(Some(self.raw_repos.clone()));
            self.repo.lock().await.replace(Some(self.raw_repo.clone()));
        }
    }

    #[async_trait]
    impl RemoteAPI for MockUpstream {
        async fn info(&self) -> Result<RemoteInfo> {
            Ok(RemoteInfo {
                name: Cow::Borrowed("MockUpstream"),
                auth_user: None,
                ping: true,
                cache: false,
            })
        }

        async fn list_repos(&self, _remote: &str, _owner: &str) -> Result<Vec<String>> {
            let repos = self
                .repos
                .lock()
                .await
                .take()
                .unwrap_or(self.update_repos.clone());
            Ok(repos)
        }

        async fn get_repo(
            &self,
            _remote: &str,
            _owner: &str,
            _name: &str,
        ) -> Result<RemoteRepository<'static>> {
            let repo = self
                .repo
                .lock()
                .await
                .take()
                .unwrap_or(self.update_repo.clone());
            Ok(repo)
        }

        async fn create_pull_request(
            &self,
            _owner: &str,
            _name: &str,
            _pr: &PullRequest,
        ) -> Result<String> {
            Ok("https://example.com/pull/1".to_string())
        }

        async fn list_pull_requests(
            &self,
            _opts: ListPullRequestsOptions,
        ) -> Result<Vec<PullRequest>> {
            Ok(vec![])
        }

        async fn get_action(&self, _owner: &str, _name: &str, _commit: &str) -> Result<Action> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_cache() {
        let repos = vec![
            "repo1".to_string(),
            "repo2".to_string(),
            "repo3".to_string(),
        ];
        let update_repos = vec!["repo4".to_string()];
        let mut repo = RemoteRepository {
            remote: Cow::Owned("github".to_string()),
            owner: Cow::Owned("alice".to_string()),
            name: Cow::Owned("repo1".to_string()),
            default_branch: "main".to_string(),
            web_url: "https://github.com/alice/repo1".to_string(),
            upstream: None,
            expire_at: 0,
        };
        let mut update_repo = RemoteRepository {
            remote: Cow::Owned("github".to_string()),
            owner: Cow::Owned("alice".to_string()),
            name: Cow::Owned("repo1".to_string()),
            default_branch: "main".to_string(),
            web_url: "https://github.com/alice/repo1".to_string(),
            upstream: None,
            expire_at: 0,
        };

        let mock = Arc::new(MockUpstream::new(
            repos.clone(),
            repo.clone(),
            update_repos.clone(),
            update_repo.clone(),
        ));
        let _ = fs::remove_file("tests/cache.db");
        let db = Arc::new(Database::open("tests/cache.db").unwrap());
        let mut cache = Cache {
            upstream: mock.clone(),
            db: db.clone(),
            expire: 50,
            force: false,
            now: 100,
        };

        let info = cache.info().await.unwrap();
        assert!(info.cache);

        // Get default data, first from mock, then cache
        let results = cache.list_repos("github", "alice").await.unwrap();
        assert_eq!(results, repos);

        let result = cache.get_repo("github", "alice", "repo1").await.unwrap();
        assert_eq!(result.expire_at, 150);
        repo.expire_at = 150;
        assert_eq!(result, repo);

        let results = mock.list_repos("github", "alice").await.unwrap();
        assert_eq!(results, update_repos);

        let result = mock.get_repo("github", "alice", "repo1").await.unwrap();
        assert_eq!(result, update_repo);

        // Make cache expire, we will get updated data
        cache.now = 200;
        let results = cache.list_repos("github", "alice").await.unwrap();
        assert_eq!(results, update_repos);

        let result = cache.get_repo("github", "alice", "repo1").await.unwrap();
        assert_eq!(result.expire_at, 250);
        update_repo.expire_at = 250;
        assert_eq!(result, update_repo);

        mock.recover().await;

        // The mock data is recovered, but we still get cache
        let results = cache.list_repos("github", "alice").await.unwrap();
        assert_eq!(results, update_repos);

        let result = cache.get_repo("github", "alice", "repo1").await.unwrap();
        assert_eq!(result, update_repo);

        // Use force to bypass cache
        cache.force = true;

        // Now we get recovered data
        let results = cache.list_repos("github", "alice").await.unwrap();
        assert_eq!(results, repos);

        let result = cache.get_repo("github", "alice", "repo1").await.unwrap();
        repo.expire_at = 250;
        assert_eq!(result, repo);
    }
}
