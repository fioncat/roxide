use std::env;
use std::rc::Rc;

use anyhow::Result;
use roxide::repo::database::Database;
use roxide::repo::types::Repo;
use roxide::utils;

fn main() {
    utils::handle_result(run())
}

fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().collect();
    args = args[1..].to_vec();
    let repos = list_repo(args)?;
    for repo in repos {
        println!("{repo:?}");
    }
    Ok(())
}

fn list_repo(args: Vec<String>) -> Result<Vec<Rc<Repo>>> {
    let db = Database::read()?;
    if args.len() == 0 {
        return Ok(db.list_all());
    }
    let remote = args.get(0).unwrap();
    if args.len() == 1 {
        return Ok(db.list_by_remote(&remote));
    }

    let owner = args.get(1).unwrap();
    Ok(db.list_by_owner(&remote, &owner))
}
