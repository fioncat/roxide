use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};

use crate::api::{self, RemoteAPI};
use crate::db::Database;
use crate::debug;

use super::Config;

pub struct ConfigContext {
    pub cfg: Config,

    pub current_dir: PathBuf,

    db: OnceLock<Result<Arc<Database>>>,

    apis: Mutex<RefCell<HashMap<String, Arc<dyn RemoteAPI>>>>,
}

impl ConfigContext {
    pub fn new(cfg: Config) -> Result<Arc<Self>> {
        let current_dir = env::current_dir().context("failed to get current directory")?;
        Ok(Arc::new(Self {
            cfg,
            current_dir,
            db: OnceLock::new(),
            apis: Mutex::new(RefCell::new(HashMap::new())),
        }))
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
        let apis = match self.apis.lock() {
            Ok(apis) => apis,
            Err(e) => bail!("failed to lock apis: {e:#}"),
        };

        if let Some(api) = apis.borrow().get(remote_name) {
            debug!("[context] Found api in cache");
            return Ok(api.clone());
        }

        debug!("[context] Init new api, and save it to cache");
        let remote_cfg = self.cfg.get_remote(remote_name)?;
        let api = api::new(remote_cfg, self.get_db()?, force_no_cache)?;
        apis.borrow_mut()
            .insert(remote_name.to_string(), api.clone());
        Ok(api)
    }
}
