use anyhow::Result;
use roxide::config;
use roxide::utils;

fn main() {
    println!("Base: {:?}", config::base());

    utils::handle_result(run())
}

fn run() -> Result<()> {
    let remotes = config::list_remotes();
    for remote in remotes {
        let remote_cfg = config::must_get_remote(remote)?;
        println!("Remote: {:?}", remote_cfg);
    }
    Ok(())
}
