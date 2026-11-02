//! End-to-end public error boundary for the check vertical (#1986).

use ripr::{CheckInput, RiprError, RiprErrorCategory, try_check_workspace_with_config};
use std::path::PathBuf;

fn workspace_input(base: Option<&str>) -> CheckInput {
    CheckInput {
        root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        base: base.map(str::to_string),
        ..CheckInput::default()
    }
}

fn missing_base_error() -> Result<RiprError, String> {
    match try_check_workspace_with_config(
        workspace_input(Some("refs/heads/ripr-1986-missing-base")),
        &ripr::config::RiprConfig::default(),
    ) {
        Err(error) => Ok(error),
        Ok(_) => Err("missing base must fail".into()),
    }
}

#[test]
fn unresolvable_base_is_repository_or_git_without_parsing_display() -> Result<(), String> {
    let error = missing_base_error()?;
    assert_eq!(error.category(), RiprErrorCategory::RepositoryOrGit);
    assert_eq!(error.code(), "base_unresolvable");
    assert!(error.is_user_or_operational());
    assert_ne!(error.to_string(), error.code());
    Ok(())
}

#[test]
fn typed_and_string_check_apis_share_human_output() -> Result<(), String> {
    let input = workspace_input(Some("refs/heads/ripr-1986-missing-base"));
    let config = ripr::config::RiprConfig::default();
    let typed = match try_check_workspace_with_config(input.clone(), &config) {
        Err(error) => error,
        Ok(_) => return Err("typed check must fail".into()),
    };
    let string = match ripr::check_workspace_with_config(input, &config) {
        Err(error) => error,
        Ok(_) => return Err("string check must fail".into()),
    };
    assert_eq!(typed.to_string(), string);
    Ok(())
}

#[test]
fn string_check_api_reports_failure_as_string() -> Result<(), String> {
    let message = match ripr::check_workspace_with_config(
        workspace_input(Some("refs/heads/ripr-1986-missing-base")),
        &ripr::config::RiprConfig::default(),
    ) {
        Err(error) => error,
        Ok(_) => return Err("expected string error".into()),
    };
    assert!(message.contains("does not resolve") || message.contains("resolve"));
    Ok(())
}

#[test]
fn unknown_category_arm_compiles_for_forward_compat() {
    fn classify(category: RiprErrorCategory) -> &'static str {
        match category {
            RiprErrorCategory::Configuration => "configuration",
            _ => "future",
        }
    }
    assert_eq!(classify(RiprErrorCategory::Analysis), "future");
}

#[test]
fn unknown_error_variant_arm_compiles_for_forward_compat() -> Result<(), String> {
    fn code(error: &RiprError) -> &str {
        match error {
            RiprError::CheckFailure { code, .. } => code,
            _ => "other",
        }
    }
    let error = missing_base_error()?;
    assert_eq!(code(&error), "base_unresolvable");
    Ok(())
}
