use super::default_theme;
use dialoguer::FuzzySelect as InnerFuzzySelect;
use std::fmt::Display;
use std::io;

pub struct FuzzySelect<'a, T>
where
    T: Display,
{
    items: Vec<&'a T>,
    prompt: String,
    default: usize,
}

impl<'a, T> FuzzySelect<'a, T>
where
    T: 'a + Display,
{
    pub fn with_prompt(prompt: &str) -> Self {
        Self {
            items: vec![],
            prompt: prompt.into(),
            default: 0,
        }
    }

    pub fn items(&mut self, items: &[&'a T]) -> &mut Self {
        for item in items {
            self.items.push(item);
        }
        self
    }

    pub fn default(&mut self, default: usize) -> &mut Self {
        self.default = default;
        self
    }

    pub fn interact(&self) -> io::Result<&'a T> {
        let selected = self
            .interact_opt()?
            .expect("At least 1 item must be selected");

        Ok(selected)
    }

    pub fn interact_opt(&self) -> io::Result<Option<&'a T>> {
        let idx = InnerFuzzySelect::with_theme(&default_theme())
            .with_prompt(format!(
                "{prompt}\n{tip}",
                prompt = &self.prompt,
                tip = prompt_tip()
            ))
            .items(&self.items)
            .default(self.default)
            .interact()?;

        let selected: Option<&'a T> = self.items.get(idx).copied();

        Ok(selected)
    }
}

fn prompt_tip() -> &'static str {
    "[You can fuzzy search here by typing]"
}