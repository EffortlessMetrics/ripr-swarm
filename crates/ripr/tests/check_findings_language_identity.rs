//! Native check-findings language identity control. The Python toggle also
//! changes conditional config-hash fields in the old producer, so this proves
//! behavior across complete native artifacts, not isolation of the new field.
#![cfg(feature = "lang-typescript")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/mod.rs"]
mod common;
use common::fixture_git::{fixture_git_ok, fixture_git_output};

struct Guard(PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())
}

#[test]
#[cfg(feature = "lang-python")]
fn complete_language_movement_is_not_comparable() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-language-complete-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let _guard = Guard(root.clone());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/blind_journey_installed_typescript/repository");
    copy_tree(&fixture.join("base"), &root)?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "fixture"],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["add", "-A"],
        &["commit", "-q", "-m", "base"],
    ] {
        fixture_git_ok(&root, args)?;
    }
    copy_tree(&fixture.join("head"), &root)?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "head"])?;
    let base = fixture_git_output(&root, &["rev-parse", "HEAD~1"])?
        .trim()
        .to_string();
    let snapshot_dir = root.join("target/ripr");
    std::fs::create_dir_all(&snapshot_dir).map_err(|e| e.to_string())?;
    let produce = |name: &str| -> Result<(PathBuf, serde_json::Value), String> {
        let out = ripr(
            &root,
            &[
                "check",
                "--root",
                ".",
                "--base",
                &base,
                "--format",
                "check-findings-json",
            ],
        )?;
        if !out.status.success() {
            return Err(format!(
                "producer: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let value: serde_json::Value =
            serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        if value["analysis_outcome"]["analysis_complete"] != true
            || value["findings"].as_array().is_none_or(Vec::is_empty)
        {
            return Err(format!("expected complete nonempty findings: {value}"));
        }
        let path = snapshot_dir.join(name);
        std::fs::write(&path, out.stdout).map_err(|e| e.to_string())?;
        Ok((path, value))
    };
    std::fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"typescript\"]\n",
    )
    .map_err(|e| e.to_string())?;
    let (before, first) = produce("before.json")?;
    std::fs::write(root.join("marker.txt"), "next\n").map_err(|e| e.to_string())?;
    fixture_git_ok(&root, &["add", "marker.txt"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "next"])?;
    let (control, same) = produce("control.json")?;
    std::fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"typescript\", \"python\"]\n",
    )
    .map_err(|e| e.to_string())?;
    let (after, changed) = produce("after.json")?;
    assert_eq!(
        first["artifact"]["analysis"]["input_identity"],
        same["artifact"]["analysis"]["input_identity"]
    );
    assert_ne!(
        first["artifact"]["analysis"]["input_identity"],
        changed["artifact"]["analysis"]["input_identity"]
    );
    let verify = |path: &Path| {
        ripr(
            &root,
            &[
                "agent",
                "verify",
                "--json",
                "--root",
                ".",
                "--before",
                &before.to_string_lossy(),
                "--after",
                &path.to_string_lossy(),
            ],
        )
    };
    let positive = verify(&control)?;
    if !positive.status.success() {
        return Err(format!(
            "same-language control: {}",
            String::from_utf8_lossy(&positive.stderr)
        ));
    }
    let negative = verify(&after)?;
    if negative.status.code() != Some(3)
        || !negative.stdout.is_empty()
        || !String::from_utf8_lossy(&negative.stderr).contains("analysis input identities differ")
    {
        return Err(format!(
            "language mismatch: status {:?}; stderr {}",
            negative.status.code(),
            String::from_utf8_lossy(&negative.stderr)
        ));
    }
    Ok(())
}

#[test]
fn public_api_javascript_selection_is_bound() -> Result<(), String> {
    use ripr::app::{CheckInput, OutputFormat, check_workspace_with_config, render_check};
    use ripr::config::RiprConfig;
    use ripr::domain::LanguageId;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-js-identity-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let _guard = Guard(root.clone());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/blind_journey_installed_typescript/repository");
    copy_tree(&fixture.join("base"), &root)?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "fixture"],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["add", "-A"],
        &["commit", "-q", "-m", "base"],
    ] {
        fixture_git_ok(&root, args)?;
    }
    copy_tree(&fixture.join("head"), &root)?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "head"])?;
    let base = fixture_git_output(&root, &["rev-parse", "HEAD~1"])?
        .trim()
        .to_owned();
    let input = CheckInput {
        root: root.clone(),
        base: Some(base),
        ..CheckInput::default()
    };
    let mut config = RiprConfig::default();
    config.languages.enabled = vec![LanguageId::TypeScript];
    let snapshots = root.join("target/ripr");
    std::fs::create_dir_all(&snapshots).map_err(|e| e.to_string())?;
    let produce =
        |name: &str, config: &RiprConfig| -> Result<(PathBuf, serde_json::Value), String> {
            let output = check_workspace_with_config(input.clone(), config)?;
            let raw = render_check(&output, &OutputFormat::CheckFindingsJson)?;
            let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            if value["analysis_outcome"]["analysis_complete"] != true
                || value["findings"].as_array().is_none_or(Vec::is_empty)
            {
                return Err(format!("expected complete nonempty findings: {value}"));
            }
            let path = snapshots.join(name);
            std::fs::write(&path, raw).map_err(|e| e.to_string())?;
            Ok((path, value))
        };
    let (before, first) = produce("before-js.json", &config)?;
    std::fs::write(root.join("marker.txt"), "next\n").map_err(|e| e.to_string())?;
    fixture_git_ok(&root, &["add", "marker.txt"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "next"])?;
    let (same, same_value) = produce("same-js.json", &config)?;
    config.languages.enabled.push(LanguageId::JavaScript);
    let (changed, changed_value) = produce("changed-js.json", &config)?;
    assert_eq!(
        first["artifact"]["analysis"]["input_identity"],
        same_value["artifact"]["analysis"]["input_identity"]
    );
    assert_ne!(
        first["artifact"]["analysis"]["input_identity"],
        changed_value["artifact"]["analysis"]["input_identity"]
    );
    let verify = |path: &Path| {
        ripr(
            &root,
            &[
                "agent",
                "verify",
                "--json",
                "--root",
                ".",
                "--before",
                &before.to_string_lossy(),
                "--after",
                &path.to_string_lossy(),
            ],
        )
    };
    let positive = verify(&same)?;
    if !positive.status.success() {
        return Err(format!(
            "same-set verify: {}",
            String::from_utf8_lossy(&positive.stderr)
        ));
    }
    let negative = verify(&changed)?;
    if negative.status.code() != Some(3)
        || !negative.stdout.is_empty()
        || !String::from_utf8_lossy(&negative.stderr).contains("analysis input identities differ")
    {
        return Err(format!(
            "changed-set verify: {:?}: {}",
            negative.status.code(),
            String::from_utf8_lossy(&negative.stderr)
        ));
    }
    Ok(())
}
