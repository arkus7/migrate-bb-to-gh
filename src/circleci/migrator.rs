use anyhow::{anyhow, Context, Error};
use std::path::PathBuf;
use std::time::Instant;
use std::{fs::File, path::Path};

use crate::circleci::action::{describe_actions, Action, EnvVar};
use crate::circleci::api;
use crate::circleci::api::CircleCiApi;
use crate::config::CircleCiConfig;
use crate::prompts::Confirm;
use crate::spinner;
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
    pub fn new(migration_file: &Path, version: &str, circleci_cfg: CircleCiConfig) -> Self {
        Self {
            migration_file: migration_file.to_path_buf(),
            version: version.to_owned(),
            circleci: CircleCiApi::new(&circleci_cfg),
        }
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        let migration = self.parse_migration_file()?;

        let actions = migration.actions;
        println!("{}", describe_actions(&actions));

        let confirmed = Confirm::with_prompt("Are you sure you want to migrate?").interact()?;

        if !confirmed {
            return Err(anyhow!("Migration canceled"));
        }

        let start = Instant::now();

        for action in actions {
            let _ = self.run(&action).await?;
        }

        let duration = start.elapsed();
        println!("Migration completed in {} seconds!", duration.as_secs());

        Ok(())
    }

    fn parse_migration_file(&self) -> Result<Migration, Error> {
        let file = File::open(&self.migration_file)?;
        let migration: Migration = serde_json::from_reader(file).with_context(|| format!("Error when parsing {} file.\nIs this a JSON file?\nDoes the version match the program version ({})?\nConsider re-generating the migration file with `wizard` subcommand.", self.migration_file.display(), self.version))?;
        if migration.version != self.version {
            return Err(anyhow!("Migration file version is not compatible with current version, expected: {}, found: {}", self.version, migration.version));
        }
        Ok(migration)
    }

    pub async fn run(&self, action: &Action) -> anyhow::Result<()> {
        match action {
            Action::CreateContext { name, variables } => self.create_context(name, variables).await,
            Action::MoveEnvironmentalVariables {
                from_repository_name,
                to_repository_name,
                env_vars,
            } => {
                self.export_env_variables(from_repository_name, to_repository_name, env_vars)
                    .await
            }
            Action::StartPipeline {
                repository_name,
                branch,
            } => self.start_pipeline(repository_name, branch).await,
        }
    }

    async fn start_pipeline(&self, repository_name: &str, branch: &str) -> Result<(), Error> {
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

    async fn export_env_variables(
        &self,
        from_repository_name: &str,
        to_repository_name: &str,
        env_vars: &[String],
    ) -> Result<(), Error> {
        let spinner = spinner::create_spinner(format!("Moving {} environmental variables from '{}' project on Bitbucket to '{}' project on Github", env_vars.len(), &from_repository_name, &to_repository_name));
        let _ = self
            .circleci
            .export_environment(from_repository_name, to_repository_name, env_vars)
            .await?;
        spinner.finish_with_message(format!("Moved {} environmental variables from '{}' project on Bitbucket to '{}' project on Github", env_vars.len(), &from_repository_name, &to_repository_name));
        Ok(())
    }

    async fn create_context(&self, name: &str, variables: &[EnvVar]) -> Result<(), Error> {
        let spinner = spinner::create_spinner(format!("Creating '{}' context", name));
        let ctx = self
            .circleci
            .create_context(name, api::VCSProvider::GitHub)
            .await?;
        spinner.finish_with_message(format!("Created context '{}' (id: {})", &ctx.name, &ctx.id));

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
}
