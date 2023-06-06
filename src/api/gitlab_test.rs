#![allow(dead_code)]

use std::collections::HashMap;
use std::env;

use crate::config::types::Remote;

use super::gitlab::Gitlab;

fn get_remote() -> Option<Remote> {
    let domain = get_env("GITLAB_DOMAIN");
    if domain == "" {
        return None;
    }
    let token = get_env("GITLAB_TOKEN");
    if token == "" {
        return None;
    }
    Some(Remote {
        name: String::from("gitlab"),
        clone: None,
        user: None,
        email: None,
        ssh: false,
        provider: None,
        token: Some(token),
        cache_hours: 24,
        owners: HashMap::new(),
        list_limit: 200,
        api_timeout: 10,
        api_domain: Some(domain),
    })
}

fn get_env(name: &str) -> String {
    match env::var_os(name) {
        Some(val) => val.to_str().unwrap().to_string(),
        None => String::new(),
    }
}

#[test]
fn test_get() {
    let remote = get_remote();
    if let None = remote {
        return;
    }
    let remote = remote.unwrap();

    let owner = get_env("GITLAB_OWNER");
    let name = get_env("GITLAB_NAME");
    if owner == "" || name == "" {
        return;
    }

    let gitlab = Gitlab::new(&remote);

    let repo = gitlab.get_repo(&owner, &name).unwrap();

    assert_eq!(repo.name, name);
}

#[test]
fn test_list() {
    let remote = get_remote();
    if let None = remote {
        return;
    }
    let remote = remote.unwrap();

    let owner = get_env("GITLAB_OWNER");
    let name = get_env("GITLAB_NAME");
    if owner == "" || name == "" {
        return;
    }

    let gitlab = Gitlab::new(&remote);

    let repos = gitlab.list_repos(&owner).unwrap();
    let mut found = false;
    for repo in repos {
        println!("repo: {}", repo);
        if repo == name {
            found = true;
            break;
        }
    }
    if !found {
        panic!("could not find expect repo {name} in {owner}");
    }
}
