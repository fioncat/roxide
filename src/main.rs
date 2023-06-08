mod cmd;

use clap::Parser;
use roxide::utils;

use crate::cmd::{App, Run};

fn main() {
    console::set_colors_enabled(true);
    utils::handle_result(App::parse().run());
}
