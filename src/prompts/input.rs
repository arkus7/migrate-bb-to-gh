use crate::prompts::default_theme;
use std::io;

type InputValidator = Box<dyn Fn(&str) -> Option<String>>;

///
/// # Example
///
/// ## Validation
///
/// ```rust,no_run
/// use migrate_bb_to_gh::prompts::Input;
///
/// let email: String = Input::with_prompt("Provide your e-mail address")
///     .validate_with(|mail: &str| if !mail.contains('@') { Some("invalid email".to_string()) } else { None })
///     .interact()
///     .expect("could not interact");
/// ```
/// ## Initial value
/// ```rust,no_run
/// use migrate_bb_to_gh::prompts::Input;
///
/// let email: String = Input::with_prompt("What's your favorite color?")
///     .initial_text("Red")
///     .interact()
///     .expect("could not interact");
/// ```
pub struct Input {
    prompt: String,
    initial_text: String,
    validator: Option<InputValidator>,
}

impl Input {
    pub fn with_prompt<S: Into<String>>(prompt: S) -> Self {
        Self {
            prompt: prompt.into(),
            initial_text: "".into(),
            validator: None,
        }
    }

    pub fn initial_text(&mut self, initial_text: &str) -> &mut Self {
        self.initial_text = initial_text.into();
        self
    }

    pub fn validate_with<F>(&mut self, validator: F) -> &mut Self
    where
        F: 'static + Fn(&str) -> Option<String>,
    {
        self.validator = Some(Box::new(validator));
        self
    }

    pub fn interact(&self) -> io::Result<String> {
        use dialoguer::Input;

        let theme = default_theme();
        loop {
            let input: String = Input::with_theme(&theme)
                .with_prompt(&self.prompt)
                .with_initial_text(&self.initial_text)
                .interact()?;

            if let Some(validator) = &self.validator {
                let err = validator(&input);
                match err {
                    None => return Ok(input),
                    Some(e) => {
                        eprintln!("Error: {}", e);
                        continue;
                    }
                }
            } else {
                return Ok(input);
            }
        }
    }
}
