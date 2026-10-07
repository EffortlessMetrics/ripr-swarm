//! Task-first repair façade (#6305): `ripr repair`, `ripr continue`, and
//! `ripr status` over the RepairAttempt authority (#2927) and the #6304
//! canonical next action.
//!
//! Each test pins one Required Control from the issue. The façade delegates to
//! the same application services as `ripr agent repair` / `ripr agent status`
//! (control 10); control 11 is demonstrated by a removal experiment during
//! review, not by a committed test.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_workspace(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ripr-{label}-{stamp}-{pid}-{counter}"))
}

fn run_ripr(current_dir: &Path, args: &[&str]) -> Result<Output, String> {
    std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(current_dir)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_commit(root: &Path, message: &str) -> Result<(), String> {
    run_git(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            message,
        ],
    )?;
    Ok(())
}

fn init_repo(root: &Path) -> Result<(), String> {
    run_git(root, &["init"])?;
    // Ordinary Rust repair fixtures meet the build-output precondition
    // without adding another tracked analysis input.
    std::fs::write(root.join(".git/info/exclude"), "/target/\n")
        .map_err(|error| format!("write git exclude: {error}"))?;
    // Keep checkouts byte-identical to what was committed: the edit cage
    // digests raw bytes, so a CRLF rewrite would read as a new edit.
    run_git(root, &["config", "core.autocrlf", "false"])?;
    Ok(())
}

const FIXTURE_CARGO_TOML: &str = "[package]\nname = \"boundary_gap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"boundary_gap_fixture\"\npath = \"src/lib.rs\"\n";
const FIXTURE_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const FIXTURE_WEAK_TEST: &str = "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n";
const FIXTURE_SEAM: &str = "67fc764ba37d77bd";
const SECOND_SEAM: &str = "59fe960b7be4b8d6";
const FIXTURE_BOUNDARY_TEST: &str = "\n#[test]\nfn at_threshold_discounts() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

/// Populate a committed single-seam workspace ready for `ripr repair`.
/// Takes the root instead of minting it so the [`Fixture`] guard owns
/// the directory before any fallible step can leak it.
fn populate_boundary_fixture(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), FIXTURE_LIB).map_err(|e| e.to_string())?;
    std::fs::write(root.join("tests/pricing.rs"), FIXTURE_WEAK_TEST).map_err(|e| e.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
    git_commit(root, "fixture source")?;
    Ok(())
}

/// Populate a committed workspace with two actionable boundary seams.
/// Takes the root instead of minting it so the [`Fixture`] guard owns
/// the directory before any fallible step can leak it.
fn populate_two_seam_fixture(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n\npub fn shipping_fee(items: u32, free_threshold: u32) -> u32 {\n    if items >= free_threshold {\n        0\n    } else {\n        499\n    }\n}\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        "use boundary_gap_fixture::{discounted_total, shipping_fee};\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n\n#[test]\nfn small_order_pays_shipping() {\n    assert_eq!(shipping_fee(1, 10), 499);\n}\n\n#[test]\nfn huge_order_ships_free() {\n    assert_eq!(shipping_fee(10_000, 10), 0);\n}\n",
    )
    .map_err(|e| e.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
    git_commit(root, "two-seam fixture source")?;
    Ok(())
}

/// Populate a committed workspace with no actionable seam: an empty
/// library. Takes the root instead of minting it so the [`Fixture`]
/// guard owns the directory before any fallible step can leak it.
fn populate_empty_fixture(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), "// no public behavior yet\n")
        .map_err(|e| e.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src"])?;
    git_commit(root, "empty fixture source")?;
    Ok(())
}

/// A self-cleaning fixture workspace: dropping it removes the directory, so
/// tests cannot leak temp checkouts on failure.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn boundary(label: &str) -> Result<Self, String> {
        let fixture = Self {
            root: unique_temp_workspace(label),
        };
        populate_boundary_fixture(&fixture.root)?;
        Ok(fixture)
    }

    fn two_seam(label: &str) -> Result<Self, String> {
        let fixture = Self {
            root: unique_temp_workspace(label),
        };
        populate_two_seam_fixture(&fixture.root)?;
        Ok(fixture)
    }

    fn empty(label: &str) -> Result<Self, String> {
        let fixture = Self {
            root: unique_temp_workspace(label),
        };
        populate_empty_fixture(&fixture.root)?;
        Ok(fixture)
    }

    fn dir(label: &str) -> Result<Self, String> {
        let fixture = Self {
            root: unique_temp_workspace(label),
        };
        std::fs::create_dir_all(&fixture.root).map_err(|error| format!("cwd: {error}"))?;
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn attempt_ids(root: &Path) -> Vec<String> {
    let dir = root.join("target/ripr/repair-attempts");
    let mut ids = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return ids;
    };
    for entry in entries.flatten() {
        if entry.path().join("attempt.json").is_file() {
            ids.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    ids.sort();
    ids
}

fn read_manifest(root: &Path, attempt_id: &str) -> Result<serde_json::Value, String> {
    let bytes = manifest_bytes(root, attempt_id)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("manifest for {attempt_id} is not JSON: {error}"))
}

fn manifest_bytes(root: &Path, attempt_id: &str) -> Result<Vec<u8>, String> {
    std::fs::read(
        root.join("target/ripr/repair-attempts")
            .join(attempt_id)
            .join("attempt.json"),
    )
    .map_err(|error| format!("read manifest bytes for {attempt_id}: {error}"))
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_exit(output: &Output, expected: i32, label: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{label}: expected exit {expected}\nstdout:\n{}\nstderr:\n{}",
        stdout_text(output),
        stderr_text(output)
    );
}

fn attempt_id_from_before_stderr(stderr: &str) -> Result<String, String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("ripr: attempt next command: "))
        .and_then(|command| {
            command
                .split_whitespace()
                .skip_while(|word| *word != "--attempt")
                .nth(1)
                .map(|id| id.trim_matches('\'').to_string())
        })
        .ok_or_else(|| format!("before phase printed no attempt command:\n{stderr}"))
}

/// Control 1: no current actionable item — `ripr repair` emits an honest
/// inspect/setup/limitation action and creates no attempt.
#[test]
fn repair_without_actionable_item_creates_no_attempt() -> Result<(), String> {
    let fixture = Fixture::empty("facade-empty")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let repair = run_ripr(root, &["repair", "--root", &root_arg])?;
    // A deliberate named outcome, not a failure: exit 3, stdout empty.
    assert_exit(&repair, 3, "repair with no actionable item");
    let stdout = stdout_text(&repair);
    assert!(
        stdout.is_empty(),
        "refusal must leave stdout empty:\n{stdout}"
    );
    let stderr = stderr_text(&repair);
    assert!(
        stderr.contains("no seams visible"),
        "must report the honest setup outcome:\n{stderr}"
    );
    assert!(
        stderr.contains("ripr pilot --root"),
        "must name the inspect route:\n{stderr}"
    );
    assert!(
        !stderr.contains("next action: run_command"),
        "must not decide executable with no actionable item:\n{stderr}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "no attempt may be created without an actionable item"
    );
    assert!(
        !root.join("target/ripr/repair-attempts").exists(),
        "no attempt store may be created without an actionable item"
    );
    Ok(())
}

/// Control 2: exactly one current actionable item — `ripr repair` creates
/// exactly one attempt and returns the same card as the canonical route.
#[test]
fn repair_single_item_matches_advanced_card() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-single")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let repair = run_ripr(root, &["repair", "--root", &root_arg])?;
    assert_exit(&repair, 0, "repair with one actionable item");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    let manifest = read_manifest(root, &ids[0])?;
    assert_eq!(manifest["seam_id"], FIXTURE_SEAM);
    assert_eq!(manifest["state"], "awaiting_edit");
    // The compact RepairCard renders on stdout after the start, and every
    // line of the canonical `agent card` rendering appears in order.
    let card = run_ripr(
        root,
        &[
            "agent",
            "card",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
        ],
    )?;
    assert_exit(&card, 0, "agent card for the started seam");
    let facade_stdout = stdout_text(&repair);
    let facade_lines: Vec<&str> = facade_stdout.lines().collect();
    let card_stdout = stdout_text(&card);
    let mut cursor = 0usize;
    for line in card_stdout.lines() {
        let Some(offset) = facade_lines[cursor..].iter().position(|c| *c == line) else {
            return Err(format!(
                "card line {line:?} missing from facade output:\n{facade_stdout}"
            ));
        };
        cursor += offset + 1;
    }
    // The started attempt resolves under the advanced routes.
    let status = run_ripr(
        root,
        &[
            "agent",
            "status",
            "--root",
            &root_arg,
            "--attempt",
            &ids[0],
            "--json",
        ],
    )?;
    assert_exit(&status, 0, "advanced status resolves the facade attempt");
    let report: serde_json::Value =
        serde_json::from_slice(&status.stdout).map_err(|error| format!("status JSON: {error}"))?;
    assert_eq!(report["attempt"]["attempt_id"], ids[0].as_str());
    assert_eq!(report["attempt"]["status_class"], "awaiting_edit");
    Ok(())
}

/// Control 3: two actionable items — no attempt is created until the user
/// selects one; never an implicit first/newest pick.
#[test]
fn repair_two_items_requires_explicit_selection() -> Result<(), String> {
    let fixture = Fixture::two_seam("facade-two")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let repair = run_ripr(root, &["repair", "--root", &root_arg])?;
    assert_exit(&repair, 3, "repair with two actionable items");
    assert!(
        stdout_text(&repair).is_empty(),
        "selection must leave stdout empty"
    );
    let stderr = stderr_text(&repair);
    assert!(
        stderr.contains("next action: choose_item"),
        "must decide an explicit item selection:\n{stderr}"
    );
    assert!(
        stderr.contains("stop [select_item]"),
        "must name the typed selection stop:\n{stderr}"
    );
    for seam in [FIXTURE_SEAM, SECOND_SEAM] {
        assert!(stderr.contains(seam), "must list seam {seam}:\n{stderr}");
    }
    assert!(
        stderr.contains("select one of 2"),
        "must bound the selection to both candidates:\n{stderr}"
    );
    assert!(
        stderr.contains("ripr repair "),
        "must name the retry command:\n{stderr}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "no attempt may be created before selection"
    );
    // Selecting one seam starts exactly that subject.
    let selected = run_ripr(root, &["repair", FIXTURE_SEAM, "--root", &root_arg])?;
    assert_exit(&selected, 0, "repair with an explicit item");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "selection starts exactly one attempt");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], FIXTURE_SEAM);
    Ok(())
}

/// Control 4: one current attempt — `ripr continue` advances that exact
/// attempt through the accepted after path.
#[test]
fn continue_single_attempt_advances_it() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-continue-one")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    // The attempt starts on the advanced route: the façade must consume
    // the same transaction it did not create (control 10 interop).
    let before = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&before, 0, "advanced before phase");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
        .map_err(|error| format!("read pricing tests: {error}"))?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let continued = run_ripr(root, &["continue", "--root", &root_arg])?;
    assert_exit(&continued, 0, "continue with one current attempt");
    assert!(
        stderr_text(&continued).contains(&attempt_id),
        "must advance the exact current attempt:\n{}",
        stderr_text(&continued)
    );
    assert_eq!(
        read_manifest(root, &attempt_id)?["state"],
        "ready_to_finish",
        "the after path must finish the attempt"
    );
    // The receipt and movement state read back through the façade status.
    let status = run_ripr(
        root,
        &[
            "status",
            "--root",
            &root_arg,
            "--attempt",
            &attempt_id,
            "--json",
        ],
    )?;
    assert_exit(&status, 0, "facade status after continue");
    let report: serde_json::Value =
        serde_json::from_slice(&status.stdout).map_err(|error| format!("status JSON: {error}"))?;
    assert_eq!(report["attempt"]["attempt_id"], attempt_id.as_str());
    assert!(
        report["attempt"]["receipt"].is_object(),
        "a finished attempt must carry its receipt:\n{report:#}"
    );
    Ok(())
}

/// Control 5: two current attempts — no newest/mtime fallback; explicit
/// selection is required and neither attempt moves before it.
#[test]
fn continue_two_attempts_requires_explicit_selection() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-continue-two")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    for _ in 0..2 {
        let before = run_ripr(
            root,
            &[
                "agent",
                "repair",
                "--root",
                &root_arg,
                "--seam-id",
                FIXTURE_SEAM,
                "--phase",
                "before",
            ],
        )?;
        assert_exit(&before, 0, "advanced before phase");
    }
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 2, "two current attempts must exist");
    let continued = run_ripr(root, &["continue", "--root", &root_arg])?;
    assert_exit(&continued, 3, "continue with two current attempts");
    assert!(
        stdout_text(&continued).is_empty(),
        "selection must leave stdout empty"
    );
    let stderr = stderr_text(&continued);
    for id in &ids {
        assert!(stderr.contains(id), "must list attempt {id}:\n{stderr}");
    }
    assert!(
        stderr.contains("ripr continue --attempt"),
        "must name the explicit-selection retry:\n{stderr}"
    );
    for id in &ids {
        assert_eq!(
            read_manifest(root, id)?["state"],
            "awaiting_edit",
            "no attempt may move before selection"
        );
    }
    // Selecting one attempt advances exactly that one.
    let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
        .map_err(|error| format!("read pricing tests: {error}"))?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let selected = run_ripr(
        root,
        &["continue", "--attempt", &ids[0], "--root", &root_arg],
    )?;
    assert_exit(&selected, 0, "continue with an explicit attempt");
    assert_eq!(read_manifest(root, &ids[0])?["state"], "ready_to_finish");
    assert_eq!(
        read_manifest(root, &ids[1])?["state"],
        "awaiting_edit",
        "the unselected attempt must stay current"
    );
    Ok(())
}

/// Control 6: stale head — continue refuses with the canonical
/// restart/recompute route and leaves the attempt awaiting its edit.
#[test]
fn continue_refuses_a_diverged_head_with_recovery() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-continue-stale")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    git_commit(root, "second commit")?;
    let before = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&before, 0, "advanced before phase");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    // Move HEAD to an ancestor of the prepared head: it no longer
    // descends, so the after path must refuse rather than finish.
    run_git(root, &["reset", "--hard", "HEAD~1"])?;
    let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
        .map_err(|error| format!("read pricing tests: {error}"))?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let continued = run_ripr(root, &["continue", "--root", &root_arg])?;
    assert_exit(&continued, 3, "continue on a diverged head");
    let stderr = stderr_text(&continued);
    assert!(
        stderr.contains("does not descend"),
        "must name the diverged head:\n{stderr}"
    );
    assert!(
        stderr.contains("ripr agent repair"),
        "must name the canonical restart route:\n{stderr}"
    );
    let manifest = read_manifest(root, &attempt_id)?;
    assert_eq!(
        manifest["state"], "awaiting_edit",
        "a refused attempt stays awaiting its edit"
    );
    assert!(
        manifest.get("last_after_refusal").is_some(),
        "the refusal must be recorded on the attempt:\n{manifest:#}"
    );
    Ok(())
}

/// Control 7: foreign CWD with `--root` — attempt, store, and artifacts
/// stay under the selected root on every façade command.
#[test]
fn facade_commands_stay_under_selected_root_from_foreign_cwd() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-foreign-root")?;
    let root = &fixture.root;
    let foreign_fixture = Fixture::dir("facade-foreign-cwd")?;
    let foreign = &foreign_fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let repair = run_ripr(foreign, &["repair", FIXTURE_SEAM, "--root", &root_arg])?;
    assert_exit(&repair, 0, "repair from a foreign cwd");
    assert_eq!(attempt_ids(root).len(), 1);
    assert!(
        !foreign.join("target").exists(),
        "no artifact may escape into the launching directory"
    );
    let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
        .map_err(|error| format!("read pricing tests: {error}"))?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let continued = run_ripr(foreign, &["continue", "--root", &root_arg])?;
    assert_exit(&continued, 0, "continue from a foreign cwd");
    let status = run_ripr(foreign, &["status", "--root", &root_arg, "--json"])?;
    assert_exit(&status, 0, "status from a foreign cwd");
    let report: serde_json::Value =
        serde_json::from_slice(&status.stdout).map_err(|error| format!("status JSON: {error}"))?;
    assert_eq!(report["repair_attempts"].as_array().map(Vec::len), Some(1));
    assert!(
        !foreign.join("target").exists(),
        "no artifact may escape into the launching directory"
    );
    assert!(
        root.join("target/ripr/repair-attempts").is_dir(),
        "the attempt store must live under the selected root"
    );
    assert!(
        root.join("target/ripr/workflow/after.repo-exposure.json")
            .is_file(),
        "after-phase artifacts must live under the selected root"
    );
    Ok(())
}

/// Control 8: terminal attempt — continue is idempotent already-complete
/// status with receipt/details, never another attempt or rewritten evidence.
#[test]
fn continue_on_a_terminal_attempt_is_already_complete() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-terminal")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let before = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&before, 0, "advanced before phase");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
        .map_err(|error| format!("read pricing tests: {error}"))?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let after = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--attempt",
            &attempt_id,
            "--phase",
            "after",
        ],
    )?;
    assert_exit(&after, 0, "advanced after phase");
    let sealed = manifest_bytes(root, &attempt_id)?;
    let status = run_ripr(
        root,
        &["status", "--root", &root_arg, "--attempt", &attempt_id],
    )?;
    assert_exit(&status, 0, "facade status of the terminal attempt");
    // Repeated invocation returns the same status, not another attempt.
    for _ in 0..2 {
        let repeated = run_ripr(
            root,
            &["continue", "--attempt", &attempt_id, "--root", &root_arg],
        )?;
        assert_exit(&repeated, 0, "repeated continue on a terminal attempt");
        assert_eq!(
            stdout_text(&repeated),
            stdout_text(&status),
            "already-complete output must equal the attempt status"
        );
        assert!(
            stderr_text(&repeated).contains("already complete"),
            "must say explicitly that the attempt is complete:\n{}",
            stderr_text(&repeated)
        );
        assert!(
            stdout_text(&repeated).contains("Receipt:"),
            "must carry the receipt details:\n{}",
            stdout_text(&repeated)
        );
    }
    assert_eq!(
        manifest_bytes(root, &attempt_id)?,
        sealed,
        "terminal evidence must be byte-identical after repeated continue"
    );
    assert_eq!(
        attempt_ids(root),
        vec![attempt_id.clone()],
        "no further attempt may be created"
    );
    // Without an ID, a lone terminal attempt is honestly no current
    // attempt — terminality never masquerades as resumability.
    let unselected = run_ripr(root, &["continue", "--root", &root_arg])?;
    assert_exit(&unselected, 3, "continue with no current attempt");
    assert!(
        stderr_text(&unselected).contains("no current attempt"),
        "must report no current attempt:\n{}",
        stderr_text(&unselected)
    );
    Ok(())
}

/// Control 9 (process half): human and JSON `status` agree on the same
/// semantic state for live attempts. The renderer-level agreement across
/// every attempt state lives in the library tests beside the shared DTO.
#[test]
fn status_human_and_json_agree_on_live_attempts() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-status-parity")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let before = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&before, 0, "advanced before phase");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    for use_json in [false, true] {
        let mut args = vec![
            "status",
            "--root",
            root_arg.as_str(),
            "--attempt",
            attempt_id.as_str(),
        ];
        if use_json {
            args.push("--json");
        }
        let status = run_ripr(root, &args)?;
        assert_exit(&status, 0, "facade status of the awaiting attempt");
        if use_json {
            let report: serde_json::Value = serde_json::from_slice(&status.stdout)
                .map_err(|error| format!("status JSON: {error}"))?;
            assert_eq!(report["attempt"]["attempt_id"], attempt_id.as_str());
            assert_eq!(report["attempt"]["state"], "awaiting_edit");
            assert_eq!(report["attempt"]["status_class"], "awaiting_edit");
        } else {
            let human = stdout_text(&status);
            assert!(
                human.contains(&attempt_id),
                "human status must name the attempt:\n{human}"
            );
            assert!(
                human.contains("Status: awaiting_edit"),
                "human status must name the class:\n{human}"
            );
            assert!(
                human.contains("Operational state: awaiting_edit"),
                "human status must name the state:\n{human}"
            );
        }
    }
    // The corrupt/unavailable selection is typed on both surfaces too.
    let bogus = "repair-attempt-000000000000000000000000";
    let human = run_ripr(root, &["status", "--root", &root_arg, "--attempt", bogus])?;
    assert_exit(&human, 0, "status of a missing attempt");
    assert!(
        stdout_text(&human).contains("Status: corrupt_or_unavailable"),
        "human status must type the missing attempt:\n{}",
        stdout_text(&human)
    );
    let json = run_ripr(
        root,
        &["status", "--root", &root_arg, "--attempt", bogus, "--json"],
    )?;
    assert_exit(&json, 0, "JSON status of a missing attempt");
    let report: serde_json::Value =
        serde_json::from_slice(&json.stdout).map_err(|error| format!("status JSON: {error}"))?;
    assert_eq!(report["attempt"]["status_class"], "corrupt_or_unavailable");
    assert_eq!(report["attempt"]["attempt_id"], bogus);
    Ok(())
}

/// Control 10: façade and advanced spellings produce the same attempt IDs,
/// manifests, cards, movement states, and receipts.
#[test]
fn facade_and_advanced_spellings_share_one_transaction() -> Result<(), String> {
    let facade_fixture = Fixture::boundary("facade-parity-a")?;
    let facade_root = &facade_fixture.root;
    let advanced_fixture = Fixture::boundary("facade-parity-b")?;
    let advanced_root = &advanced_fixture.root;
    let facade_arg = facade_root.to_string_lossy().into_owned();
    let advanced_arg = advanced_root.to_string_lossy().into_owned();
    // Twin starts: façade with an explicit item, advanced before phase.
    let facade_repair = run_ripr(
        facade_root,
        &["repair", FIXTURE_SEAM, "--root", &facade_arg],
    )?;
    assert_exit(&facade_repair, 0, "facade repair with an explicit item");
    let advanced_before = run_ripr(
        advanced_root,
        &[
            "agent",
            "repair",
            "--root",
            &advanced_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&advanced_before, 0, "advanced before phase");
    let facade_ids = attempt_ids(facade_root);
    let advanced_ids = attempt_ids(advanced_root);
    assert_eq!(facade_ids.len(), 1);
    assert_eq!(advanced_ids.len(), 1);
    // Manifests share structure: same seam, state, kind, and artifact
    // roles. Identity, timestamps, heads, and embedded commands differ
    // legitimately across twin repositories.
    let facade_manifest = read_manifest(facade_root, &facade_ids[0])?;
    let advanced_manifest = read_manifest(advanced_root, &advanced_ids[0])?;
    for pointer in ["/seam_id", "/state", "/kind", "/schema_version"] {
        assert_eq!(
            facade_manifest.pointer(pointer),
            advanced_manifest.pointer(pointer),
            "manifests must agree on {pointer}"
        );
    }
    let roles = |manifest: &serde_json::Value| {
        manifest["artifacts"]
            .as_array()
            .map(|artifacts| {
                artifacts
                    .iter()
                    .filter_map(|artifact| artifact["role"].as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert_eq!(roles(&facade_manifest), roles(&advanced_manifest));
    // Status projections are byte-identical across spellings for one
    // attempt: one DTO, one renderer pair.
    for with_json in [false, true] {
        let mut facade_args = vec![
            "status",
            "--root",
            facade_arg.as_str(),
            "--attempt",
            facade_ids[0].as_str(),
        ];
        let mut advanced_args = vec![
            "agent",
            "status",
            "--root",
            facade_arg.as_str(),
            "--attempt",
            facade_ids[0].as_str(),
        ];
        if with_json {
            facade_args.push("--json");
            advanced_args.push("--json");
        }
        let facade_status = run_ripr(facade_root, &facade_args)?;
        let advanced_status = run_ripr(facade_root, &advanced_args)?;
        assert_exit(&facade_status, 0, "facade status");
        assert_exit(&advanced_status, 0, "advanced status");
        assert_eq!(
            stdout_text(&facade_status),
            stdout_text(&advanced_status),
            "status spellings must render identical bytes (json={with_json})"
        );
    }
    // The inventory status agrees byte-for-byte as well.
    let facade_inventory = run_ripr(facade_root, &["status", "--root", &facade_arg, "--json"])?;
    let advanced_inventory = run_ripr(
        facade_root,
        &["agent", "status", "--root", &facade_arg, "--json"],
    )?;
    assert_exit(&facade_inventory, 0, "facade inventory status");
    assert_exit(&advanced_inventory, 0, "advanced inventory status");
    assert_eq!(
        stdout_text(&facade_inventory),
        stdout_text(&advanced_inventory),
        "inventory spellings must render identical bytes"
    );
    // Twin finishes: façade continue, advanced after. Same movement and
    // receipt disposition for the same edit.
    for root in [facade_root, advanced_root] {
        let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
            .map_err(|error| format!("read pricing tests: {error}"))?;
        std::fs::write(
            root.join("tests/pricing.rs"),
            format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
        )
        .map_err(|error| format!("write focused edit: {error}"))?;
    }
    let facade_continue = run_ripr(facade_root, &["continue", "--root", &facade_arg])?;
    assert_exit(&facade_continue, 0, "facade continue");
    let advanced_after = run_ripr(
        advanced_root,
        &[
            "agent",
            "repair",
            "--root",
            &advanced_arg,
            "--attempt",
            &advanced_ids[0],
            "--phase",
            "after",
        ],
    )?;
    assert_exit(&advanced_after, 0, "advanced after phase");
    let facade_finished = run_ripr(
        facade_root,
        &[
            "status",
            "--root",
            &facade_arg,
            "--attempt",
            &facade_ids[0],
            "--json",
        ],
    )?;
    let advanced_finished = run_ripr(
        advanced_root,
        &[
            "agent",
            "status",
            "--root",
            &advanced_arg,
            "--attempt",
            &advanced_ids[0],
            "--json",
        ],
    )?;
    let facade_report: serde_json::Value = serde_json::from_slice(&facade_finished.stdout)
        .map_err(|error| format!("facade status JSON: {error}"))?;
    let advanced_report: serde_json::Value = serde_json::from_slice(&advanced_finished.stdout)
        .map_err(|error| format!("advanced status JSON: {error}"))?;
    assert_eq!(
        facade_report["attempt"]["status_class"], advanced_report["attempt"]["status_class"],
        "both spellings must finish in the same movement state"
    );
    // Receipts embed their attempt's store path; redact the identity
    // before comparing the reading itself.
    let mut facade_receipt = facade_report["attempt"]["receipt"].clone();
    let mut advanced_receipt = advanced_report["attempt"]["receipt"].clone();
    facade_receipt
        .as_object_mut()
        .ok_or_else(|| "facade receipt is not an object".to_string())?
        .remove("path");
    advanced_receipt
        .as_object_mut()
        .ok_or_else(|| "advanced receipt is not an object".to_string())?
        .remove("path");
    assert_eq!(
        facade_receipt, advanced_receipt,
        "both spellings must issue the same receipt reading"
    );
    Ok(())
}

/// Control 12 (process half): short help is task-first and the full/machine
/// surfaces expose the alias/canonical relationships. Catalog-class guards
/// live in the library tests beside the catalog.
#[test]
fn help_surfaces_stay_task_first() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-help")?;
    let root = &fixture.root;
    let help = run_ripr(root, &["--help"])?;
    assert_exit(&help, 0, "short help");
    let short = stdout_text(&help);
    assert!(
        short.contains("ripr repair"),
        "short help must lead with the task-first repair:\n{short}"
    );
    assert!(
        short.contains("ripr continue"),
        "short help must lead with the task-first continue:\n{short}"
    );
    assert!(
        !short.contains("agent repair"),
        "short help must not teach the internal phase route first:\n{short}"
    );
    let all = run_ripr(root, &["help", "--all"])?;
    assert_exit(&all, 0, "exhaustive help");
    let exhaustive = stdout_text(&all);
    for needle in ["ripr repair ", "ripr continue", "ripr status "] {
        assert!(
            exhaustive.contains(needle),
            "exhaustive help must document {needle:?}"
        );
    }
    for line in exhaustive.lines() {
        // Listing lines start with two spaces and the command; prose
        // bullets name the spellings without markers.
        if !line.starts_with("  ripr ") {
            continue;
        }
        if line.contains("ripr agent repair") || line.contains("ripr agent status") {
            assert!(
                line.trim_end().ends_with("[advanced]"),
                "advanced spellings must carry the marker: {line:?}"
            );
        }
    }
    let json = run_ripr(root, &["help", "--json"])?;
    assert_exit(&json, 0, "machine help");
    let catalog: serde_json::Value =
        serde_json::from_slice(&json.stdout).map_err(|error| format!("help JSON: {error}"))?;
    let commands = catalog["commands"]
        .as_array()
        .ok_or_else(|| format!("help JSON has no commands:\n{catalog:#}"))?;
    let find = |id: &str| {
        commands
            .iter()
            .find(|row| row["id"] == id)
            .cloned()
            .ok_or_else(|| format!("help JSON lost {id}"))
    };
    // The machine document sorts every embedded string list.
    for (id, routes) in [
        ("cmd:repair", vec!["continue", "status"]),
        ("cmd:continue", vec!["receipt write", "status"]),
        ("cmd:status", vec!["continue", "repair"]),
    ] {
        let row = find(id)?;
        assert_eq!(row["class"], "public", "{id} must be public");
        assert_eq!(
            row["discovery"], "ordinary_public",
            "{id} must be ordinary discovery"
        );
        assert_eq!(
            row["relation"]["kind"], "canonical",
            "{id} must be canonical"
        );
        assert_eq!(
            row["next_routes"],
            serde_json::Value::Array(routes.into_iter().map(serde_json::Value::from).collect()),
            "{id} must route onward through the facade"
        );
    }
    for id in ["cmd:repair", "cmd:continue"] {
        let row = find(id)?;
        assert_eq!(
            row["exit"]["kind"], "decision_or_refusal",
            "{id} must advertise its decision exits"
        );
        assert_eq!(row["json_support"], false, "{id} has no --json flag");
    }
    let status = find("cmd:status")?;
    assert_eq!(status["exit"]["kind"], "completed_or_failed");
    assert_eq!(status["json_support"], true);
    for id in ["cmd:agent.repair", "cmd:agent.status"] {
        let row = find(id)?;
        assert_eq!(row["class"], "advanced", "{id} must be advanced");
        assert_eq!(row["discovery"], "advanced", "{id} must be advanced");
        assert_eq!(
            row["relation"]["kind"], "canonical",
            "{id} stays a supported spelling"
        );
    }
    let workflow = run_ripr(root, &["help", "workflow", "repair-gap"])?;
    assert_exit(&workflow, 0, "repair-gap workflow help");
    let rendered = stdout_text(&workflow);
    assert!(
        rendered.contains("ripr repair"),
        "the repair workflow must start at the facade:\n{rendered}"
    );
    Ok(())
}

/// The façade parses like its siblings: `--help` works per command, unknown
/// flags fail closed with usage, and missing values name their flag.
#[test]
fn facade_argument_errors_stay_usage_errors() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-args")?;
    let root = &fixture.root;
    for command in ["repair", "continue", "status"] {
        let help = run_ripr(root, &[command, "--help"])?;
        assert_exit(&help, 0, &format!("{command} --help"));
        assert!(
            stdout_text(&help).contains(&format!("Usage: ripr {command}")),
            "{command} help must name its usage:\n{}",
            stdout_text(&help)
        );
        let via_help = run_ripr(root, &["help", command])?;
        assert_exit(&via_help, 0, &format!("help {command}"));
        assert_eq!(
            stdout_text(&via_help),
            stdout_text(&help),
            "help {command} must print the command help"
        );
    }
    let unknown = run_ripr(root, &["repair", "--phase", "before"])?;
    assert_exit(&unknown, 2, "repair rejects phase vocabulary");
    assert!(
        stderr_text(&unknown).contains("--phase"),
        "the error must name the rejected flag:\n{}",
        stderr_text(&unknown)
    );
    let missing = run_ripr(root, &["status", "--attempt"])?;
    assert_exit(&missing, 2, "status names its missing value");
    assert!(
        stderr_text(&missing).contains("--attempt"),
        "the error must name the flag:\n{}",
        stderr_text(&missing)
    );
    Ok(())
}

/// Rewrite one manifest field in place. Used to stage terminal states
/// and trust bindings the test drives through the real selector; the
/// attempt lifecycle that produces them is covered beside the
/// authority, not here.
fn patch_manifest(root: &Path, attempt_id: &str, patch: &serde_json::Value) -> Result<(), String> {
    let path = root
        .join("target/ripr/repair-attempts")
        .join(attempt_id)
        .join("attempt.json");
    let mut manifest: serde_json::Value = read_manifest(root, attempt_id)?;
    for (key, value) in patch.as_object().cloned().unwrap_or_default() {
        manifest[key.as_str()] = value;
    }
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("rewrite manifest: {error}"))?;
    std::fs::write(&path, &bytes).map_err(|error| format!("write manifest: {error}"))?;
    Ok(())
}

/// Review repair (#7032): only a receipt-ready attempt is already
/// complete. A stale, failed, or incomparable attempt ends without a
/// receipt, so `continue --attempt` reports the facts and refuses with
/// exit 3 instead of claiming completion.
#[test]
fn continue_on_unsuccessful_terminal_attempts_refuses_without_completion() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-terminal-refusal")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let before = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&before, 0, "advanced before phase");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    let pricing = std::fs::read_to_string(root.join("tests/pricing.rs"))
        .map_err(|error| format!("read pricing tests: {error}"))?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{pricing}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let after = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--attempt",
            &attempt_id,
            "--phase",
            "after",
        ],
    )?;
    assert_exit(&after, 0, "advanced after phase");
    for state in ["stale", "failed", "incomparable"] {
        patch_manifest(root, &attempt_id, &serde_json::json!({ "state": state }))?;
        let sealed = manifest_bytes(root, &attempt_id)?;
        let repeated = run_ripr(
            root,
            &["continue", "--attempt", &attempt_id, "--root", &root_arg],
        )?;
        assert_exit(
            &repeated,
            3,
            &format!("continue on a {state} attempt must refuse"),
        );
        let stderr = stderr_text(&repeated);
        assert!(
            stderr.contains("without a receipt") && stderr.contains("no completion is claimed"),
            "a {state} attempt must not claim completion:\n{stderr}"
        );
        assert!(
            stderr.contains(&attempt_id),
            "the refusal must name the attempt:\n{stderr}"
        );
        assert!(
            !stderr.contains("already complete"),
            "a {state} attempt is never already complete:\n{stderr}"
        );
        assert!(
            stdout_text(&repeated).contains(&attempt_id),
            "the facts still print like status:\n{}",
            stdout_text(&repeated)
        );
        assert_eq!(
            manifest_bytes(root, &attempt_id)?,
            sealed,
            "a refused continue must not rewrite terminal evidence"
        );
    }
    assert_eq!(
        attempt_ids(root),
        vec![attempt_id.clone()],
        "no further attempt may be created"
    );
    Ok(())
}

/// Review repair (#7032): the auto selector applies the same
/// severity-off omission as the before-phase packet producer. With the
/// sole seam's class configured off, `ripr repair` reports no eligible
/// seam and starts nothing; restoring the severity starts the repair,
/// proving the omission caused the refusal.
#[test]
fn repair_skips_severity_off_seams_with_named_omission() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-severity-off")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    std::fs::write(
        root.join("ripr.toml"),
        "[severity.seams]\nweakly_gripped = \"off\"\n",
    )
    .map_err(|error| format!("write severity config: {error}"))?;
    let repair = run_ripr(root, &["repair", "--root", &root_arg])?;
    assert_exit(&repair, 3, "repair with the sole seam configured off");
    let stderr = stderr_text(&repair);
    assert!(
        stderr.contains("none repair-eligible"),
        "must report no eligible seam:\n{stderr}"
    );
    assert!(
        stderr.contains("configured off"),
        "must name the policy omission:\n{stderr}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "a refused start must create no attempt"
    );
    std::fs::write(
        root.join("ripr.toml"),
        "[severity.seams]\nweakly_gripped = \"warning\"\n",
    )
    .map_err(|error| format!("restore severity config: {error}"))?;
    let retry = run_ripr(root, &["repair", "--root", &root_arg])?;
    assert_exit(&retry, 0, "repair after restoring severity");
    assert_eq!(
        attempt_ids(root).len(),
        1,
        "restoring the severity must start the repair"
    );
    Ok(())
}

// Trust-bound exclusion is proven beside the selector
// (`app::task_first::tests::implicit_continue_skips_trust_bound_attempts`),
// not here: the attempt commitment forbids staging a binding by editing
// a manifest, and the full trust ceremony belongs to the binding
// authority's own suite (`python_repair_attempt.rs`).
