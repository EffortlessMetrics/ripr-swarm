use crate::app::CheckInput;
use crate::config::{CheckInputExplicit, RiprConfig, apply_to_check_input, load_for_root};
use std::path::Path;

pub(super) fn ensure_root_directory(command: &str, root: &Path) -> Result<(), String> {
    if root.is_dir() {
        Ok(())
    } else {
        Err(format!(
            "{command} root {} is not a directory",
            root.display()
        ))
    }
}

pub(super) fn build_check_input_with_config(
    root: &Path,
) -> Result<(CheckInput, RiprConfig), String> {
    let config = load_for_root(root)?;
    let mut input = CheckInput {
        root: root.to_path_buf(),
        ..CheckInput::default()
    };
    apply_to_check_input(&mut input, &config, CheckInputExplicit::default());
    Ok((input, config))
}
