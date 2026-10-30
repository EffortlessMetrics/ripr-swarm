#![cfg(feature = "lang-rust")]

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(root: &Path, args: &[&str], budget: Option<&str>) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(root).args(args);
    if let Some(budget) = budget {
        command.env("RIPR_CHECK_FINDINGS_BYTES", budget);
    }
    command.output().map_err(|error| error.to_string())
}

#[test]
fn bounded_findings_cannot_authorize_review_scope() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let fixture = Fixture(
        std::env::temp_dir().join(format!("ripr-capped-scope-{}-{stamp}", std::process::id())),
    );
    let root = fixture.0.as_path();
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"cap-control\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    let base_source = "pub fn a(n:i32)->bool { n > 1 }\npub fn b(n:i32)->bool { n > 2 }\npub fn c(n:i32)->bool { n > 3 }\n";
    std::fs::write(root.join("src/lib.rs"), base_source).map_err(|error| error.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "fixture"],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["add", "."],
        &["commit", "-qm", "base"],
    ] {
        fixture_git_ok(root, args)?;
    }
    let base = fixture_git_output(root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    std::fs::write(root.join("src/lib.rs"), base_source.replace(" > ", " >= "))
        .map_err(|error| error.to_string())?;
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(root, &["commit", "-qm", "changed"])?;
    let head = fixture_git_output(root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let root_arg = root.to_string_lossy();
    let check = |budget| {
        run(
            root,
            &[
                "check", "--root", &root_arg, "--base", &base, "--format", "json",
            ],
            Some(budget),
        )
    };
    let bounded = check("1")?;
    let full = check("0")?;
    if !bounded.status.success() || !full.status.success() {
        return Err(format!(
            "producer failed: {} / {}",
            String::from_utf8_lossy(&bounded.stderr),
            String::from_utf8_lossy(&full.stderr)
        ));
    }
    let b: Value = serde_json::from_slice(&bounded.stdout).map_err(|error| error.to_string())?;
    let f: Value = serde_json::from_slice(&full.stdout).map_err(|error| error.to_string())?;
    let count = f["findings"]
        .as_array()
        .ok_or("full findings absent")?
        .len();
    if count <= 1
        || b["findings"]
            .as_array()
            .ok_or("bounded findings absent")?
            .len()
            != 1
        || b["analysis_outcome"]["document_complete"] != false
        || b["analysis_outcome"]["findings_total"] != count as u64
    {
        return Err(format!(
            "fixture did not establish a bounded prefix: bounded={b}, full={f}"
        ));
    }
    let bounded_path = root.join("bounded.json");
    let full_path = root.join("full.json");
    std::fs::write(&bounded_path, bounded.stdout).map_err(|error| error.to_string())?;
    std::fs::write(&full_path, full.stdout).map_err(|error| error.to_string())?;
    let review = |path: &Path, out: &Path| {
        run(
            root,
            &[
                "review-comments",
                "--root",
                &root_arg,
                "--base",
                &base,
                "--head",
                &head,
                "--check-output",
                &path.to_string_lossy(),
                "--out",
                &out.to_string_lossy(),
            ],
            None,
        )
    };
    let refused_path = root.join("bounded-review.json");
    let refused = review(&bounded_path, &refused_path)?;
    if refused.status.code() != Some(2)
        || !String::from_utf8_lossy(&refused.stderr).contains("findings-bounded")
        || refused_path.exists()
    {
        return Err(format!(
            "bounded input was not refused: {:?} {}",
            refused.status,
            String::from_utf8_lossy(&refused.stderr)
        ));
    }
    let review_path = root.join("full-review.json");
    let accepted = review(&full_path, &review_path)?;
    if !accepted.status.success() {
        return Err(format!(
            "full review failed: {}",
            String::from_utf8_lossy(&accepted.stderr)
        ));
    }
    let review: Value =
        serde_json::from_slice(&std::fs::read(&review_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let scope = &review["analysis_scope"];
    if scope["scope"] != "diff_scoped_changed_files"
        || scope["classified_seams_considered"].as_u64().unwrap_or(0) == 0
        || scope["cap_omitted_seams"] != 0
    {
        return Err(format!("full review scope was not established: {scope}"));
    }
    let gate_path = root.join("gate.json");
    let gate = run(
        root,
        &[
            "gate",
            "evaluate",
            "--root",
            &root_arg,
            "--pr-guidance",
            &review_path.to_string_lossy(),
            "--out",
            &gate_path.to_string_lossy(),
        ],
        None,
    )?;
    if !gate.status.success() {
        return Err(format!(
            "gate failed: {}",
            String::from_utf8_lossy(&gate.stderr)
        ));
    }
    let gate: Value =
        serde_json::from_slice(&std::fs::read(gate_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if gate["analysis_scope"] != *scope {
        return Err("gate did not retain producer scope".to_string());
    }
    Ok(())
}
