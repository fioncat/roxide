#![allow(dead_code)] // TODO: remove this

use crate::exec::Cmd;

mod api;
mod batch;
mod config;
mod db;
mod exec;
mod format;
mod repo;
mod term;

fn main() {
    let mut cmd = Cmd::new("echo").args(&["hello"]);
    cmd.execute().unwrap();
}
