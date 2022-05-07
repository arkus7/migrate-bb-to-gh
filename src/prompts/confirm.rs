use crate::prompts::default_theme;
use std::io;

pub struct Confirm {
    prompt: String,
    default: bool,
}

impl Confirm {
    pub fn with_prompt<S: Into<String>>(prompt: S) -> Self {
        Self {
            prompt: prompt.into(),
            default: false,
        }
    }

    pub fn default(&mut self, default: bool) -> &mut Self {
        self.default = default;
        self
    }

    pub fn interact(&self) -> io::Result<bool> {
        use dialoguer::Confirm;

        Confirm::with_theme(&default_theme())
            .with_prompt(&self.prompt)
            .default(self.default)
            .interact()
    }
}
