use anyhow::Result;

use crate::cmd::complete::Complete;
use crate::repo::database::Database;
use crate::{config, utils};

pub fn complete(args: &[&str]) -> Result<Complete> {
    match args.len() {
        0 | 1 => {
            let remotes = config::list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(Complete::from(items))
        }
        2 => {
            let remote_name = &args[0];
            let db = Database::read()?;
            let remote = config::must_get_remote(remote_name)?;

            let owner_alias_map = utils::revert_map(&remote.owner_alias);
            let name_alias_map = utils::revert_map(&remote.repo_alias);

            let query = &args[1];
            if !query.contains("/") {
                let owners = db.list_owners(remote_name);
                let items: Vec<_> = owners
                    .into_iter()
                    // .map(|owner| format!("{}/", owner))
                    .map(|owner| {
                        if let Some(alias_owner) = owner_alias_map.get(owner.as_str()) {
                            return format!("{alias_owner}/");
                        }
                        format!("{owner}/")
                    })
                    .collect();
                return Ok(Complete::from(items).no_space());
            }

            let (owner, _) = utils::parse_query(&remote, query);
            let repos = db.list_by_remote(remote_name);
            let items: Vec<_> = repos
                .into_iter()
                .filter(|repo| repo.owner.as_str().eq(&owner))
                // .map(|repo| format!("{}", repo.long_name()))
                .map(|repo| {
                    let mut owner = format!("{}", repo.owner);
                    let mut name = format!("{}", repo.name);
                    if let Some(alias_owner) = owner_alias_map.get(&owner) {
                        owner = alias_owner.clone();
                    }
                    if let Some(alias_name) = name_alias_map.get(&name) {
                        name = alias_name.clone();
                    }
                    format!("{owner}/{name}")
                })
                .collect();
            Ok(Complete::from(items))
        }
        _ => Ok(Complete::empty()),
    }
}
