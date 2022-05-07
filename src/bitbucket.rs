use reqwest::header::HeaderMap;
use std::fmt::{Display, Formatter};

use crate::api::{ApiClient, BasicAuth};
use crate::config::BitbucketConfig;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
    pub uuid: String,
    pub key: String,
    pub name: String,
}

impl Project {
    pub fn get_key(&self) -> &str {
        &self.key
    }
}

impl Display for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Key: {})", self.name, self.key)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Repository {
    links: RepositoryLinks,
    pub full_name: String,
    pub name: String,
    #[serde(rename = "mainbranch")]
    pub main_branch: Branch,
}

impl Display for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (branch: {})", self.name, self.main_branch)
    }
}

impl Repository {
    pub fn get_ssh_url(&self) -> Option<String> {
        for link in &self.links.clone {
            if let CloneLink::Ssh(url) = link {
                return Some(url.clone());
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RepositoryLinks {
    clone: Vec<CloneLink>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "name", content = "href")]
#[serde(rename_all = "snake_case")]
enum CloneLink {
    Ssh(String),
    Https(String),
}

#[derive(Deserialize, Debug)]
struct PageResponse<T> {
    values: Vec<T>,
    next: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Branch {
    pub name: String,
}

impl Display for Branch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub(crate) struct BitbucketApi {
    config: BitbucketConfig,
}

impl BitbucketApi {
    pub fn new(config: &BitbucketConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub async fn get_projects(&self) -> Result<Vec<Project>, anyhow::Error> {
        let url = format!(
            "https://api.bitbucket.org/2.0/workspaces/{workspace}/projects",
            workspace = &self.config.workspace_name
        );

        let projects = self.get_all_pages(url).await?;

        Ok(projects)
    }

    pub async fn get_project_repositories(
        &self,
        project_key: &str,
    ) -> Result<Vec<Repository>, anyhow::Error> {
        let url = format!("https://api.bitbucket.org/2.0/repositories/{workspace}?q=project.key=\"{key}\"&pagelen={pagelen}", workspace = &self.config.workspace_name, key = project_key, pagelen = 100);
        let res: PageResponse<Repository> = self.get(url).await?;

        Ok(res.values)
    }

    pub async fn get_repository_branches(
        &self,
        full_repo_name: &str,
    ) -> anyhow::Result<Vec<Branch>> {
        let url = format!("https://api.bitbucket.org/2.0/repositories/{full_repo_name}/refs/branches?pagelen={pagelen}", full_repo_name = full_repo_name, pagelen = 100);

        let branches = self.get_all_pages(url).await?;

        Ok(branches)
    }

    pub async fn get_repository(&self, repo_name: &str) -> anyhow::Result<Option<Repository>> {
        let url = format!(
            "https://api.bitbucket.org/2.0/repositories/{repo_name}",
            repo_name = repo_name
        );
        let res = self.get(url).await;

        match res {
            Ok(res) => Ok(Some(res)),
            Err(err) => match err.status() {
                Some(status) => {
                    if status.as_u16() == 404 {
                        Ok(None)
                    } else {
                        Err(anyhow::anyhow!(
                            "Error: Repository {} was not found in Bitbucket account: {}",
                            &repo_name,
                            err
                        ))
                    }
                }
                None => Err(anyhow::anyhow!("Unknown error: {}", err)),
            },
        }
    }

    async fn get_all_pages<T>(&self, initial_url: String) -> anyhow::Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let mut result = vec![];
        let mut url = initial_url;
        loop {
            let response: PageResponse<T> = self.get(url).await?;
            result.extend(response.values);

            if let Some(next_url) = response.next {
                url = next_url;
            } else {
                break;
            }
        }

        Ok(result)
    }
}

impl ApiClient for BitbucketApi {
    fn basic_auth(&self) -> Option<BasicAuth> {
        Some(BasicAuth::new(&self.config.username, &self.config.password))
    }

    fn headers(&self) -> Option<HeaderMap> {
        None
    }
}
