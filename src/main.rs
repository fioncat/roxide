#![allow(dead_code)] // TODO: remove this

use crate::exec::Cmd;

mod api;
mod config;
mod db;
mod exec;
mod term;

fn main() {
    let mut cmd = Cmd::new("echo").args(&["hello"]);
    cmd.execute().unwrap();
}
