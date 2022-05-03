use crate::circleci::api;
use crate::spinner;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    MoveEnvironmentalVariables {
        from_repository_name: String,
        to_repository_name: String,
        env_vars: Vec<String>,
    },
    CreateContext {
        name: String,
        variables: Vec<EnvVar>,
    },
    StartPipeline {
        repository_name: String,
        branch: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl Action {
    pub fn describe(&self) -> String {
        match self {
            Action::MoveEnvironmentalVariables {
                from_repository_name,
                to_repository_name,
                env_vars,
            } => format!(
                "Move environmental variables from '{}' project in Bitbucket to '{}' project Github\n  Envs: {}",
                from_repository_name,
                to_repository_name,
                env_vars.join(", ")
            ),
            Action::CreateContext { name, variables } => format!(
                "Create context named '{}' with {} variables:\n{}",
                name,
                variables.len(),
                variables
                    .iter()
                    .map(|e| format!("  {}={}", e.name, e.value))
                    .collect::<Vec<_>>()
                    .join(",\n"),
            ),
            Action::StartPipeline { repository_name, branch } => format!(
                "Start pipeline for {} on branch {}",
                repository_name,
                branch,
            ),
        }
    }
}
