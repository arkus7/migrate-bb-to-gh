use dialoguer::theme::ColorfulTheme;

mod confirm;
mod fuzzy_select;
mod multi_select;
mod input;

pub use fuzzy_select::FuzzySelect;
pub use multi_select::MultiSelect;
pub use confirm::Confirm;
pub use input::Input;

fn default_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}
