use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};

use crate::api::{self, RemoteAPI};
use crate::db::Database;
use crate::debug;
use crate::filelock::FileLock;

use super::Config;

pub struct ConfigContext {
    pub cfg: Config,

    pub current_dir: PathBuf,

    db: OnceLock<Result<Arc<Database>>>,

    apis: Mutex<HashMap<String, Arc<dyn RemoteAPI>>>,

    file_lock: OnceLock<Result<FileLock>>,
}

impl ConfigContext {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let current_dir = env::current_dir().context("failed to get current directory")?;
        Ok(Arc::new(Self {
            cfg,
            current_dir,
            db: OnceLock::new(),
            apis: Mutex::new(HashMap::new()),
            file_lock: OnceLock::new(),
        }))
    }

    #[cfg(test)]
    pub fn new_mock(
        cfg: Config,
        api: Arc<dyn RemoteAPI>,
        current_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let current_dir = current_dir.unwrap_or(env::current_dir().unwrap());
        let mut apis = HashMap::new();
        apis.insert("github".to_string(), api);
        Arc::new(Self {
            cfg,
            current_dir,
            db: OnceLock::new(),
            apis: Mutex::new(apis),
            file_lock: OnceLock::new(),
        })
    }

    pub fn get_db(&self) -> Result<Arc<Database>> {
        debug!("[context] Get database instance");
        let result = self.db.get_or_init(|| {
            let path = PathBuf::from(&self.cfg.data_dir).join("sqlite.db");
            debug!(
                "[context] Init new database instance, path: {}",
                path.display()
            );
            let db = Database::open(&path)
                .with_context(|| format!("failed to open database {}", path.display()))?;
            Ok(Arc::new(db))
        });
        match result {
            Ok(db) => Ok(db.clone()),
            Err(e) => bail!(e.to_string()),
        }
    }

    pub fn get_api(&self, remote_name: &str, force_no_cache: bool) -> Result<Arc<dyn RemoteAPI>> {
        debug!("[context] Get api for remote {remote_name:?}, force_no_cache: {force_no_cache}");
        let mut apis = match self.apis.lock() {
            Ok(apis) => apis,
            Err(e) => bail!("failed to lock apis: {e:#}"),
        };

        if let Some(api) = apis.get(remote_name) {
            debug!("[context] Found api in cache");
            return Ok(api.clone());
        }

        debug!("[context] Init new api, and save it to cache");
        let remote_cfg = self.cfg.get_remote(remote_name)?;
        let api = api::new(remote_cfg, self.get_db()?, force_no_cache)?;
        apis.insert(remote_name.to_string(), api.clone());
        Ok(api)
    }

    pub fn lock(&self) -> Result<()> {
        if let Err(e) = self.file_lock.get_or_init(|| {
            let path = PathBuf::from(&self.cfg.data_dir).join("lock");
            debug!(
                "[context] Acquire global file lock, path: {}",
                path.display()
            );
            FileLock::acquire(path)
        }) {
            bail!("failed to acquire global file lock: {e:#}");
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use std::borrow::Cow;
    use std::fs;

    use crate::api::RemoteInfo;
    use crate::db::remote_repo::{RemoteRepository, RemoteUpstream};
    use crate::db::repo::Repository;
    use crate::{config, db, repo};

    use super::*;

    struct MockAPI {}

    #[async_trait::async_trait]
    impl RemoteAPI for MockAPI {
        async fn info(&self) -> Result<RemoteInfo> {
            Ok(RemoteInfo {
                name: Cow::Borrowed("mock"),
                auth_user: None,
                ping: false,
                cache: false,
            })
        }

        async fn list_repos(&self, _remote: &str, _owner: &str) -> Result<Vec<String>> {
            Ok(vec![
                "roxide".to_string(),
                "otree".to_string(),
                "csync".to_string(),
                "dotfiles".to_string(),
                "filewarden".to_string(),
                "nvimdots".to_string(),
            ])
        }

        async fn get_repo(
            &self,
            remote: &str,
            owner: &str,
            name: &str,
        ) -> Result<RemoteRepository<'static>> {
            let mut repo = RemoteRepository {
                remote: Cow::Owned(remote.to_string()),
                owner: Cow::Owned(owner.to_string()),
                name: Cow::Owned(name.to_string()),
                default_branch: "main".to_string(),
                web_url: format!("https://github.com/{owner}/{name}"),
                expire_at: 0,
                ..Default::default()
            };

            if name == "nvimdots" {
                repo.upstream = Some(RemoteUpstream {
                    owner: "ayamir".to_string(),
                    name: "nvimdots".to_string(),
                    default_branch: "main".to_string(),
                });
            }

            Ok(repo)
        }
    }

    pub fn build_test_context(name: &str, current_dir: Option<PathBuf>) -> Arc<ConfigContext> {
        let base_dir = format!("tests/{name}");
        let _ = fs::remove_dir_all(&base_dir);
        repo::ensure_dir(&base_dir).unwrap();

        let workspace = format!("{base_dir}/workspace");
        let data_dir = format!("{base_dir}/data");

        repo::ensure_dir(&workspace).unwrap();
        repo::ensure_dir(&data_dir).unwrap();

        let workspace = fs::canonicalize(&workspace).unwrap();
        let data_dir = fs::canonicalize(&data_dir).unwrap();

        let hooks = config::hook::tests::expect_hooks();

        let mut remotes = config::remote::tests::expect_remotes();
        for remote in remotes.iter_mut() {
            remote.validate(&hooks).unwrap();
        }

        let mut cfg = Config {
            workspace: format!("{}", workspace.display()),
            data_dir: format!("{}", data_dir.display()),
            remotes,
            hooks,
            ..Default::default()
        };
        let home_dir = dirs::home_dir().unwrap();
        cfg.validate(&home_dir).unwrap();

        let ctx = ConfigContext::new_mock(cfg, Arc::new(MockAPI {}), current_dir);

        let repos = db::tests::test_repos();

        let db = ctx.get_db().unwrap();
        db.with_transaction(|tx| {
            for repo in repos {
                tx.repo().insert(&repo).unwrap();
            }
            Ok(())
        })
        .unwrap();

        ctx
    }

    #[tokio::test]
    async fn test_context() {
        let ctx = build_test_context("context", None);
        let repo = ctx
            .get_db()
            .unwrap()
            .with_transaction(|tx| tx.repo().get("github", "fioncat", "roxide"))
            .unwrap()
            .unwrap();
        assert_eq!(
            repo,
            Repository {
                remote: "github".to_string(),
                owner: "fioncat".to_string(),
                name: "roxide".to_string(),
                path: None,
                sync: true,
                pin: true,
                last_visited_at: 2234,
                visited_count: 20,
                new_created: false,
            }
        );

        let api = ctx.get_api("github", false).unwrap();
        let repos = api.list_repos("github", "fioncat").await.unwrap();
        assert_eq!(
            repos,
            vec![
                "roxide",
                "otree",
                "csync",
                "dotfiles",
                "filewarden",
                "nvimdots"
            ]
        );
        let repo = api.get_repo("github", "fioncat", "roxide").await.unwrap();
        assert_eq!(
            repo,
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("fioncat".to_string()),
                name: Cow::Owned("roxide".to_string()),
                default_branch: "main".to_string(),
                web_url: "https://github.com/fioncat/roxide".to_string(),
                upstream: None,
                expire_at: 0,
            }
        );
    }
}
