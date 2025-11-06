use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::config::remote::RenameContext;
use crate::db::remote_repo::RemoteRepository;
use crate::debug;

use super::{Action, ListPullRequestsOptions, PullRequest, RemoteAPI, RemoteInfo};

pub struct Rename {
    pub upstream: Arc<dyn RemoteAPI>,
    pub rename_ctx: RenameContext,
}

#[async_trait]
impl RemoteAPI for Rename {
    async fn info(&self) -> Result<RemoteInfo> {
        self.upstream.info().await
    }

    async fn list_repos(&self, remote: &str, owner: &str) -> Result<Vec<String>> {
        let (original_owner, rename_cfg) = self.rename_ctx.get_original_owner(owner);
        debug!("[rename] list_repos owner for {remote:?}: {owner:?} => {original_owner:?}");
        let repos = self.upstream.list_repos(remote, original_owner).await?;
        let repos = repos
            .into_iter()
            .map(|name| match rename_cfg {
                Some(cfg) => match cfg.get_renamed_repo(&name) {
                    Some(renamed_name) => {
                        debug!("[rename] list_repos repo: {name:?} => {renamed_name:?}");
                        renamed_name.to_string()
                    }
                    None => name,
                },
                None => name,
            })
            .collect();
        Ok(repos)
    }

    async fn get_repo(
        &self,
        remote: &str,
        owner: &str,
        name: &str,
    ) -> Result<RemoteRepository<'static>> {
        let (original_owner, original_name) = self.rename_ctx.get_original(owner, name);
        debug!("[rename] get_repo: {remote}:{owner}/{name} => {original_owner}/{original_name}");
        self.upstream
            .get_repo(remote, original_owner, original_name)
            .await
    }

    async fn create_pull_request(
        &self,
        owner: &str,
        name: &str,
        pr: &PullRequest,
    ) -> Result<String> {
        let (original_owner, original_name) = self.rename_ctx.get_original(owner, name);
        debug!("[rename] create_pull_request: {owner}/{name} => {original_owner}/{original_name}");
        self.upstream
            .create_pull_request(original_owner, original_name, pr)
            .await
    }

    async fn list_pull_requests(
        &self,
        mut opts: ListPullRequestsOptions,
    ) -> Result<Vec<PullRequest>> {
        let (original_owner, original_name) = self.rename_ctx.get_original(&opts.owner, &opts.name);
        debug!(
            "[rename] list_pull_requests: {}/{} => {original_owner}/{original_name}",
            opts.owner, opts.name
        );
        let original_owner = original_owner.to_string();
        let original_name = original_name.to_string();
        opts.name = original_name;
        opts.owner = original_owner;
        self.upstream.list_pull_requests(opts).await
    }

    async fn get_action_optional(
        &self,
        owner: &str,
        name: &str,
        commit: &str,
    ) -> Result<Option<Action>> {
        let (original_owner, original_name) = self.rename_ctx.get_original(owner, name);
        debug!("[rename] get_action_optional: {owner}/{name} => {original_owner}/{original_name}");
        self.upstream
            .get_action_optional(original_owner, original_name, commit)
            .await
    }

    async fn get_job_log(&self, owner: &str, name: &str, id: u64) -> Result<String> {
        let (original_owner, original_name) = self.rename_ctx.get_original(owner, name);
        debug!("[rename] get_job_log: {owner}/{name} => {original_owner}/{original_name}");
        self.upstream
            .get_job_log(original_owner, original_name, id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use anyhow::bail;

    use crate::config::context;

    use super::*;

    struct MockUpstream {}

    #[async_trait]
    impl RemoteAPI for MockUpstream {
        async fn info(&self) -> Result<RemoteInfo> {
            Ok(RemoteInfo {
                name: Cow::Borrowed("MockUpstream"),
                auth_user: None,
                ping: true,
                cache: false,
            })
        }

        async fn list_repos(&self, _remote: &str, owner: &str) -> Result<Vec<String>> {
            if owner == "torpedo" {
                return Ok(vec!["cube-deploy".to_string(), "cube-core".to_string()]);
            }
            if owner == "kubernetes" {
                return Ok(vec![
                    "kubernetes".to_string(),
                    "kubernetes-sigs".to_string(),
                    "kubectl".to_string(),
                ]);
            }
            Ok(vec![])
        }

        async fn get_repo(
            &self,
            _remote: &str,
            owner: &str,
            name: &str,
        ) -> Result<RemoteRepository<'static>> {
            if owner == "kubernetes" {
                if name == "kubernetes-sigs" {
                    return Ok(RemoteRepository {
                        owner: Cow::Borrowed("kubernetes"),
                        name: Cow::Borrowed("kubernetes-sigs"),
                        ..Default::default()
                    });
                }
                if name == "kubectl" {
                    return Ok(RemoteRepository {
                        owner: Cow::Borrowed("kubernetes"),
                        name: Cow::Borrowed("kubectl"),
                        ..Default::default()
                    });
                }
            }
            if owner == "torpedo" && name == "cube-deploy" {
                return Ok(RemoteRepository {
                    owner: Cow::Borrowed("torpedo"),
                    name: Cow::Borrowed("cube-deploy"),
                    ..Default::default()
                });
            }
            bail!("repo not found");
        }

        async fn create_pull_request(
            &self,
            _owner: &str,
            _name: &str,
            _pr: &PullRequest,
        ) -> Result<String> {
            unreachable!()
        }

        async fn list_pull_requests(
            &self,
            _opts: ListPullRequestsOptions,
        ) -> Result<Vec<PullRequest>> {
            unreachable!()
        }

        async fn get_action_optional(
            &self,
            _owner: &str,
            _name: &str,
            _commit: &str,
        ) -> Result<Option<Action>> {
            unreachable!()
        }

        async fn get_job_log(&self, _owner: &str, _name: &str, _id: u64) -> Result<String> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn test_rename() {
        let ctx = context::tests::build_test_context("api_rename");

        let mock = Arc::new(MockUpstream {});
        let rename = Rename {
            upstream: mock,
            rename_ctx: ctx.cfg.get_remote("gitlab").unwrap().rename_ctx.clone(),
        };

        let repos = rename.list_repos("gitlab", "k8s").await.unwrap();
        assert_eq!(
            repos,
            vec![
                "k8s".to_string(),
                "k8s-sigs".to_string(),
                "kubectl".to_string()
            ]
        );

        let repos = rename.list_repos("gitlab", "cube").await.unwrap();
        assert_eq!(
            repos,
            vec!["cube-deploy".to_string(), "cube-core".to_string()]
        );

        let repos = rename.list_repos("gitlab", "torpedo").await.unwrap();
        assert_eq!(
            repos,
            vec!["cube-deploy".to_string(), "cube-core".to_string()]
        );
        let repos = rename.list_repos("gitlab", "fioncat").await.unwrap();
        assert!(repos.is_empty());

        let repo = rename.get_repo("gitlab", "k8s", "k8s-sigs").await.unwrap();
        assert_eq!(repo.owner, "kubernetes");
        assert_eq!(repo.name, "kubernetes-sigs");

        let repo = rename.get_repo("gitlab", "k8s", "kubectl").await.unwrap();
        assert_eq!(repo.owner, "kubernetes");
        assert_eq!(repo.name, "kubectl");

        let repo = rename
            .get_repo("gitlab", "cube", "cube-deploy")
            .await
            .unwrap();
        assert_eq!(repo.owner, "torpedo");
        assert_eq!(repo.name, "cube-deploy");
    }
}
