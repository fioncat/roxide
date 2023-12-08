mod api;
mod batch;
mod cmd;
mod config;
mod errors;
mod repo;
mod term;
mod utils;

use anyhow::Result;
use clap::Parser;

use crate::cmd::{App, Run};
use crate::config::Config;

fn run() -> Result<()> {
    let cfg = Config::load()?;
    App::parse().run(&cfg)
}

fn main() {
    console::set_colors_enabled(true);
    utils::handle_result(run());
}
