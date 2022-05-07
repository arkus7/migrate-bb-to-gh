mod models;

use crate::circleci::api::models::{
    ContextOwnerBody, CreateContextBody, ExportEnvironmentBody, PageResponse, StartPipelineBody,
    UpdateContextVariableBody,
};
use crate::config::CircleCiConfig;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::api::{ApiClient, BasicAuth};
pub(crate) use models::{Context, ContextVariable, EnvVar};

const AUTH_HEADER: &str = "circle-token";

pub(crate) enum VCSProvider {
    Bitbucket,
    GitHub,
}

impl VCSProvider {
    const fn slug_prefix(&self) -> &str {
        match self {
            VCSProvider::Bitbucket => "bitbucket",
            VCSProvider::GitHub => "gh",
        }
    }
}

pub(crate) struct CircleCiApi {
    config: CircleCiConfig,
}

impl ApiClient for CircleCiApi {
    fn basic_auth(&self) -> Option<BasicAuth> {
        None
    }

    fn headers(&self) -> Option<HeaderMap> {
        let mut headers = HeaderMap::new();

        let header_name = HeaderName::from_static(AUTH_HEADER);
        let token_value = HeaderValue::from_str(&self.config.token).unwrap();

        headers.insert(header_name, token_value);

        Some(headers)
    }
}

impl CircleCiApi {
    pub fn new(config: &CircleCiConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub async fn get_env_vars(
        &self,
        vcs: VCSProvider,
        full_repo_name: &str,
    ) -> anyhow::Result<Vec<EnvVar>> {
        let project_slug = format!("{}/{}", vcs.slug_prefix(), full_repo_name);
        let url = format!(
            "https://circleci.com/api/v2/project/{project_slug}/envvar",
            project_slug = project_slug,
        );

        let res: Result<PageResponse<EnvVar>, reqwest::Error> = self.get(url).await;
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

    pub async fn get_contexts(&self, vcs: VCSProvider) -> anyhow::Result<Vec<Context>> {
        let url = format!(
            "https://circleci.com/api/v2/context?owner-id={org_id}",
            org_id = self.org_id(vcs)
        );

        let res: PageResponse<Context> = self.get(url).await?;

        Ok(res.items)
    }

    pub async fn get_context_variables(
        &self,
        context_id: &str,
    ) -> anyhow::Result<Vec<ContextVariable>> {
        let url = format!(
            "https://circleci.com/api/v2/context/{context_id}/environment-variable",
            context_id = context_id
        );

        let res: PageResponse<ContextVariable> = self.get(url).await?;

        Ok(res.items)
    }

    pub async fn export_environment(
        &self,
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

        // We try to export environment multiple times as sometimes the response status code
        // is successful, but there are no env vars moved to the new project.
        //
        // Usually, 2 requests suffice, but `MAX_ATTEMPTS` is set to a greater value just in case.

        let mut variables = vec![];
        let mut attempts_made = 0;

        const MAX_ATTEMPTS: u8 = 5;

        while variables.len() < env_vars.len() && attempts_made < MAX_ATTEMPTS {
            let _: serde_json::Value = self.post(&url, Some(&body)).await?;
            variables = self.get_env_vars(VCSProvider::GitHub, to_repo_name).await?;
            attempts_made += 1;
        }

        Ok(())
    }

    pub async fn start_pipeline(&self, repo_name: &str, branch: &str) -> Result<(), anyhow::Error> {
        let url = format!(
            "https://circleci.com/api/v1.1/project/gh/{repo_name}/follow",
            repo_name = repo_name
        );
        let body = StartPipelineBody { branch };

        let _: serde_json::Value = self.post(url, Some(body)).await?;

        Ok(())
    }

    pub async fn create_context(
        &self,
        name: &str,
        vcs: VCSProvider,
    ) -> Result<Context, anyhow::Error> {
        let url = "https://circleci.com/api/v2/context";
        let body = CreateContextBody {
            name: name.to_string(),
            owner: ContextOwnerBody {
                id: self.org_id(vcs).to_string(),
            },
        };

        let ctx = self.post(url, Some(body)).await?;
        Ok(ctx)
    }

    pub async fn add_context_variable(
        &self,
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

        let var = self.put(url, Some(body)).await?;
        Ok(var)
    }

    fn org_id(&self, provider: VCSProvider) -> &str {
        match provider {
            VCSProvider::Bitbucket => &self.config.bitbucket_org_id,
            VCSProvider::GitHub => &self.config.github_org_id,
        }
    }
}
