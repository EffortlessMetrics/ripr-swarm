//! Actual check-fast dispatch controls for RIPR-SPEC-0060 (#7528).
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;
use std::time::{SystemTime, UNIX_EPOCH};

const SPEC: &str = "docs/specs/RIPR-SPEC-9999-support-route-control.md";

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", text(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn fixture() -> Result<Fixture, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    // Keep the private clone's path short: the fixture runs the real
    // `cargo fmt --check`, which passes every test target's file path on one
    // command line, and Windows refuses a line over 32,767 characters (os error
    // 206) once the workspace has a few hundred test targets (#9272).
    let root =
        std::env::temp_dir().join(format!("rsr{}-{:x}", std::process::id(), nonce & 0xff_ffff));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest has no repository parent")?;
    // Materialize tracked source once, with no build output or sibling worktree
    // inspection. The private clone retains a real selector and policy inputs.
    let source_text = source.to_str().ok_or("source path is not UTF-8")?;
    let head = git(source, &["rev-parse", "HEAD"])?;
    println!("fixture source/base: {head}");
    git(
        &fixture.0,
        &["clone", "--shared", "--no-checkout", source_text, "."],
    )?;
    git(&fixture.0, &["checkout", "--detach", &head])?;
    git(
        &fixture.0,
        &["update-ref", "refs/remotes/origin/main", &head],
    )?;
    git(
        &fixture.0,
        &["config", "user.name", "support-route-fixture"],
    )?;
    git(
        &fixture.0,
        &["config", "user.email", "fixture@example.invalid"],
    )?;
    Ok(fixture)
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    git(root, &["add", "--", SPEC])?;
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-m", message],
    )?;
    let changed = git(root, &["diff", "--name-only", "origin/main...HEAD"])?;
    if changed != SPEC {
        return Err(format!(
            "expected exactly the committed spec control, got {changed:?}"
        ));
    }
    println!(
        "committed control: {}",
        git(root, &["show", "--format=fuller", "HEAD", "--", SPEC])?
    );
    Ok(())
}

fn run(root: &Path, command: &str) -> Result<Output, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(root)
        .arg(command)
        .output()
        .map_err(|error| error.to_string())?;
    println!(
        "command={command}; native_exit={:?}; {}",
        output.status.code(),
        text(&output)
    );
    Ok(output)
}

fn read_report(root: &Path, name: &str) -> Result<String, String> {
    fs::read_to_string(root.join("target/ripr/reports").join(name))
        .map_err(|error| error.to_string())
}

#[test]
fn committed_spec_declarations_reach_actual_check_fast_dispatch() -> Result<(), String> {
    let fixture = fixture()?;
    let root = &fixture.0;
    fs::write(root.join(SPEC), "# RIPR-SPEC-9999: Support route control\n\nSupport-tier impact:\n\n- None. No product support claim changes.\n")
        .map_err(|error| error.to_string())?;
    commit(root, "invalid support declaration control")?;

    // Establish that the declaration is rejected by the unchanged real leaf,
    // then require the public aggregate dispatch to reach that same predicate.
    for command in ["check-support-tiers", "check-fast"] {
        let output = run(root, command)?;
        if output.status.code() != Some(1)
            || !text(&output).contains("spec_support_tier_reference_missing")
        {
            return Err(format!(
                "{command} missed the invalid declaration: {}",
                text(&output)
            ));
        }
    }
    let failure = read_report(root, "check-fast.md")?;
    let selector = read_report(root, "check-fast-selector.md")?;
    if !failure.contains("Status: failed")
        || !failure.contains("spec_support_tier_reference_missing")
        || !selector.contains("Selector: fast_gate_failed")
        || !selector.contains("Changed files: 1")
    {
        return Err(format!(
            "missing failure/selector evidence: {failure}\n{selector}"
        ));
    }

    fs::write(root.join(SPEC), "# RIPR-SPEC-9999: Support route control\n\nSupport-tier impact:\n\n- None.\n\nNo product support claim changes.\n")
        .map_err(|error| error.to_string())?;
    commit(root, "canonical no-impact declaration control")?;
    let output = run(root, "check-fast")?;
    if !output.status.success() {
        return Err(format!("canonical None failed: {}", text(&output)));
    }
    let success = read_report(root, "check-fast.md")?;
    let leaf = read_report(root, "support-tiers.md")?;
    if !success.contains("- check-support-tiers\n") || !leaf.contains("Status: pass") {
        return Err(format!(
            "actual success omitted leaf evidence: {success}\n{leaf}"
        ));
    }

    // Return the private clone's net diff to empty, remove its stale leaf
    // receipt and require the established unrelated/zero-path no-op.
    fs::remove_file(root.join(SPEC)).map_err(|error| error.to_string())?;
    git(root, &["add", "--", SPEC])?;
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "remove control",
        ],
    )?;
    if !git(root, &["diff", "--name-only", "origin/main...HEAD"])?.is_empty() {
        return Err("control removal did not restore zero-path selector".to_string());
    }
    let leaf_path = root.join("target/ripr/reports/support-tiers.md");
    fs::remove_file(&leaf_path).map_err(|error| error.to_string())?;
    let output = run(root, "check-fast")?;
    if !output.status.success() || leaf_path.exists() {
        return Err(format!(
            "zero-path support no-op changed: {}",
            text(&output)
        ));
    }
    if !read_report(root, "check-fast.md")?.contains("- check-support-tiers (not applicable)") {
        return Err("zero-path report omitted support applicability disposition".to_string());
    }
    Ok(())
}
