use std::borrow::Cow;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use octocrab::Octocrab;

use crate::db::remote_repo::{RemoteRepository, RemoteUpstream};
use crate::{debug, warn};

use super::{RemoteAPI, RemoteInfo};

pub struct GitHub {
    client: Octocrab,
    has_token: bool,
}

impl GitHub {
    const LIST_LIMIT: u8 = 100;

    pub fn new(token: &str) -> Result<Self> {
        let mut has_token = false;
        debug!("[github] Build GitHub API client, token: {token:?}");
        let client = if token.is_empty() {
            warn!("GitHub token is empty, using unauthenticated client which may be rate limited");
            Octocrab::builder().build()?
        } else {
            has_token = true;
            Octocrab::builder().personal_token(token).build()?
        };

        Ok(Self { client, has_token })
    }
}

#[async_trait]
impl RemoteAPI for GitHub {
    async fn info(&self) -> Result<RemoteInfo> {
        debug!("[github] Get GitHub API info");
        let mut auth_user = None;
        let ping = if self.has_token {
            let user = self
                .client
                .current()
                .user()
                .await
                .context("get current user")?;
            auth_user = Some(user.login);
            true
        } else {
            super::test_connection("api.github.com:443").await
        };
        let info = RemoteInfo {
            name: Cow::Borrowed("GitHub API"),
            auth_user,
            ping,
            cache: false,
        };
        debug!("[github] Result: {info:?}");
        Ok(info)
    }

    async fn list_repos(&self, _remote: &str, owner: &str) -> Result<Vec<String>> {
        debug!("[github] List repos for: {owner}");
        let page = self
            .client
            .users(owner)
            .repos()
            .per_page(Self::LIST_LIMIT)
            .send()
            .await
            .context("failed to fetch github user repos")?;

        let mut names = Vec::with_capacity(page.items.len());
        for item in page.items {
            names.push(item.name);
        }
        debug!("[github] Results: {names:?}");
        Ok(names)
    }

    async fn get_repo(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<RemoteRepository<'static>> {
        debug!("[github] Get repo: {owner}/{name}");
        let repo = self
            .client
            .repos(owner, name)
            .get()
            .await
            .context("failed to fetch github repo")?;

        let Some(default_branch) = repo.default_branch else {
            bail!("github repo {owner}/{name} default branch is empty");
        };

        let Some(web_url) = repo.html_url.map(|u| u.to_string()) else {
            bail!("github repo {owner}/{name} html url is empty");
        };

        let upstream = if let Some(s) = repo.source {
            let Some(u_owner) = s.owner else {
                bail!("github repo {owner}/{name} upstream owner is empty");
            };
            let u_owner = u_owner.login;

            let Some(u_default_branch) = s.default_branch else {
                bail!("github repo {owner}/{name} upstream default branch is empty");
            };

            Some(RemoteUpstream {
                owner: u_owner,
                name: s.name,
                default_branch: u_default_branch,
            })
        } else {
            None
        };
        let remote_repo = RemoteRepository {
            remote: Cow::Owned(remote.to_string()),
            owner: Cow::Owned(owner.to_string()),
            name: Cow::Owned(name.to_string()),
            default_branch,
            web_url,
            upstream,
            expire_at: 0,
        };
        debug!("[github] Result: {remote_repo:?}");
        Ok(remote_repo)
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    fn new() -> Option<GitHub> {
        let token = env::var("TEST_GITHUB_TOKEN").ok()?;
        if token.is_empty() {
            return None;
        }
        Some(GitHub::new(&token).unwrap())
    }

    #[tokio::test]
    async fn test_info() {
        let Some(github) = new() else {
            return;
        };
        let info = github.info().await.unwrap();
        assert_eq!(
            info,
            RemoteInfo {
                name: Cow::Borrowed("GitHub API"),
                auth_user: Some("fioncat".to_string()),
                ping: true,
                cache: false,
            }
        );
    }

    #[tokio::test]
    async fn test_list_repos() {
        let Some(github) = new() else {
            return;
        };
        let repos = github.list_repos("github", "fioncat").await.unwrap();
        assert!(repos.contains(&"roxide".to_string()));
    }

    #[tokio::test]
    async fn test_get_repo() {
        let Some(github) = new() else {
            return;
        };
        let repo = github
            .get_repo("github", "fioncat", "roxide")
            .await
            .unwrap();
        assert_eq!(
            repo,
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("fioncat".to_string()),
                name: Cow::Owned("roxide".to_string()),
                default_branch: "main".to_string(),
                web_url: "https://github.com/fioncat/roxide".to_string(),
                upstream: None,
                expire_at: 0,
            }
        );

        let repo = github
            .get_repo("github", "fioncat", "kubernetes")
            .await
            .unwrap();
        assert_eq!(
            repo,
            RemoteRepository {
                remote: Cow::Owned("github".to_string()),
                owner: Cow::Owned("fioncat".to_string()),
                name: Cow::Owned("kubernetes".to_string()),
                default_branch: "master".to_string(),
                web_url: "https://github.com/fioncat/kubernetes".to_string(),
                upstream: Some(RemoteUpstream {
                    owner: "kubernetes".to_string(),
                    name: "kubernetes".to_string(),
                    default_branch: "master".to_string(),
                }),
                expire_at: 0,
            }
        );
    }
}
