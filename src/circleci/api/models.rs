use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct PageResponse<T> {
    pub(crate) items: Vec<T>,
    pub(crate) next_page_token: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct EnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Context {
    pub name: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ContextVariable {
    pub variable: String,
    pub context_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ExportEnvironmentBody {
    /// List of URLs to the projects where env variables should be exported to
    pub(crate) projects: Vec<String>,
    #[serde(rename = "env-vars")]
    pub(crate) env_vars: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct StartPipelineBody<'a> {
    pub(crate) branch: &'a str,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct FollowProjectBody<'a> {
    pub(crate) branch: &'a str,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct FollowProjectResponse {
    pub(crate) first_build: Option<bool>,
    pub(crate) following: bool,
    pub(crate) workflow: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct CreateContextBody {
    pub(crate) name: String,
    pub(crate) owner: ContextOwnerBody,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct ContextOwnerBody {
    pub(crate) id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct UpdateContextVariableBody {
    pub(crate) value: String,
}
