#![allow(dead_code)] // TODO: remove this

use std::process;

mod api;
mod batch;
mod cmd;
mod config;
mod db;
mod exec;
mod filelock;
mod format;
mod repo;
mod scan;
mod term;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let result = cmd::run().await;
    if result.code == 0 {
        return;
    }
    if let Some(message) = result.message {
        eprintln!("{message}");
    }
    process::exit(result.code as _);
}
