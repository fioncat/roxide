use anyhow::Result;
use roxide::{config, repo::database::Database};

use super::Complete;

pub fn complete(args: &[String]) -> Result<Complete> {
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
