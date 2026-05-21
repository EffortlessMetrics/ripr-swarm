use crate::app::CheckInput;
use crate::config::{CheckInputExplicit, RiprConfig, apply_to_check_input, load_for_root};
use std::path::Path;

pub(super) fn require_root_dir(command: &str, root: &Path) -> Result<(), String> {
    if root.is_dir() {
        return Ok(());
    }
    Err(format!(
        "{command} root {} is not a directory",
        root.display()
    ))
}

pub(super) fn load_configured_check_input(root: &Path) -> Result<(RiprConfig, CheckInput), String> {
    let config = load_for_root(root)?;
    let mut input = CheckInput {
        root: root.to_path_buf(),
        ..CheckInput::default()
    };
    apply_to_check_input(&mut input, &config, CheckInputExplicit::default());
    Ok((config, input))
}
