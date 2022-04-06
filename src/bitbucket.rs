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

impl Display for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Key: {})", self.name, self.key)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProjectResponse {
  values: Vec<Project>,
  next: Option<String>
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

async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(url: U) -> Result<T, reqwest::Error> {
  let client = reqwest::Client::new();
  let res = client.get(url).basic_auth(USERNAME, Some(PASSWORD)).send().await?.json::<T>().await?;

  Ok(res)
}
