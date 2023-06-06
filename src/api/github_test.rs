use std::collections::HashMap;
use std::env;

use crate::config::types::Remote;

use super::github::Github;

#[test]
fn test_get() {
    let token = env::var_os("GITHUB_TOKEN");
    if let None = token {
        // No token, skip this test
        return;
    }
    let token = token.unwrap().to_str().unwrap().to_string();
    let remote = Remote {
        name: String::from("github"),
        clone: None,
        user: None,
        email: None,
        ssh: false,
        provider: None,
        token: Some(token),
        cache_hours: 24,
        owners: HashMap::new(),
        list_limit: 100,
        api_timeout: 10,
        api_domain: None,
    };

    let github = Github::new(&remote);

    // Get myself
    let repo = github.get_repo("fioncat", "roxide").unwrap();

    assert_eq!(repo.name, "roxide");
    assert_eq!(repo.default_branch, "main");
    assert_eq!(repo.upstream, None);
}

#[test]
fn test_list() {
    let token = env::var_os("GITHUB_TOKEN");
    if let None = token {
        // No token, skip this test
        return;
    }
    let token = token.unwrap().to_str().unwrap().to_string();
    let remote = Remote {
        name: String::from("github"),
        clone: None,
        user: None,
        email: None,
        ssh: false,
        provider: None,
        token: Some(token),
        cache_hours: 24,
        owners: HashMap::new(),
        list_limit: 100,
        api_timeout: 10,
        api_domain: None,
    };

    let github = Github::new(&remote);

    let repos = github.list_repos("fioncat").unwrap();
    assert_ne!(repos.len(), 0);

    let mut found_roxide = false;
    let mut found_readme = false;
    for repo in repos {
        if repo == "roxide" {
            found_roxide = true;
        }
        if repo == "fioncat" {
            found_readme = true;
        }
    }

    if !found_readme {
        panic!("readme not found!");
    }
    if !found_roxide {
        panic!("roxide not found!");
    }
}
