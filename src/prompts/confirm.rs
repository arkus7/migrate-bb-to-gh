use crate::prompts::default_theme;
use std::io;

pub struct Confirm {
    prompt: String,
}

impl Confirm {
    pub fn with_prompt(prompt: &str) -> Self {
        Self {
            prompt: prompt.into(),
        }
    }

    pub fn interact(&self) -> io::Result<bool> {
        use dialoguer::Confirm;

        Confirm::with_theme(&default_theme())
            .with_prompt(&self.prompt)
            .interact()
    }
}
