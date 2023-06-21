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

pub fn release() -> HashMap<String, String> {
    let mut hmap = HashMap::with_capacity(5);
    hmap.insert(String::from("patch"), String::from("v{0}.{1}.{2+}"));
    hmap.insert(String::from("minor"), String::from("v{0}.{1+}.0"));
    hmap.insert(String::from("major"), String::from("v{0+}.0.0"));

    hmap.insert(String::from("date-stash"), String::from("{%Y}-{%m}-{%d}"));
    hmap.insert(String::from("date-dot"), String::from("{%Y}.{%m}.{%d}"));

    hmap
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
        release: release(),
    }
}

pub fn disable() -> bool {
    false
}
