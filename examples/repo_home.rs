use std::env;
use std::rc::Rc;

use anyhow::{bail, Result};
use roxide::repo::database::Database;
use roxide::repo::types::Repo;
use roxide::{config, confirm, utils};

fn main() {
    utils::handle_result(run())
}

fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().collect();
    args = args[1..].to_vec();

    let mut db = Database::read()?;
    let repo = get_repo(args, &db)?;

    println!("Path: {}", repo.get_path().display());
    db.update(repo);

    db.close()
}

fn get_repo(args: Vec<String>, db: &Database) -> Result<Rc<Repo>> {
    if args.len() == 0 {
        return db.must_latest("");
    }
    let remote_name = args.get(0).unwrap();
    let maybe_remote = config::get_remote(remote_name)?;
    if let None = maybe_remote {
        return db.must_get_fuzzy("", remote_name);
    }
    let remote = maybe_remote.unwrap();
    if args.len() == 1 {
        return db.must_latest(&remote.name);
    }

    let query = args.get(1).unwrap();
    let (owner, name) = utils::parse_query(&query);
    if name.is_empty() {
        bail!("Query name can not be empty");
    }

    if owner.is_empty() {
        return db.must_get_fuzzy(&remote.name, &name);
    }

    if let Some(repo) = db.get(&remote.name, &owner, &name) {
        return Ok(repo);
    }

    confirm!("Could not find repo {}, do you want to create it", query);
    Ok(Repo::new(&remote.name, &owner, &name, None))
}
