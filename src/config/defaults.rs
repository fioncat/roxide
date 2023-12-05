use std::collections::{HashMap, HashSet};

pub fn workspace() -> String {
    String::from("~/dev")
}

pub fn metadir() -> String {
    String::from("~/.local/share/roxide")
}

pub fn cmd() -> String {
    String::from("ro")
}

pub fn empty_map<K, V>() -> HashMap<K, V> {
    HashMap::new()
}

pub fn empty_set<K>() -> HashSet<K> {
    HashSet::new()
}

pub fn release() -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(5);
    map.insert(String::from("patch"), String::from("v{0}.{1}.{2+}"));
    map.insert(String::from("minor"), String::from("v{0}.{1+}.0"));
    map.insert(String::from("major"), String::from("v{0+}.0.0"));

    map.insert(String::from("date-stash"), String::from("{%Y}-{%m}-{%d}"));
    map.insert(String::from("date-dot"), String::from("{%Y}.{%m}.{%d}"));

    map
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

pub fn disable() -> bool {
    false
}

pub fn empty_string() -> String {
    String::new()
}
