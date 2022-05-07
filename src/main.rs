use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use migrate_bb_to_gh::circleci;
use migrate_bb_to_gh::config;
use migrate_bb_to_gh::repositories::{self, Migrator, Wizard};

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

    let config = config::parse_config()?;

    match &cli.command {
        Commands::Wizard { output } => {
            let wizard = Wizard::new(output.clone(), version, config.bitbucket, config.github);
            let res = wizard.run().await?;

            println!(
                "Migration file saved to {:?}",
                std::fs::canonicalize(&res.migration_file_path)?
            );
            println!("{}", repositories::describe_actions(&res.actions));
            println!(
                "Run '{} migrate {}' to start migration process",
                name,
                output.display()
            );
        }
        Commands::Migrate { migration_file } => {
            let migrator = Migrator::new(migration_file, version, config);
            let _ = migrator.migrate().await?;
        }
        Commands::CircleCi { command } => match &command {
            CircleCiCommands::Wizard { output } => {
                let res = circleci::Wizard::new(output, version, config).run().await?;
                println!(
                    "Migration file saved to {:?}",
                    std::fs::canonicalize(&res.migration_file_path)?
                );
                println!("{}", circleci::describe_actions(&res.actions));
                println!(
                    "Run '{} circleci migrate {}' to start migration process",
                    name,
                    output.display()
                );
            }
            CircleCiCommands::Migrate { migration_file } => {
                let migrator = circleci::Migrator::new(migration_file, version, config.circleci);
                let _ = migrator.migrate().await?;
            }
        },
    }

    Ok(())
}
