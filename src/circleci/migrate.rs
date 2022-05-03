use anyhow::{anyhow, Context};
use std::{fs::File, path::Path};

use dialoguer::Confirm;
use serde::{Deserialize, Serialize};
use crate::circleci::action::Action;

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
        let _ = action.run().await?;
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