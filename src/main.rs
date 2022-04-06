use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use indicatif::ProgressBar;

mod bitbucket;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let spinner = create_spinner("Fetching projects from Bitbucket...");
    let projects = bitbucket::get_projects().await?;
    // dbg!(&projects);
    spinner.finish_with_message("Fetched!");

    let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
    .with_prompt("Select project")
    .items(&projects)
    .interact()
    .unwrap();

    println!("Selected project: {:?}", projects[selection]);

    Ok(())
}

fn create_spinner(message: &'static str) -> ProgressBar {
  let pb = ProgressBar::new_spinner();
  pb.enable_steady_tick(120);

  pb.set_message(message);

  pb
}
