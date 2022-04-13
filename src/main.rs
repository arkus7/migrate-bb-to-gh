use std::path::PathBuf;

mod bitbucket;
mod circleci;
mod github;
mod migrator;
mod spinner;
mod wizard;

use clap::{Parser, Subcommand, CommandFactory};

use crate::wizard::Wizard;

const BIN_NAME: &str = "migrate-bb-to-gh";

/// Utility tool for migration of repositories from Bitbucket to GitHub for Mood Up Team
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Guides you through migration process, generating migration file for "migrate" subcommand
    Wizard {
        #[clap(
            short,
            long,
            parse(from_os_str),
            value_name = "OUTPUT_FILE",
            default_value = "migration.json"
        )]
        output: PathBuf,
    },
    /// Migrates repositories from Bitbucket to GitHub, following the actions defined in migration file
    Migrate {
        /// Path to migration file
        #[clap(parse(from_os_str), value_name = "MIGRATION_FILE")]
        migration_file: PathBuf,
    },
    /// Tool for migrating CircleCI configuration
    #[clap(name = "circleci")]
    CircleCi {
        #[clap(subcommand)]
        command: CircleCiCommands,
    },
}

#[derive(Subcommand)]
enum CircleCiCommands {
    /// Guides you through migration process, generating migration file for "migrate" subcommand
    Wizard {
        #[clap(
            short,
            long,
            parse(from_os_str),
            value_name = "OUTPUT_FILE",
            default_value = "ci-migration.json",
            value_hint = clap::ValueHint::FilePath
        )]
        output: PathBuf,
    },
    /// Migrates CircleCI configuration to GitHub organization on CircleCI
    Migrate {
        /// Path to migration file
        #[clap(parse(from_os_str), value_name = "MIGRATION_FILE")]
        migration_file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let cmd = Cli::command();
    let version = cmd.get_version().unwrap();
    let name = cmd.get_name();

    match &cli.command {
        Commands::Wizard { output } => {
            let wizard = Wizard::new(output.clone(), version);
            let res = wizard.run().await?;

            println!(
                "Migration file saved to {:?}",
                std::fs::canonicalize(&res.migration_file_path)?
            );
            println!("{}", migrator::describe_actions(&res.actions));
            println!(
                "Run '{} migrate {}' to start migration process",
                name,
                output.display()
            );
        }
        Commands::Migrate { migration_file } => {
            migrator::migrate(migration_file, version).await?;
        }
        Commands::CircleCi { command } => match &command {
            CircleCiCommands::Wizard { output } => {
                let res = circleci::wizard::Wizard::new(output, version).run().await?;
                println!(
                    "Migration file saved to {:?}",
                    std::fs::canonicalize(&res.migration_file_path)?
                );
                println!("{}", circleci::migrate::describe_actions(&res.actions));
                println!(
                    "Run '{} circleci migrate {}' to start migration process",
                    name,
                    output.display()
                );
            }
            CircleCiCommands::Migrate { migration_file } => {
                circleci::migrate::migrate(migration_file, version).await?;
            }
        },
    }

    Ok(())
}
