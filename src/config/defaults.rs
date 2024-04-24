use std::collections::HashMap;

use crate::config::Detect;
use crate::config::Docker;
use crate::config::RemoteConfig;
use crate::utils;

pub fn workspace() -> String {
    String::from("~/dev")
}

pub fn metadir() -> String {
    String::from("~/.local/share/roxide")
}

pub fn cmd() -> String {
    String::from("rox")
}

pub fn docker() -> Docker {
    Docker {
        name: docker_name(),
        args: empty_vec(),
        shell: docker_shell(),
    }
}

pub fn detect() -> Detect {
    Detect {
        auto: false,
        ignores: empty_vec(),
    }
}

pub fn remote(remote: impl AsRef<str>) -> RemoteConfig {
    RemoteConfig {
        clone: None,
        user: None,
        email: None,
        ssh: false,
        labels: None,
        provider: None,
        token: None,
        cache_hours: cache_hours(),
        list_limit: list_limit(),
        api_timeout: api_timeout(),
        api_domain: None,
        owners: empty_map(),
        name: Some(remote.as_ref().to_string()),
        alias_owner_map: None,
        alias_repo_map: None,
    }
}

pub fn docker_name() -> String {
    String::from("docker")
}

pub fn docker_shell() -> String {
    String::from("sh")
}

pub fn keyword_expire() -> u64 {
    utils::DAY
}

pub fn empty_map<K, V>() -> HashMap<K, V> {
    HashMap::new()
}

pub fn empty_vec<T>() -> Vec<T> {
    vec![]
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

pub fn docker_file() -> String {
    String::from("Dockerfile")
}

pub fn step_work_dir() -> String {
    String::from("/work")
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

pub fn enable() -> bool {
    false
}

pub fn empty_string() -> String {
    String::new()
}
