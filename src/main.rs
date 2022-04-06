use dialoguer::{theme::ColorfulTheme, FuzzySelect, MultiSelect};
use indicatif::ProgressBar;

mod bitbucket;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let theme = ColorfulTheme::default();
    let spinner = create_spinner("Fetching projects from Bitbucket...");
    let projects = bitbucket::get_projects().await?;

    spinner.finish_with_message("Fetched!");

    let selection = FuzzySelect::with_theme(&theme)
    .with_prompt("Select project")
    .items(&projects)
    .interact()
    .unwrap();

    let project = projects.get(selection).expect("No project selected");

    spinner.set_message(format!("Fetching repositories from {} project", project));
    let repositories = bitbucket::get_repositories(project.get_key()).await?;
    spinner.finish_with_message("Fetched!");

    let selection = MultiSelect::with_theme(&theme)
    .with_prompt(format!("Select repositories from {} project", project))
    .items(&repositories)
    .interact()
    .unwrap();

    let repositories = selection.iter().flat_map(|&idx| repositories.get(idx)).collect::<Vec<_>>();

    dbg!(&repositories);

    Ok(())
}

fn create_spinner(message: &'static str) -> ProgressBar {
  let pb = ProgressBar::new_spinner();
  pb.enable_steady_tick(120);

  pb.set_message(message);

  pb
}
