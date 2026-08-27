use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const VALID_SPEC: &str = include_str!("../../docs/templates/SPEC_TEMPLATE.md");

fn temp_root(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-spec-format-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("docs/specs")).map_err(|error| error.to_string())?;
    Ok(root)
}

fn write_spec(root: &Path, text: &str) -> Result<(), String> {
    fs::write(
        root.join("docs/specs/RIPR-SPEC-9999-template-fixture.md"),
        text,
    )
    .map_err(|error| error.to_string())
}

fn run_check(root: &Path) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("check-spec-format")
        .current_dir(root)
        .output()
        .map_err(|error| error.to_string())
}

fn output_text(output: &std::process::Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn finish(
    root: PathBuf,
    output: std::process::Output,
    expected_success: bool,
) -> Result<(), String> {
    let status = output.status.success();
    let details = output_text(&output);
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    if status == expected_success {
        Ok(())
    } else {
        Err(details)
    }
}

#[test]
fn canonical_template_materializes_into_a_valid_spec_without_git_history() -> Result<(), String> {
    let root = temp_root("template")?;
    write_spec(&root, &VALID_SPEC.replace("RIPR-SPEC-NNNN: Title", "RIPR-SPEC-9999: Template fixture"))?;
    let output = run_check(&root)?;
    finish(root, output, true)
}

#[test]
fn structural_spec_errors_remain_blocking() -> Result<(), String> {
    let root = temp_root("invalid")?;
    let invalid = VALID_SPEC
        .replace("RIPR-SPEC-NNNN: Title", "RIPR-SPEC-9999: Template fixture")
        .replacen("## Metrics", "## Metrics removed", 1);
    write_spec(&root, &invalid)?;
    let output = run_check(&root)?;
    let details = output_text(&output);
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    if output.status.success() {
        return Err(format!(
            "missing required heading passed unexpectedly: {details}"
        ));
    }
    if !details.contains("missing `## Metrics`") {
        return Err(format!(
            "missing-heading diagnostic was not preserved: {details}"
        ));
    }
    Ok(())
}
