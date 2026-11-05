use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-current-state-contract-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(std::env::temp_dir())
            .map_err(|error| format!("create temporary root: {error}"))?;
        fs::create_dir(&root).map_err(|error| format!("create fixture: {error}"))?;
        let fixture = Self(root);
        // Minimal local Git metadata: the producer reads local config, never a
        // remote or network. No process-global cwd or environment mutation.
        fs::create_dir_all(fixture.0.join(".git/objects"))
            .map_err(|error| format!("create objects: {error}"))?;
        fs::create_dir_all(fixture.0.join(".git/refs"))
            .map_err(|error| format!("create refs: {error}"))?;
        fixture.write(".git/HEAD", "ref: refs/heads/main\n")?;
        fixture.write(
            ".git/config",
            "[core]\nrepositoryformatversion = 0\n[remote \"origin\"]\nurl = https://github.com/Example/Inventory.git\n",
        )?;
        fixture.write(
            "Cargo.toml",
            "[workspace.package]\nversion = \"0.11.0\"\nrust-version = \"1.95\"\n",
        )?;
        fixture.write("docs/ROADMAP.md", "# Roadmap\n\nCurrent bounded work.\n")?;
        fixture.write("README.md", "See docs/status/SUPPORT_TIERS.md.\n")?;
        fixture.write("docs/proposals/RIPR-PROP-0001-main.md", "# RIPR-PROP-0001\n")?;
        fixture.write("docs/proposals/RIPR-PROP-0002-fragment.md", "# RIPR-PROP-0002\n")?;
        fixture.write(
            "policy/doc-artifacts.toml",
            &format!("schema_version = \"1.0\"\n{}", artifact(1, "main", "accepted")),
        )?;
        fixture.write(
            "policy/doc-artifacts.d/current-state.toml",
            &artifact(2, "fragment", "proposed"),
        )?;
        fixture.write(
            "docs/status/SUPPORT_TIERS.md",
            "# Support Tiers\n\n## Current Support Map\n\n| Capability | Tier | Surface | Proof | Known limits |\n| --- | --- | --- | --- | --- |\n| Rust gap repair loop | `usable alpha` | CLI | `cargo xtask rust-repair-trust-report` | Governed promotion remains open. |\n| Registered graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Structural checks only. |\n",
        )?;
        // collect_files treats missing roots as empty; an explicit empty spec
        // directory makes the intended no-spec fixture boundary clear.
        fs::create_dir_all(fixture.0.join("docs/specs"))
            .map_err(|error| format!("create specs: {error}"))?;
        check_current_state(&fixture.0)?;
        Ok(fixture)
    }

    fn write(&self, relative: &str, body: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("create {relative}: {error}"))?;
        }
        fs::write(path, body).map_err(|error| format!("write {relative}: {error}"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn artifact(number: u32, name: &str, status: &str) -> String {
    format!(
        "\n[[artifact]]\nid = \"RIPR-PROP-{number:04}\"\nkind = \"proposal\"\npath = \"docs/proposals/RIPR-PROP-{number:04}-{name}.md\"\nstatus = \"{status}\"\nowner = \"repo-infra\"\n"
    )
}

fn check_error(root: &Path, expected: &str) -> Result<(), String> {
    match check_current_state(root) {
        Err(error) if error.contains(expected) => Ok(()),
        other => Err(format!(
            "expected current-state check error {expected:?}, got {other:?}"
        )),
    }
}

#[test]
fn current_state_counts_main_and_fragment_records_and_recorded_tiers() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let inventory = produce_current_state(&fixture.0)?;
    assert_eq!(inventory.registered_artifacts_by_kind_and_status["proposal"]["accepted"], 1);
    assert_eq!(inventory.registered_artifacts_by_kind_and_status["proposal"]["proposed"], 1);
    assert_eq!(inventory.support_rows_by_tier["usable alpha"], 1);
    assert_eq!(inventory.support_rows_by_tier["stable building block"], 1);
    assert!(inventory.contract_violations.is_empty());
    fixture.write(
        "policy/doc-artifacts.d/current-state.toml",
        &artifact(2, "fragment", "accepted"),
    )?;
    let changed = produce_current_state(&fixture.0)?;
    assert_eq!(changed.registered_artifacts_by_kind_and_status["proposal"]["accepted"], 2);
    assert!(!changed.registered_artifacts_by_kind_and_status["proposal"].contains_key("proposed"));
    Ok(())
}

#[test]
fn current_state_check_rejects_missing_registered_file() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.0.join("docs/proposals/RIPR-PROP-0002-fragment.md"))
        .map_err(|error| format!("remove registered artifact: {error}"))?;
    check_error(
        &fixture.0,
        "points at missing file `docs/proposals/RIPR-PROP-0002-fragment.md`",
    )?;
    let inventory = produce_current_state(&fixture.0)?;
    assert!(inventory.contract_violations.iter().any(|violation| {
        violation.contains("policy/doc-artifacts.d/current-state.toml:")
            && violation.contains("points at missing file")
    }));
    Ok(())
}

#[test]
fn current_state_check_rejects_unknown_support_proof_command() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let support = fs::read_to_string(fixture.0.join("docs/status/SUPPORT_TIERS.md"))
        .map_err(|error| format!("read support: {error}"))?;
    fixture.write(
        "docs/status/SUPPORT_TIERS.md",
        &support.replace("cargo xtask check-doc-artifacts", "cargo xtask nope"),
    )?;
    check_error(&fixture.0, "unknown xtask command `nope`")
}

#[test]
fn current_state_required_inputs_fail_by_name() -> Result<(), String> {
    for (path, expected) in [
        ("policy/doc-artifacts.toml", "doc-artifacts.toml"),
        ("docs/status/SUPPORT_TIERS.md", "SUPPORT_TIERS.md"),
    ] {
        let fixture = Fixture::new()?;
        fs::remove_file(fixture.0.join(path)).map_err(|error| format!("remove {path}: {error}"))?;
        check_error(&fixture.0, expected)?;
    }
    for (path, body, expected) in [
        ("policy/doc-artifacts.toml", "not valid ledger input\n", "doc-artifacts.toml"),
        (
            "policy/doc-artifacts.d/current-state.toml",
            "schema_version = \"1.0\"\n",
            "must not set schema_version",
        ),
        ("docs/status/SUPPORT_TIERS.md", "# Support Tiers\n", "Current Support Map"),
        (
            "docs/status/SUPPORT_TIERS.md",
            "# Support Tiers\n| Capability | Tier | Surface | Proof | Known limits |\n| broken | row |\n",
            "support-tier row must have 5 cells",
        ),
    ] {
        let fixture = Fixture::new()?;
        fixture.write(path, body)?;
        check_error(&fixture.0, expected)?;
    }
    Ok(())
}

fn report_pair(root: &Path) -> Result<(String, String), String> {
    let inventory = produce_current_state(root)?;
    Ok((
        serde_json::to_string_pretty(&inventory).map_err(|error| format!("serialize: {error}"))?,
        current_state_markdown(&inventory),
    ))
}

#[test]
fn current_state_report_bytes_are_equal_across_roots_including_diagnostics() -> Result<(), String> {
    let first = Fixture::new()?;
    let second = Fixture::new()?;
    assert_ne!(first.0, second.0);
    assert_eq!(report_pair(&first.0)?, report_pair(&second.0)?);
    for fixture in [&first, &second] {
        fs::remove_file(fixture.0.join("docs/proposals/RIPR-PROP-0002-fragment.md"))
            .map_err(|error| format!("remove artifact: {error}"))?;
    }
    let reports = report_pair(&first.0)?;
    assert!(reports.0.contains("points at missing file"));
    assert_eq!(reports, report_pair(&second.0)?);
    Ok(())
}

#[test]
fn current_state_generation_preserves_inputs_and_renders_shared_facts() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let before = current_state_input_snapshot(&fixture.0)?;
    check_current_state(&fixture.0)?;
    assert!(!fixture.0.join("target/ripr/reports").exists());
    let output = fixture.0.join("target/ripr/reports");
    let json = write_current_state_reports(&fixture.0, &output)?;
    assert_eq!(before, current_state_input_snapshot(&fixture.0)?);
    let expected = produce_current_state(&fixture.0)?;
    let decoded: CurrentStateInventoryV1 = serde_json::from_str(&json)
        .map_err(|error| format!("parse generated inventory: {error}"))?;
    assert_eq!(decoded, expected);
    let markdown = fs::read_to_string(output.join("current-state.md"))
        .map_err(|error| format!("read Markdown report: {error}"))?;
    assert_eq!(markdown, current_state_markdown(&decoded));
    assert!(markdown.contains("| proposal | proposed | 1 |"));
    assert!(markdown.contains("| usable alpha | 1 |"));
    assert!(markdown.contains("## Coverage and limitations"));
    assert!(!decoded.coverage_limitations.is_empty());
    assert!(
        decoded
            .coverage_limitations
            .iter()
            .any(|limit| limit.contains("unrun checks"))
    );
    Ok(())
}

#[test]
fn current_state_read_only_snapshot_covers_added_inputs_and_membership() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let before = current_state_input_snapshot(&fixture.0)?;
    for expected in [
        "policy/doc-artifacts.toml",
        "policy/doc-artifacts.d/current-state.toml",
        "docs/proposals/RIPR-PROP-0002-fragment.md",
        "docs/status/SUPPORT_TIERS.md",
        "README.md",
        "metrics/rust-repair-trust/corpus.json",
    ] {
        assert!(before.iter().any(|(path, _)| path == expected), "{expected}");
    }
    fixture.write("docs/specs/RIPR-SPEC-0001-new.md", "# RIPR-SPEC-0001\n")?;
    let after = current_state_input_snapshot(&fixture.0)?;
    assert_ne!(before, after);
    assert!(after.iter().any(|(path, bytes)| {
        path == "docs/specs/RIPR-SPEC-0001-new.md" && bytes.is_some()
    }));
    fixture.write(
        "docs/proposals/RIPR-PROP-0002-fragment.md",
        "# RIPR-PROP-0002\nChanged bytes.\n",
    )?;
    assert_ne!(after, current_state_input_snapshot(&fixture.0)?);
    Ok(())
}
