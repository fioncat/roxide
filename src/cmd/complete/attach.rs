use anyhow::Result;

use crate::cmd::complete::Complete;
use crate::{config, repo::database::Database};

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let remotes = config::get().list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(Complete::from(items))
        }
        2 => {
            let remote_name = &args[0];
            let db = Database::read()?;

            let owners = db.list_owners(remote_name);
            let items: Vec<_> = owners
                .into_iter()
                .map(|owner| format!("{}/", owner))
                .collect();
            return Ok(Complete::from(items).no_space());
        }
        _ => Ok(Complete::empty()),
    }
}
