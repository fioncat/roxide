use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Request};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::{ApiRepo, ApiUpstream, MergeOptions, Provider};
use crate::repo::Remote;

#[derive(Debug, Deserialize)]
struct Repo {
    pub name: String,
    pub full_name: String,
    pub html_url: String,

    pub source: Option<Source>,

    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct SearchRepoResult {
    pub items: Vec<Repo>,
}

#[derive(Debug, Deserialize)]
struct Source {
    pub name: String,
    pub owner: Owner,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
struct Owner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct Error {
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    pub tag_name: String,
}

impl Repo {
    fn to_api(self) -> ApiRepo {
        let Repo {
            name: _,
            html_url,
            full_name: _,
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
            default_branch,
            upstream,
            web_url: html_url,
        }
    }
}

struct PullRequestOptions {
    owner: String,
    name: String,

    head: String,
    head_search: String,

    base: String,
}

#[derive(Debug, Serialize)]
struct PullRequestBody {
    head: String,
    base: String,

    title: String,
    body: String,
}

impl From<MergeOptions> for PullRequestOptions {
    fn from(merge: MergeOptions) -> Self {
        let MergeOptions {
            owner,
            name,
            upstream,
            source,
            target,
        } = merge;

        let (head, head_search, owner, name) = match upstream {
            Some(upstream) => {
                let head = format!("{owner}:{source}");
                let head_search = format!("{owner}/{source}");
                (head, head_search, upstream.owner, upstream.name)
            }
            None => {
                let head_search = source.clone();
                (source, head_search, owner, name)
            }
        };
        PullRequestOptions {
            owner,
            name,
            head,
            head_search,
            base: target,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    html_url: String,
}

pub struct Github {
    token: Option<String>,

    client: Client,

    per_page: u32,
}

impl Provider for Github {
    fn list_repos(&self, owner: &str) -> Result<Vec<String>> {
        let path = format!("users/{owner}/repos?per_page={}", self.per_page);
        let github_repos = self.execute_get::<Vec<Repo>>(&path)?;
        let repos: Vec<String> = github_repos.into_iter().map(|repo| repo.name).collect();
        Ok(repos)
    }

    fn get_repo(&self, owner: &str, name: &str) -> Result<ApiRepo> {
        let path = format!("repos/{}/{}", owner, name);
        Ok(self.execute_get::<Repo>(&path)?.to_api())
    }

    fn get_merge(&self, merge: MergeOptions) -> Result<Option<String>> {
        let opts: PullRequestOptions = merge.into();
        let head = urlencoding::encode(&opts.head_search);
        let base = urlencoding::encode(&opts.base);
        let path = format!(
            "repos/{}/{}/pulls?state=open&head={}&base={}",
            opts.owner, opts.name, head, base
        );
        let mut prs = self.execute_get::<Vec<PullRequest>>(&path)?;
        if prs.is_empty() {
            return Ok(None);
        }
        Ok(Some(prs.remove(0).html_url))
    }

    fn create_merge(&mut self, merge: MergeOptions, title: String, body: String) -> Result<String> {
        let opts: PullRequestOptions = merge.into();
        let path = format!("repos/{}/{}/pulls", opts.owner, opts.name);
        let body = PullRequestBody {
            head: opts.head,
            base: opts.base,
            title,
            body,
        };
        let pr = self.execute_post::<PullRequestBody, PullRequest>(&path, body)?;
        Ok(pr.html_url)
    }

    fn search_repos(&self, query: &str) -> Result<Vec<String>> {
        let path = format!("search/repositories?q={}", query);
        let result = self.execute_get::<SearchRepoResult>(&path)?;
        let repos: Vec<String> = result
            .items
            .into_iter()
            .map(|item| item.full_name)
            .collect();

        Ok(repos)
    }
}

impl Github {
    const API_VERSION: &'static str = "2022-11-28";

    pub fn new(remote: &Remote) -> Box<dyn Provider> {
        let client = super::build_common_client(remote);
        Box::new(Github {
            token: remote.cfg.token.clone(),
            per_page: remote.cfg.list_limit,
            client,
        })
    }

    pub fn new_empty() -> Github {
        let client = Client::builder()
            .timeout(Duration::from_secs_f32(20.0))
            .build()
            .unwrap();
        Github {
            token: None,
            per_page: 30,
            client,
        }
    }

    fn execute_get<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let req = self
            .build_request(path, Method::GET, None)
            .context("build Github request")?;
        self.execute(req)
    }

    fn execute_post<B, R>(&self, path: &str, body: B) -> Result<R>
    where
        B: Serialize,
        R: DeserializeOwned + ?Sized,
    {
        let body = serde_json::to_vec(&body).context("encode Github request body")?;
        let req = self.build_request(path, Method::POST, Some(body))?;
        self.execute(req)
    }

    fn execute<T>(&self, req: Request) -> Result<T>
    where
        T: DeserializeOwned + ?Sized,
    {
        let resp = self.client.execute(req).context("Github http request")?;
        let ok = resp.status().is_success();
        let data = resp.bytes().context("read Github response body")?;
        if ok {
            return serde_json::from_slice(&data).context("decode Github response data");
        }

        match serde_json::from_slice::<Error>(&data) {
            Ok(err) => bail!("Github api error: {}", err.message),
            Err(_err) => bail!(
                "unknown GitHub api error: {}",
                String::from_utf8(data.to_vec())
                    .context("decode Github response to UTF-8 string")?
            ),
        }
    }

    fn build_request(&self, path: &str, method: Method, body: Option<Vec<u8>>) -> Result<Request> {
        let url = format!("https://api.github.com/{path}");
        let url = Url::parse(url.as_str()).with_context(|| format!("parse url {url}"))?;
        let mut builder = self.client.request(method, url);
        builder = builder
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "roxide-client")
            .header("X-GitHub-Api-Version", Self::API_VERSION);
        if let Some(token) = &self.token {
            let token_value = format!("Bearer {token}");
            builder = builder.header("Authorization", token_value);
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        builder.build().context("build request")
    }

    pub fn get_latest_tag(&self, owner: &str, name: &str) -> Result<String> {
        let path = format!("repos/{owner}/{name}/releases/latest");
        let release = self.execute_get::<Release>(&path)?;
        Ok(release.tag_name)
    }
}
