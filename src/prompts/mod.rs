use dialoguer::theme::ColorfulTheme;

mod fuzzy_select;

pub use fuzzy_select::FuzzySelect;

fn default_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}
