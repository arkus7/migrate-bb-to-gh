use dialoguer::theme::ColorfulTheme;

mod confirm;
mod fuzzy_select;
mod multi_select;

pub use fuzzy_select::FuzzySelect;
pub use multi_select::MultiSelect;
pub use confirm::Confirm;

fn default_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}
