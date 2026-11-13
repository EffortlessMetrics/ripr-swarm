//! Compatibility facade for the shared product-independent user-text matcher.

fn changed_paths() -> Result<Vec<String>, String> {
    let mut changed = crate::changed_files_vs_base(std::path::Path::new("."))?;
    changed.extend(crate::git_changed_files(std::path::Path::new("."), "HEAD")?);
    Ok(changed)
}

pub(crate) fn check_user_text_full() -> Result<(), String> {
    check_user_text_with_args(&["--full".to_string()])
}

pub(crate) fn check_user_text() -> Result<(), String> {
    check_user_text_with_args(&[])
}

pub(crate) fn check_user_text_with_args(args: &[String]) -> Result<(), String> {
    repo_policy::user_text::check_user_text_with_args(args, changed_paths)
}
