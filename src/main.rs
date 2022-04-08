use std::{path::PathBuf};

use bitbucket::Repository;
use github::TeamRepositoryPermission;

use serde::{Deserialize, Serialize};

mod bitbucket;
mod github;
mod spinner;
mod wizard;

use clap::{Parser, Subcommand};

use crate::wizard::Wizard;

#[derive(Parser)]
#[clap(author, version, about)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Wizard {
        #[clap(short, long, parse(from_os_str), value_name = "OUTPUT_FILE")]
        output: Option<PathBuf>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Action {
    MigrateRepositories {
        repositories: Vec<Repository>,
    },
    CreateTeam {
        name: String,
        repositories: Vec<String>,
    },
    AssignRepositoriesToTeam {
        team_name: String,
        team_id: u32,
        permission: TeamRepositoryPermission,
        repositories: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Wizard { output } => {
            let output_path = output.clone().unwrap_or_else(|| {
                let mut path = PathBuf::from("./migration.json");
                path.set_extension("json");
                path
            });

            let wizard = Wizard::new(output_path);
            wizard.run().await?;
        }
    }

    Ok(())
}
