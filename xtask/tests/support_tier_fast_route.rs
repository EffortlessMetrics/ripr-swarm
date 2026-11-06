//! Actual check-fast dispatch controls for RIPR-SPEC-0060 (#7528).
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

const SPEC: &str = "docs/specs/RIPR-SPEC-9999-support-route-control.md";

struct Fixture {
    scratch: PathBuf,
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.scratch);
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

fn source_root() -> Result<&'static Path, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest has no repository parent".to_string())
}

fn write(root: &Path, path: &str, body: &str) -> Result<(), String> {
    let path = root.join(path);
    let parent = path.parent().ok_or("fixture file has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::write(path, body).map_err(|error| error.to_string())
}

fn fixture() -> Result<Fixture, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    // Keep the private repository path short on Windows (#9272). The actual
    // formatter needs only this dependency-free package, not every workspace
    // test target and its source in a second checkout (#12916).
    let scratch =
        std::env::temp_dir().join(format!("rsr{}-{:x}", std::process::id(), nonce & 0xff_ffff));
    fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
    let root = scratch.join("repo with spaces");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let fixture = Fixture { scratch, root };
    // A malformed sibling must neither contaminate the fixture's successful
    // declaration nor receive reports from a command run in the private root.
    write(
        &fixture.scratch,
        SPEC,
        "Support-tier impact: none. Outside the repository.\n",
    )?;
    let source = source_root()?;
    let head = git(source, &["rev-parse", "HEAD"])?;
    println!("fixture source: {head}");
    git(&fixture.root, &["init", "--initial-branch=main"])?;
    git(
        &fixture.root,
        &["config", "user.name", "support-route-fixture"],
    )?;
    git(
        &fixture.root,
        &["config", "user.email", "fixture@example.invalid"],
    )?;
    for (path, body) in [
        (".gitignore", "target/\nCargo.lock\n"),
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\n\n[package]\nname = \"support-route-control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n\n[lints]\nworkspace = true\n",
        ),
        (
            "rust-toolchain.toml",
            include_str!("../../rust-toolchain.toml"),
        ),
        ("src/lib.rs", "pub fn support_route_control() {}\n"),
        (
            "README.md",
            "# Support route fixture\n\n[Support tiers](docs/status/SUPPORT_TIERS.md).\n",
        ),
        (
            "docs/status/SUPPORT_TIERS.md",
            "# Support Tiers\n\n## Current Support Map\n\n| Capability | Tier | Surface | Proof | Known limits |\n| --- | --- | --- | --- | --- |\n| Rust gap repair loop | preview | CLI | `cargo xtask check-support-tiers` | Fixture-only static advisory surface. |\n",
        ),
        (
            ".ripr/static-language-allowlist.toml",
            "schema_version = 1\n",
        ),
        (
            "policy/generated_allowlist.txt",
            "# No tracked generated output.\n",
        ),
        (
            "policy/clippy-lints.toml",
            "schema_version = \"0.1\"\n\n[[active.rust]]\nname = \"unsafe_code\"\nlevel = \"forbid\"\n",
        ),
        (
            "policy/clippy-debt.toml",
            "schema_version = \"1.0\"\npolicy = \"clippy-debt\"\nowner = \"support-route-fixture\"\nstatus = \"active\"\n",
        ),
    ] {
        write(&fixture.root, path, body)?;
    }
    fs::create_dir_all(fixture.root.join("docs/specs")).map_err(|error| error.to_string())?;
    // The unchanged command catalog compares its built-in entries with real
    // workflow invocations. Copy just those inputs rather than inventing a
    // second catalog or mocking any of check-fast's unconditional gates.
    let workflows = fixture.root.join(".github/workflows");
    fs::create_dir_all(&workflows).map_err(|error| error.to_string())?;
    let mut workflow_count = 0;
    for entry in
        fs::read_dir(source.join(".github/workflows")).map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if matches!(
            entry.path().extension().and_then(|value| value.to_str()),
            Some("yml" | "yaml")
        ) {
            fs::copy(entry.path(), workflows.join(entry.file_name()))
                .map_err(|error| error.to_string())?;
            workflow_count += 1;
        }
    }
    if workflow_count == 0 {
        return Err("support route fixture copied no workflow inputs".to_string());
    }
    git(&fixture.root, &["add", "."])?;
    git(
        &fixture.root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture baseline",
        ],
    )?;
    git(
        &fixture.root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    )?;
    let toplevel = git(&fixture.root, &["rev-parse", "--show-toplevel"])?;
    if Path::new(&toplevel)
        .canonicalize()
        .map_err(|error| error.to_string())?
        != fixture
            .root
            .canonicalize()
            .map_err(|error| error.to_string())?
        || !git(
            &fixture.root,
            &["diff", "--name-only", "origin/main...HEAD"],
        )?
        .is_empty()
    {
        return Err("support route fixture is not an independent clean repository".to_string());
    }
    println!(
        "fixture workflows: {workflow_count}; tracked files: {}",
        git(&fixture.root, &["ls-files"])?.lines().count()
    );
    Ok(fixture)
}

fn selector_identity(root: &Path) -> Result<String, String> {
    Ok(format!(
        "Selector identity: origin/main={} HEAD={} merge-base={}",
        git(root, &["rev-parse", "origin/main"])?,
        git(root, &["rev-parse", "HEAD"])?,
        git(root, &["merge-base", "origin/main", "HEAD"])?,
    ))
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
    let output = xtask_bin::output(|xtask| {
        xtask.current_dir(root).arg(command);
    })?;
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
    // The full clone's positive phase also blocked invalid declarations in the
    // repository itself. Retain that obligation once in this same test; the
    // source-of-truth workflow is advisory and ci-policy does not own this leaf.
    let source_check = run(source_root()?, "check-support-tiers")?;
    if !source_check.status.success() {
        return Err(format!(
            "repository support tiers failed: {}",
            text(&source_check)
        ));
    }
    let fixture = fixture()?;
    let root = &fixture.root;
    for (message, declaration) in [
        (
            "invalid extended None control",
            "Support-tier impact:\n\n- None. No product support claim changes.\n",
        ),
        (
            "historical inline none control",
            "Support-tier impact: none. This governs the support route.\n",
        ),
    ] {
        write(
            root,
            SPEC,
            &format!("# RIPR-SPEC-9999: Support route control\n\n{declaration}"),
        )?;
        commit(root, message)?;

        // Establish that each declaration is rejected by the unchanged real
        // leaf, then require public aggregate dispatch to reach that predicate.
        for command in ["check-support-tiers", "check-fast"] {
            let output = run(root, command)?;
            if output.status.code() != Some(1)
                || !text(&output).contains("spec_support_tier_reference_missing")
                || !text(&output).contains(SPEC)
            {
                return Err(format!(
                    "{command} missed the invalid declaration: {}",
                    text(&output)
                ));
            }
        }
        let failure = read_report(root, "check-fast.md")?;
        let selector = read_report(root, "check-fast-selector.md")?;
        let identity = selector_identity(root)?;
        if !failure.contains("Status: failed")
            || !failure.contains("spec_support_tier_reference_missing")
            || !failure.contains(SPEC)
            || !failure.contains(&identity)
            || !selector.contains("Selector: fast_gate_failed")
            || !selector.contains("Changed files: 1")
            || !selector.contains(&identity)
        {
            return Err(format!(
                "missing failure/selector evidence: {failure}\n{selector}"
            ));
        }
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
    if !success.contains("- check-support-tiers\n")
        || !success.contains(&selector_identity(root)?)
        || !leaf.contains("Status: pass")
    {
        return Err(format!(
            "actual success omitted leaf evidence: {success}\n{leaf}"
        ));
    }

    // Return the private repository's net diff to empty, remove its stale leaf
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
    if !read_report(root, "check-fast.md")?.contains(&selector_identity(root)?)
        || fixture.scratch.join("target").exists()
        || fs::read_to_string(fixture.scratch.join(SPEC)).map_err(|error| error.to_string())?
            != "Support-tier impact: none. Outside the repository.\n"
    {
        return Err("support dispatch escaped its private repository root".to_string());
    }
    Ok(())
}
