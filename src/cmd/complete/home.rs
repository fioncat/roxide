use anyhow::Result;
use roxide::{config, repo::database::Database};

pub fn complete(args: &[String]) -> Result<Vec<String>> {
    match args.len() {
        0 | 1 => {
            let remotes = config::get().list_remotes();
            let items: Vec<_> = remotes
                .into_iter()
                .map(|remote| remote.to_string())
                .collect();
            Ok(items)
        }
        2 => {
            let remote_name = &args[0];
            let db = Database::read()?;

            let query = &args[1];
            if !query.contains("/") {
                let owners = db.list_owners(remote_name);
                let items: Vec<_> = owners
                    .into_iter()
                    .map(|owner| format!("{}/", owner))
                    .collect();
                return Ok(items);
            }

            let repos = db.list_by_remote(remote_name);
            let items: Vec<_> = repos
                .into_iter()
                .filter(|repo| repo.long_name().starts_with(query))
                .map(|repo| format!("{}", repo.long_name()))
                .collect();
            Ok(items)
        }
        _ => Ok(vec![]),
    }
}
