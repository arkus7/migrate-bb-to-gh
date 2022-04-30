use dialoguer::theme::ColorfulTheme;

mod confirm;
mod fuzzy_select;
mod multi_select;
mod input;
mod select;

pub use fuzzy_select::FuzzySelect;
pub use multi_select::MultiSelect;
pub use confirm::Confirm;
pub use input::Input;
pub use select::Select;

fn default_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}
