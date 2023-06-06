use std::env;

use anyhow::{bail, Result};
use roxide::{api, config, utils};

fn main() {
    utils::handle_result(run())
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        bail!("Usage: <remote> <owner>");
    }

    let remote_name = args.get(1).unwrap();
    let owner = args.get(2).unwrap();
    let remote = config::must_get_remote(remote_name)?;

    let provider = api::init_provider(&remote)?;

    let repos = provider.list_repos(&owner)?;
    for repo in repos {
        println!("{repo}");
    }

    Ok(())
}
