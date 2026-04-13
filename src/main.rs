mod api;
mod batch;
mod check;
mod cmd;
mod config;
mod db;
mod exec;
mod filelock;
mod format;
mod repo;
mod scan;
mod secret;
mod term;
mod vscode_projects;

use std::process;

use rustls::crypto::ring::default_provider;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    default_provider()
        .install_default()
        .expect("Failed to install default CryptoProvider");
    let result = cmd::run().await;
    if result.code == 0 {
        return;
    }
    if let Some(message) = result.message {
        eprintln!("{message}");
    }
    process::exit(result.code as _);
}
