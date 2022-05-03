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

    pub async fn run(&self) -> anyhow::Result<()> {
        match self {
            Action::CreateContext { name, variables } => {
                let spinner = spinner::create_spinner(format!("Creating '{}' context", name));
                let ctx = api::create_context(name, api::Vcs::GitHub).await?;
                spinner.finish_with_message(format!(
                    "Created context '{}' (id: {})",
                    &ctx.name, &ctx.id
                ));

                for var in variables {
                    let spinner = spinner::create_spinner(format!(
                        "Adding '{}' variable to '{}' context",
                        &var.name, &name
                    ));
                    let _ = api::add_context_variable(&ctx.id, &var.name, &var.value).await?;
                    spinner.finish_with_message(format!("Added '{}' variable", &var.name));
                }

                Ok(())
            }
            Action::MoveEnvironmentalVariables {
                from_repository_name,
                to_repository_name,
                env_vars,
            } => {
                let spinner = spinner::create_spinner(format!("Moving {} environmental variables from '{}' project on Bitbucket to '{}' project on Github", env_vars.len(), &from_repository_name, &to_repository_name));
                let _ =
                    api::export_environment(from_repository_name, to_repository_name, env_vars)
                        .await?;
                spinner.finish_with_message(format!("Moved {} environmental variables from '{}' project on Bitbucket to '{}' project on Github", env_vars.len(), &from_repository_name, &to_repository_name));
                Ok(())
            }
            Action::StartPipeline {
                repository_name,
                branch,
            } => {
                let spinner = spinner::create_spinner(format!(
                    "Starting pipeline for {} on branch {}",
                    &repository_name, &branch
                ));
                let _ = api::start_pipeline(repository_name, branch).await?;
                spinner.finish_with_message(format!(
                    "Started pipeline for {} on branch {}",
                    &repository_name, &branch
                ));
                Ok(())
            }
        }
    }
}
