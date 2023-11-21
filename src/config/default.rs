use std::collections::{HashMap, HashSet};

use crate::config::types::{Base, Command};

pub fn workspace() -> String {
    String::from("~/src")
}

pub fn metadir() -> String {
    String::from("~/.local/share/roxide")
}

pub fn command() -> Command {
    Command {
        base: Some(String::from("ro")),
        home: Some(String::from("rh")),
        remotes: empty_map(),
    }
}

pub fn empty_map<K, V>() -> HashMap<K, V> {
    HashMap::new()
}

pub fn empty_set<K>() -> HashSet<K> {
    HashSet::new()
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
        sync: empty_map(),
        workflows: empty_map(),
        release: release(),
    }
}

pub fn disable() -> bool {
    false
}

pub fn empty_string() -> String {
    String::new()
}
