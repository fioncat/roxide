mod api;
mod batch;
mod cmd;
mod config;
mod errors;
mod repo;
mod term;
mod utils;

use clap::Parser;

use crate::cmd::{App, Run};

fn main() {
    console::set_colors_enabled(true);
    utils::handle_result(App::parse().run());
}
