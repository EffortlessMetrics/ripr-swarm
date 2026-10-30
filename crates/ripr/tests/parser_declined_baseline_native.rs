//! Native review-comments → gate → baseline parser-declined control.
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "common/mod.rs"]
mod common;
use common::fixture_git::{fixture_git_ok, fixture_git_output};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ripr {args:?}: {:?}\n{}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
fn read(root: &Path, name: &str) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&std::fs::read(root.join(name)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn write(root: &Path, path: &str, contents: &str) -> Result<(), String> {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, contents).map_err(|e| e.to_string())
}
fn review_and_gate(root: &Path, base: &str, label: &str) -> Result<(), String> {
    let review = format!("{label}-review.json");
    let gate = format!("{label}-gate.json");
    let markdown = format!("{label}-gate.md");
    run(
        root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            base,
            "--head",
            "HEAD",
            "--out",
            &review,
        ],
    )?;
    run(
        root,
        &[
            "gate",
            "evaluate",
            "--root",
            ".",
            "--pr-guidance",
            &review,
            "--mode",
            "acknowledgeable",
            "--out",
            &gate,
            "--out-md",
            &markdown,
        ],
    )
}

#[test]
fn parser_decline_keeps_native_baseline_debt() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-parser-declined-native-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let _fixture = Fixture(root.clone());
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&root, &["config", "commit.gpgsign", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname=\"parser-proof\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )?;
    write(
        &root,
        "src/lib.rs",
        "pub fn early(n: i32) -> bool { n > 10 }\n",
    )?;
    write(
        &root,
        "tests/boundary.rs",
        "#[test] fn boundary() { assert!(parser_proof::early(10)); }\n",
    )?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "-qm", "base"])?;
    let base = fixture_git_output(&root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let shallow = "pub fn early(n: i32) -> bool { n >= 10 }\n";
    write(&root, "src/lib.rs", shallow)?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "shallow"])?;
    review_and_gate(&root, &base, "shallow")?;
    run(
        &root,
        &[
            "baseline",
            "create",
            "--from",
            "shallow-gate.json",
            "--out",
            "baseline.json",
        ],
    )?;
    let baseline = read(&root, "baseline.json")?;
    let identities = baseline["entries"]
        .as_array()
        .ok_or("missing baseline entries")?
        .iter()
        .map(|e| e["identity"].clone())
        .collect::<Vec<_>>();
    if identities.is_empty() {
        return Err("fixture captured no baseline debt".into());
    }
    write(&root, "src/lib.rs", "pub fn unrelated() {}\n")?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "fixed"])?;
    review_and_gate(&root, &base, "fixed")?;
    run(
        &root,
        &[
            "baseline",
            "diff",
            "--baseline",
            "baseline.json",
            "--current",
            "fixed-gate.json",
            "--out",
            "fixed-delta.json",
            "--out-md",
            "fixed-delta.md",
        ],
    )?;
    let fixed = read(&root, "fixed-delta.json")?;
    assert_eq!(
        fixed["delta"]["resolved"],
        identities.len() as u64,
        "{fixed}"
    );
    run(
        &root,
        &[
            "baseline",
            "update",
            "--baseline",
            "baseline.json",
            "--current",
            "fixed-gate.json",
            "--remove-resolved",
            "--out",
            "fixed-update.json",
        ],
    )?;
    assert_eq!(
        read(&root, "fixed-update.json")?["entries"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    let declined = format!(
        "{shallow}pub fn deep() {{ {}{} }}\n",
        "{".repeat(400),
        "}".repeat(400)
    );
    write(&root, "src/lib.rs", &declined)?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "declined"])?;
    review_and_gate(&root, &base, "declined")?;
    run(
        &root,
        &[
            "baseline",
            "diff",
            "--baseline",
            "baseline.json",
            "--current",
            "declined-gate.json",
            "--out",
            "declined-delta.json",
            "--out-md",
            "declined-delta.md",
        ],
    )?;
    let delta = read(&root, "declined-delta.json")?;
    assert_eq!(
        delta["analysis_scope"]["parser_declined_files"],
        serde_json::json!(["src/lib.rs"])
    );
    assert_eq!(delta["delta"]["resolved"], 0);
    assert_eq!(
        delta["delta"]["stale_baseline_entry"],
        identities.len() as u64
    );
    let items = delta["items"].as_array().ok_or("missing delta items")?;
    assert_eq!(items.len(), identities.len());
    assert!(items.iter().all(|item| {
        item["not_evaluated_reason"]
            .as_str()
            .is_some_and(|s| s.contains("not parsed"))
    }));
    run(
        &root,
        &[
            "baseline",
            "update",
            "--baseline",
            "baseline.json",
            "--current",
            "declined-gate.json",
            "--remove-resolved",
            "--out",
            "declined-update.json",
        ],
    )?;
    let updated = read(&root, "declined-update.json")?;
    let retained = updated["entries"]
        .as_array()
        .ok_or("missing retained entries")?
        .iter()
        .map(|e| e["identity"].clone())
        .collect::<Vec<_>>();
    assert_eq!(identities, retained);
    Ok(())
}
