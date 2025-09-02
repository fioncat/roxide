use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};

use crate::api::{self, RemoteAPI};
use crate::db::Database;

use super::Config;

pub struct ConfigContext {
    pub cfg: Config,

    db: OnceLock<Result<Arc<Database>>>,

    apis: Mutex<RefCell<HashMap<String, Arc<dyn RemoteAPI>>>>,
}

impl ConfigContext {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            db: OnceLock::new(),
            apis: Mutex::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn get_db(&self) -> Result<Arc<Database>> {
        let result = self.db.get_or_init(|| {
            let path = PathBuf::from(&self.cfg.data_dir).join("sqlite.db");
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
        let apis = match self.apis.lock() {
            Ok(apis) => apis,
            Err(e) => bail!("failed to lock apis: {:#}", e),
        };

        if let Some(api) = apis.borrow().get(remote_name) {
            return Ok(api.clone());
        }

        let remote_cfg = self.cfg.get_remote(remote_name)?;
        let api = api::new(remote_cfg, self.get_db()?, force_no_cache)?;
        apis.borrow_mut()
            .insert(remote_name.to_string(), api.clone());
        Ok(api)
    }
}
