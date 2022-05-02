use std::fmt::{Display, Formatter};
use reqwest::header::HeaderMap;

use crate::config::{BitbucketConfig};
use serde::{Deserialize, Serialize};
use crate::api::{ApiClient, BasicAuth};

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

#[derive(Serialize, Deserialize, Debug)]
struct ProjectResponse {
    values: Vec<Project>,
    next: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RepositoriesResponse {
    values: Vec<Repository>,
    next: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Repository {
    links: RepositoryLinks,
    pub full_name: String,
    pub name: String,
    pub mainbranch: Branch,
}

impl Display for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (branch: {})", self.name, self.mainbranch)
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

#[derive(Serialize, Deserialize, Debug)]
struct BranchesResponse {
    values: Vec<Branch>,
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

pub(crate) struct BitbucketApi<'a> {
    config: &'a BitbucketConfig,
}

impl<'a> BitbucketApi<'a> {
    pub fn new(config: &'a BitbucketConfig) -> Self {
        Self {
            config
        }
    }

    pub async fn get_projects(&self) -> Result<Vec<Project>, anyhow::Error> {
        let url = format!(
            "https://api.bitbucket.org/2.0/workspaces/{workspace}/projects",
            workspace = &self.config.workspace_name
        );
        let mut projects_res: ProjectResponse = self.get(url).await?;

        let mut projects = projects_res.values.clone();
        while projects_res.next.is_some() {
            projects_res = self.get(projects_res.next.unwrap()).await?;
            projects.append(&mut projects_res.values);
        }

        Ok(projects)
    }

    pub async fn get_project_repositories(&self, project_key: &str) -> Result<Vec<Repository>, anyhow::Error> {
        let url = format!("https://api.bitbucket.org/2.0/repositories/{workspace}?q=project.key=\"{key}\"&pagelen={pagelen}", workspace = &self.config.workspace_name, key = project_key, pagelen = 100);
        let res: RepositoriesResponse = self.get(url).await?;

        Ok(res.values)
    }

    pub async fn get_repository_branches(&self, full_repo_name: &str) -> anyhow::Result<Vec<Branch>> {
        let url = format!("https://api.bitbucket.org/2.0/repositories/{full_repo_name}/refs/branches?pagelen={pagelen}", full_repo_name = full_repo_name, pagelen = 100);

        let mut branches_res: BranchesResponse = self.get(url).await?;

        let mut branches = branches_res.values.clone();
        while branches_res.next.is_some() {
            branches_res = self.get(branches_res.next.unwrap()).await?;
            branches.append(&mut branches_res.values);
        }

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
}

impl<'a> ApiClient for BitbucketApi<'a> {
    fn basic_auth(&self) -> Option<BasicAuth> {
        Some(BasicAuth::new(&self.config.username, &self.config.password))
    }

    fn headers(&self) -> Option<HeaderMap> {
        None
    }
}