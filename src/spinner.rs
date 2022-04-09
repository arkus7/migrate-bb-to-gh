use std::{borrow::Cow, time::Duration};

use indicatif::ProgressBar;

pub fn create_spinner<T: Into<Cow<'static, str>>>(message: T) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(120));

    pb.set_message(message);

    pb
}
