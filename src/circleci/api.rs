mod models;
use crate::config::CONFIG;
use reqwest::IntoUrl;
use serde::{de::DeserializeOwned, Serialize};
use crate::circleci::api::models::{ContextOwnerBody, ContextsResponse, ContextVariablesResponse, CreateContextBody, EnvVarsResponse, ExportEnvironmentBody, StartPipelineBody, UpdateContextVariableBody};

pub use models::{Context, ContextVariable, EnvVar};

const AUTH_HEADER: &str = "Circle-Token";

pub enum Vcs {
    Bitbucket,
    GitHub,
}

impl Vcs {
    fn org_id(&self) -> &str {
        match self {
            Vcs::Bitbucket => &CONFIG.circleci.bitbucket_org_id,
            Vcs::GitHub => &CONFIG.circleci.github_org_id,
        }
    }

    const fn slug_prefix(&self) -> &str {
        match self {
            Vcs::Bitbucket => "bitbucket",
            Vcs::GitHub => "gh",
        }
    }
}

pub async fn get_env_vars(vcs: Vcs, full_repo_name: &str) -> anyhow::Result<Vec<EnvVar>> {
    let project_slug = format!("{}/{}", vcs.slug_prefix(), full_repo_name);
    let url = format!(
        "https://circleci.com/api/v2/project/{project_slug}/envvar",
        project_slug = project_slug,
    );

    let res: Result<EnvVarsResponse, reqwest::Error> = send_get_request(url).await;
    let items = match res {
        Ok(res) => res.items,
        Err(err) => {
            if let Some(code) = err.status() {
                if code == 404 {
                    return Ok(vec![]);
                }
            }
            return Err(anyhow::anyhow!("Failed to get env vars: {}", err));
        }
    };

    Ok(items)
}

pub async fn get_contexts(vcs: Vcs) -> anyhow::Result<Vec<Context>> {
    let url = format!(
        "https://circleci.com/api/v2/context?owner-id={org_id}",
        org_id = vcs.org_id()
    );

    let res: ContextsResponse = send_get_request(url).await?;

    Ok(res.items)
}

pub async fn get_context_variables(context_id: &str) -> anyhow::Result<Vec<ContextVariable>> {
    let url = format!(
        "https://circleci.com/api/v2/context/{context_id}/environment-variable",
        context_id = context_id
    );

    let res: ContextVariablesResponse = send_get_request(url).await?;

    Ok(res.items)
}

pub async fn export_environment(
    from_repo_name: &str,
    to_repo_name: &str,
    env_vars: &[String],
) -> Result<(), anyhow::Error> {
    let url = format!(
        "https://circleci.com/api/v1.1/project/bitbucket/{repo_name}/info/export-environment",
        repo_name = from_repo_name
    );
    let body = ExportEnvironmentBody {
        projects: vec![format!("https://github.com/{}", to_repo_name)],
        env_vars: env_vars.to_vec(),
    };

    let _: serde_json::Value = send_post_request(url, Some(body)).await?;
    Ok(())
}

pub async fn start_pipeline(repo_name: &str, branch: &str) -> Result<(), anyhow::Error> {
    let url = format!(
        "https://circleci.com/api/v1.1/project/gh/{repo_name}/follow",
        repo_name = repo_name
    );
    let body = StartPipelineBody { branch };

    let r: serde_json::Value = send_post_request(url, Some(body)).await?;

    dbg!(&r);

    Ok(())
}

pub async fn create_context(name: &str, vcs: Vcs) -> Result<Context, anyhow::Error> {
    let url = "https://circleci.com/api/v2/context";
    let body = CreateContextBody {
        name: name.to_string(),
        owner: ContextOwnerBody {
            id: vcs.org_id().to_string(),
        },
    };

    let ctx = send_post_request(url, Some(body)).await?;
    Ok(ctx)
}

pub async fn add_context_variable(
    context_id: &str,
    name: &str,
    value: &str,
) -> Result<ContextVariable, anyhow::Error> {
    let url = format!(
        "https://circleci.com/api/v2/context/{context_id}/environment-variable/{env_var_name}",
        context_id = context_id,
        env_var_name = name
    );
    let body = UpdateContextVariableBody {
        value: value.to_string(),
    };

    let var = send_put_request(url, Some(body)).await?;
    Ok(var)
}

async fn send_get_request<T: DeserializeOwned, U: IntoUrl>(
    url: U,
) -> Result<T, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .get(url)
        .header(AUTH_HEADER, &CONFIG.circleci.token)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(res)
}

async fn send_post_request<T: DeserializeOwned, U: IntoUrl, B: Serialize>(
    url: U,
    body: Option<B>,
) -> Result<T, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .post(url)
        .header(AUTH_HEADER, &CONFIG.circleci.token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(res)
}

async fn send_put_request<T: DeserializeOwned, U: IntoUrl, B: Serialize>(
    url: U,
    body: Option<B>,
) -> Result<T, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .put(url)
        .header(AUTH_HEADER, &CONFIG.circleci.token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<T>()
        .await?;

    Ok(res)
}
