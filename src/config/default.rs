use std::collections::HashMap;

use crate::config::types::{Base, Command, Owner, WorkflowStep};

pub fn workspace() -> String {
    String::from("~/src")
}

pub fn metadir() -> String {
    String::from("~/.local/share/roxide")
}

pub fn command() -> Command {
    Command {
        base: Some(String::from("z")),
        home: Some(String::from("zz")),
        remotes: command_remotes(),
    }
}

pub fn command_remotes() -> HashMap<String, String> {
    HashMap::new()
}

pub fn workflows() -> HashMap<String, Vec<WorkflowStep>> {
    HashMap::new()
}

pub fn cache_hours() -> u32 {
    24
}

pub fn owners() -> HashMap<String, Owner> {
    HashMap::new()
}

pub fn list_limit() -> u32 {
    200
}

pub fn api_timeout() -> u64 {
    10
}

pub fn base() -> Base {
    Base {
        workspace: workspace(),
        metadir: metadir(),
        command: command(),
        workflows: workflows(),
    }
}

pub fn disable() -> bool {
    false
}
