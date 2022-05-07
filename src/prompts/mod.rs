use dialoguer::theme::ColorfulTheme;

mod confirm;
mod fuzzy_select;
mod input;
mod multi_select;
mod select;

pub use confirm::Confirm;
pub use fuzzy_select::FuzzySelect;
pub use input::Input;
pub use multi_select::MultiSelect;
pub use select::Select;

fn default_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}
