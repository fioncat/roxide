use std::sync::Arc;

use anyhow::Result;

use crate::config::context::ConfigContext;
use crate::db::repo::Repository;

pub fn get_current_repo(ctx: Arc<ConfigContext>) -> Result<Option<Repository>> {
    todo!()
}
