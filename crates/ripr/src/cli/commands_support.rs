use crate::app::CheckInput;
use crate::config::{CheckInputExplicit, RiprConfig, apply_to_check_input, load_for_root};
use std::path::Path;

pub(crate) fn require_root_dir(root: &Path, noun: &str) -> Result<(), String> {
    if root.is_dir() {
        Ok(())
    } else {
        Err(format!("{noun} root {} is not a directory", root.display()))
    }
}

pub(crate) fn configured_check_input(
    root: &Path,
    explicit: CheckInputExplicit,
) -> Result<(RiprConfig, CheckInput), String> {
    let config = load_for_root(root)?;
    let mut input = CheckInput {
        root: root.to_path_buf(),
        ..CheckInput::default()
    };
    apply_to_check_input(&mut input, &config, explicit);
    Ok((config, input))
}
