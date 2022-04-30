use dialoguer::theme::ColorfulTheme;

mod fuzzy_select;
mod multi_select;

pub use fuzzy_select::FuzzySelect;
pub use multi_select::MultiSelect;

fn default_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}
