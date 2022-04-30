use crate::prompts::default_theme;
use std::fmt::Display;

pub struct MultiSelect<'a, T> {
    items: Vec<&'a T>,
    prompt: String,
}

impl<'a, T> MultiSelect<'a, T>
where
    T: 'a + Display,
{
    pub fn with_prompt(prompt: &str) -> Self {
        Self {
            items: vec![],
            prompt: prompt.into(),
        }
    }

    pub fn items(&mut self, items: &'a [&'a T]) -> &mut Self {
        for item in items {
            self.items.push(item);
        }
        self
    }

    pub fn interact(&self) -> std::io::Result<Vec<&'a T>> {
        use dialoguer::MultiSelect;

        let indices = MultiSelect::with_theme(&default_theme())
            .with_prompt(format!(
                "{prompt}\n{tip}",
                prompt = &self.prompt,
                tip = prompt_tip()
            ))
            .items(&self.items)
            .interact()?;

        let selected = indices
            .into_iter()
            .flat_map(|idx| self.items.get(idx).copied())
            .collect();

        Ok(selected)
    }
}

fn prompt_tip() -> &'static str {
    "[Space = select, Enter = continue]"
}
