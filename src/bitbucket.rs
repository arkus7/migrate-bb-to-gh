use std::fmt::Display;

use reqwest::IntoUrl;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const USERNAME: &str = "arek-moodup";
const PASSWORD: &str = "LpymWNsc7KutVfgTRzqb";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
  uuid: String,
  key: String,
  name: String
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
  next: Option<String>
}

#[derive(Serialize, Deserialize, Debug)]
struct RepositoriesResponse {
  values: Vec<Repository>,
  next: Option<String>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Repository {
  links: RepositoryLinks,
  full_name: String,
  name: String,
  mainbranch: MainBranch,
}

impl Display for Repository {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.name)
  }
}

#[derive(Serialize, Deserialize, Debug)]
struct RepositoryLinks {
  clone: Vec<CloneLink>
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "name", content = "href")]
#[serde(rename_all = "snake_case")]
enum CloneLink {
  Ssh(String),
  Https(String)
}


#[derive(Serialize, Deserialize, Debug)]
struct MainBranch {
  name: String
}

pub async fn get_projects() -> Result<Vec<Project>, anyhow::Error> {
  let url = format!("https://api.bitbucket.org/2.0/workspaces/{workspace}/projects", workspace = "moodup");
  let mut projects_res: ProjectResponse = send_get_request(url).await?;

  let mut projects = projects_res.values.clone();
  while projects_res.next.is_some() {
    projects_res = send_get_request(projects_res.next.unwrap()).await?;
    projects.append(&mut projects_res.values);
  }

  Ok(projects)
}

pub async fn get_repositories(project_key: &str) -> Result<Vec<Repository>, anyhow::Error> {
  let url = format!("https://api.bitbucket.org/2.0/repositories/{workspace}?q=project.key=\"{key}\"&pagelen={pagelen}", workspace = "moodup", key = project_key, pagelen = 100);
  let res: RepositoriesResponse = send_get_request(url).await?;

  Ok(res.values)
}

async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(url: U) -> Result<T, reqwest::Error> {
  let client = reqwest::Client::new();
  let res = client.get(url).basic_auth(USERNAME, Some(PASSWORD)).send().await?.json::<T>().await?;

  Ok(res)
}
