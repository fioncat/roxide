#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::config::types::Remote;

use super::{ApiRepo, ApiUpstream, Provider};

#[derive(Debug, Deserialize)]
struct GithubRepo {
    pub name: String,
    pub html_url: String,

    pub source: Option<GithubSource>,

    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct GithubSource {
    pub name: String,
    pub owner: GithubOwner,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct GithubOwner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct GithubError {
    pub message: String,
}

impl GithubRepo {
    fn to_api(self) -> ApiRepo {
        let GithubRepo {
            name,
            html_url,
            source,
            default_branch,
        } = self;
        let upstream = match source {
            Some(source) => Some(ApiUpstream {
                owner: source.owner.login,
                name: source.name,
                default_branch: source.default_branch,
            }),
            None => None,
        };
        ApiRepo {
            name,
            default_branch,
            upstream,
            web_url: html_url,
        }
    }
}

pub struct Github {
    token: Option<String>,

    client: Client,

    per_page: u32,
}

impl Provider for Github {
    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = format!("users/{owner}/repos?per_page={}", self.per_page);
        let github_repos = self.execute::<Vec<GithubRepo>>(&path)?;
        let repos: Vec<String> = github_repos.into_iter().map(|repo| repo.name).collect();
        Ok(repos)
    }

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let path = format!("repos/{}/{}", owner, name);
        Ok(self.execute::<GithubRepo>(&path)?.to_api())
    }
}

impl Github {
    const API_VERSION: &str = "2022-11-28";

    pub fn new(remote: &Remote) -> Box<dyn Provider> {
        let client = super::build_common_client(remote);
        Box::new(Github {
            token: remote.token.clone(),
            per_page: remote.list_limit,
            client,
        })
    }

    fn execute<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let req = self.build_request(path)?;
        let resp = self.client.execute(req).context("Github http request")?;
        let ok = resp.status().is_success();
        let data = resp.bytes().context("Read Github response body")?;
        if ok {
            return serde_json::from_slice(&data).context("Decode Github response data");
        }

        match serde_json::from_slice::<GithubError>(&data) {
            Ok(err) => bail!("Github api error: {}", err.message),
            Err(_err) => bail!(
                "Unknown Github api error: {}",
                String::from_utf8(data.to_vec())
                    .context("Decode Github response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str) -> Result<Request> {
        let url = format!("https://api.github.com/{path}");
        let url = Url::parse(url.as_str()).with_context(|| format!("Parse url {url}"))?;
        let mut builder = self.client.request(Method::GET, url);
        builder = builder
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "roxide-client")
            .header("X-GitHub-Api-Version", Self::API_VERSION);
        if let Some(token) = &self.token {
            let token_value = format!("Bearer {token}");
            builder = builder.header("Authorization", token_value);
        }

        builder.build().context("Build Github request")
    }
}
