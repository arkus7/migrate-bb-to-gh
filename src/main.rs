use std::path::PathBuf;

mod bitbucket;
mod github;
mod migrator;
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
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Wizard { output } => {
            let wizard = Wizard::new(output.clone());
            let res = wizard.run().await?;

            println!(
                "Migration file saved to {:?}",
                std::fs::canonicalize(&res.migration_file_path)?
            );
            println!("{}", migrator::describe_actions(&res.actions));
        }
        Commands::Migrate { migration_file } => {
            migrator::migrate(migration_file).await?;
        }
    }

    Ok(())
}
