//! #9008: a conflicting repeated `--root` is a usage error (exit 2) naming
//! both values, while an identical repeat stays absorbed for scripted
//! double-root invocations (#10542). These subprocess tests pin the
//! refusal on a representative command set and the absorb acceptance on
//! one command; every path here is constructed at runtime, so the suite
//! stays platform-neutral (no Windows-only path shapes).

use super::{run_ripr, unique_temp_workspace};
use std::fs;

fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// One refusal shape per representative command family: parsing fails
/// before any root access, so relative values need no fixture workspace.
#[test]
fn conflicting_repeated_root_refuses_with_exit_2() -> Result<(), String> {
    for (command, args) in [
        ("check", vec!["check", "--root", "a", "--root", "b"]),
        ("status", vec!["status", "--root", "a", "--root", "b"]),
        ("doctor", vec!["doctor", "--root", "a", "--root", "b"]),
        (
            "agent start",
            vec!["agent", "start", "--root", "a", "--root", "b"],
        ),
    ] {
        let output = run_ripr(&args);
        if output.status.code() != Some(2) {
            return Err(format!(
                "{command}: conflicting repeated --root must exit 2: {}",
                combined_output(&output)
            ));
        }
        let text = combined_output(&output);
        if !text.contains("--root") {
            return Err(format!("{command}: refusal must name --root: {text}"));
        }
        if !text.contains("takes --root once") {
            return Err(format!(
                "{command}: refusal must carry the repeated-flag wording: {text}"
            ));
        }
    }
    Ok(())
}

/// The refusal names both values, so a script or user can see which roots
/// collided without rerunning anything.
#[test]
fn conflicting_repeated_root_names_both_values() -> Result<(), String> {
    let output = run_ripr(&["check", "--root", "first", "--root", "second"]);
    if output.status.code() != Some(2) {
        return Err(format!(
            "conflicting repeated --root must exit 2: {}",
            combined_output(&output)
        ));
    }
    let text = combined_output(&output);
    if !text.contains("'first'") || !text.contains("'second'") {
        return Err(format!("both values must be named: {text}"));
    }
    Ok(())
}

/// #10542 contract: an identical repeated `--root` is silently absorbed,
/// so scripted double-root invocations keep working.
#[test]
fn identical_repeated_root_is_absorbed() -> Result<(), String> {
    let workspace = unique_temp_workspace("duplicate-root-absorbed");
    fs::create_dir_all(&workspace).map_err(|error| error.to_string())?;
    let root = workspace.display().to_string();
    let output = run_ripr(&["config", "validate", "--root", &root, "--root", &root]);
    let result = if output.status.code() != Some(0) {
        Err(format!(
            "identical repeated --root must be absorbed: {}",
            combined_output(&output)
        ))
    } else {
        Ok(())
    };
    let _ = fs::remove_dir_all(&workspace);
    result
}
