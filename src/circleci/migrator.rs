use anyhow::{anyhow, Context};
use std::path::PathBuf;
use std::{fs::File, path::Path};

use crate::circleci::action::{describe_actions, Action};
use crate::circleci::api;
use crate::circleci::api::CircleCiApi;
use crate::config::CONFIG;
use crate::spinner;
use dialoguer::Confirm;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Migration {
    version: String,
    actions: Vec<Action>,
}

impl Migration {
    pub fn new(version: &str, actions: &[Action]) -> Self {
        Self {
            version: version.to_string(),
            actions: actions.to_vec(),
        }
    }
}

pub struct Migrator {
    migration_file: PathBuf,
    version: String,
    circleci: CircleCiApi,
}

impl Migrator {
    pub fn new(migration_file: &Path, version: &str) -> Self {
        Self {
            migration_file: migration_file.to_path_buf(),
            version: version.to_owned(),
            circleci: CircleCiApi::new(&CONFIG.circleci),
        }
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        let file = File::open(&self.migration_file)?;
        let migration: Migration = serde_json::from_reader(file).with_context(|| format!("Error when parsing {:?} file.\nIs this a JSON file?\nDoes the version match the program version ({})?\nConsider re-generating the migration file with `wizard` subcommand.", self.migration_file, self.version))?;
        if migration.version != self.version {
            return Err(anyhow!("Migration file version is not compatible with current version, expected: {}, found: {}", self.version, migration.version));
        }
        let actions = migration.actions;

        println!("{}", describe_actions(&actions));

        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to migrate?")
            .interact()?;

        if !confirmed {
            return Err(anyhow!("Migration canceled"));
        }

        for action in actions {
            let _ = self.run(&action).await?;
        }

        Ok(())
    }

    pub async fn run(&self, action: &Action) -> anyhow::Result<()> {
        match action {
            Action::CreateContext { name, variables } => {
                let spinner = spinner::create_spinner(format!("Creating '{}' context", name));
                let ctx = self.circleci.create_context(name, api::Vcs::GitHub).await?;
                spinner.finish_with_message(format!(
                    "Created context '{}' (id: {})",
                    &ctx.name, &ctx.id
                ));

                for var in variables {
                    let spinner = spinner::create_spinner(format!(
                        "Adding '{}' variable to '{}' context",
                        &var.name, &name
                    ));
                    let _ = self
                        .circleci
                        .add_context_variable(&ctx.id, &var.name, &var.value)
                        .await?;
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
                let _ = self
                    .circleci
                    .export_environment(from_repository_name, to_repository_name, env_vars)
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
                let _ = self
                    .circleci
                    .start_pipeline(repository_name, branch)
                    .await?;
                spinner.finish_with_message(format!(
                    "Started pipeline for {} on branch {}",
                    &repository_name, &branch
                ));
                Ok(())
            }
        }
    }
}
