use std::env;

use anyhow::{bail, Result};
use roxide::{api, config, utils};

fn main() {
    utils::handle_result(run())
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        bail!("Usage: <remote> <owner> <name>");
    }
    let remote_name = args.get(1).unwrap();
    let owner = args.get(2).unwrap();
    let name = args.get(3).unwrap();

    let remote = config::must_get_remote(remote_name)?;

    let provider = api::init_provider(&remote)?;

    let repo = provider.get_repo(&owner, &name)?;
    println!("{repo:?}");

    Ok(())
}
