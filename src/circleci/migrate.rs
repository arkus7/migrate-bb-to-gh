use anyhow::{anyhow, Context};
use std::{fs::File, path::Path};

use dialoguer::Confirm;
use serde::{Deserialize, Serialize};
use crate::circleci::action::Action;
use crate::circleci::api;
use crate::circleci::api::CircleCiApi;
use crate::config::CONFIG;
use crate::spinner;

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

pub async fn migrate(migration_file: &Path, version: &str) -> anyhow::Result<()> {
    let file = File::open(migration_file)?;
    let migration: Migration = serde_json::from_reader(file).with_context(|| format!("Error when parsing {:?} file.\nIs this a JSON file?\nDoes the version match the program version ({})?\nConsider re-generating the migration file with `wizard` subcommand.", migration_file, version))?;
    if migration.version != version {
        return Err(anyhow!("Migration file version is not compatible with current version, expected: {}, found: {}", version, migration.version));
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
        let _ = run(&action).await?;
    }

    Ok(())
}

pub fn describe_actions(actions: &[Action]) -> String {
    let actions_list = actions
        .iter()
        .enumerate()
        .map(|(idx, action)| format!("{}. {}", idx + 1, action.describe()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "There are {} actions to be done during migration:\n{}",
        actions.len(),
        actions_list
    )
}
pub async fn run(action: &Action) -> anyhow::Result<()> {
    // FIXME: store circleci client inside migrator struct
    let circleci = CircleCiApi::new(&CONFIG.circleci);
    match action {
        Action::CreateContext { name, variables } => {
            let spinner = spinner::create_spinner(format!("Creating '{}' context", name));
            let ctx = circleci.create_context(name, api::Vcs::GitHub).await?;
            spinner.finish_with_message(format!(
                "Created context '{}' (id: {})",
                &ctx.name, &ctx.id
            ));

            for var in variables {
                let spinner = spinner::create_spinner(format!(
                    "Adding '{}' variable to '{}' context",
                    &var.name, &name
                ));
                let _ = circleci.add_context_variable(&ctx.id, &var.name, &var.value).await?;
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
                circleci.export_environment(from_repository_name, to_repository_name, env_vars)
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
            let _ = circleci.start_pipeline(repository_name, branch).await?;
            spinner.finish_with_message(format!(
                "Started pipeline for {} on branch {}",
                &repository_name, &branch
            ));
            Ok(())
        }
    }
}
