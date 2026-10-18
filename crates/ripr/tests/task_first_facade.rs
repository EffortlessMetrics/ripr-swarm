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
    run_ripr_env(current_dir, args, &[])
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
const FIXTURE_GAP: &str = "gap:4b5fdc1a2a157b0d";
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

/// Populate a committed workspace whose one function holds two boundary
/// seams with the same file, owner, kind and expression, so the producer gap
/// ID cannot tell them apart.
fn populate_duplicate_expression_fixture(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn tiered(amount: i32, threshold: i32) -> i32 {\n    let base = if amount >= threshold {\n        10\n    } else {\n        0\n    };\n    let bonus = if amount >= threshold {\n        5\n    } else {\n        0\n    };\n    base + bonus\n}\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        "use boundary_gap_fixture::tiered;\n\n#[test]\nfn below_threshold_pays_nothing() {\n    assert_eq!(tiered(50, 100), 0);\n}\n\n#[test]\nfn far_above_threshold_pays() {\n    assert_eq!(tiered(10_000, 100), 15);\n}\n",
    )
    .map_err(|e| e.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
    git_commit(root, "duplicate-expression fixture source")?;
    Ok(())
}

const TRIPLE_LIB: &str = "pub fn tiered(amount: i32, threshold: i32) -> i32 {\n    let base = if amount >= threshold {\n        10\n    } else {\n        0\n    };\n    let bonus = if amount >= threshold {\n        5\n    } else {\n        0\n    };\n    let extra = if amount >= threshold {\n        2\n    } else {\n        0\n    };\n    base + bonus + extra\n}\n";
/// The triple fixture after weakening only occurrences 1 and 3
/// (`>=` -> `>` on lines 2 and 12; line 7 is unchanged).
const TRIPLE_LIB_WEAKENED_1_AND_3: &str = "pub fn tiered(amount: i32, threshold: i32) -> i32 {\n    let base = if amount > threshold {\n        10\n    } else {\n        0\n    };\n    let bonus = if amount >= threshold {\n        5\n    } else {\n        0\n    };\n    let extra = if amount > threshold {\n        2\n    } else {\n        0\n    };\n    base + bonus + extra\n}\n";

/// Populate a committed workspace whose one function holds three boundary
/// seams with the same file, owner, kind and expression (lines 2, 7, 12).
fn populate_triple_expression_fixture(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), TRIPLE_LIB).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        "use boundary_gap_fixture::tiered;\n\n#[test]\nfn below_threshold_pays_nothing() {\n    assert_eq!(tiered(50, 100), 0);\n}\n\n#[test]\nfn far_above_threshold_pays() {\n    assert_eq!(tiered(10_000, 100), 17);\n}\n",
    )
    .map_err(|e| e.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
    git_commit(root, "triple-expression fixture source")?;
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
    let persisted = manifest["next_command"]
        .as_str()
        .ok_or("manifest next_command missing")?;
    assert!(
        persisted.contains("ripr agent repair") && persisted.contains("--phase after"),
        "persisted next_command stays the advanced after spelling:\n{persisted}"
    );
    let stderr = stderr_text(&repair);
    let printed = stderr
        .lines()
        .find_map(|line| line.strip_prefix("ripr: attempt next command: "))
        .ok_or_else(|| format!("before phase printed no attempt command:\n{stderr}"))?;
    assert!(
        printed.starts_with("ripr continue --root ")
            && printed.contains(&format!("--attempt {}", ids[0])),
        "human next command must name `ripr continue`:\n{printed}"
    );
    assert!(
        !printed.contains("--phase after"),
        "human next command must not teach `--phase after`:\n{printed}"
    );
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
    assert!(
        facade_stdout.contains(&format!("Next, after the test edit: {printed}")),
        "stdout trailer must name the same ordinary continue route:\n{facade_stdout}"
    );
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
    let status_command = report
        .pointer("/next_action/command")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("status JSON missing next_action.command:\n{report:#}"))?;
    assert_eq!(
        status_command, printed,
        "status must name the same ordinary continue route the before phase printed"
    );
    Ok(())
}

fn assert_no_repair_attempt_side_effects(root: &Path) {
    assert!(
        attempt_ids(root).is_empty(),
        "an ineligible explicit subject must create no repair attempt"
    );
    assert!(
        !root
            .join("target/ripr/repair-attempts/.before.lock")
            .exists(),
        "an ineligible explicit subject must not create the before-phase lock"
    );
    assert!(
        !root.join("target/ripr/repair-attempts").exists(),
        "an ineligible explicit subject must not create the repair-attempt store"
    );
}

/// #8166 Slice A (positive control): an explicit eligible seam still
/// starts exactly one before-phase attempt through the façade.
#[test]
fn repair_explicit_eligible_seam_starts_one_attempt() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-eligible-explicit")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let repair = run_ripr(root, &["repair", FIXTURE_SEAM, "--root", &root_arg])?;
    assert_exit(&repair, 0, "repair with an explicit eligible seam");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], FIXTURE_SEAM);
    Ok(())
}

/// #8166 Slice A: an explicit seam that is no longer repair-eligible
/// (its boundary is already gripped, so it is not a headline repair
/// target) must refuse as a Decision (exit 3) naming the ineligibility
/// reason, before any attempt or before-phase lock side effect.
#[test]
fn repair_explicit_ineligible_seam_refuses_without_attempt_or_lock() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-ineligible-seam")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{FIXTURE_WEAK_TEST}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|e| e.to_string())?;
    run_git(root, &["add", "tests/pricing.rs"])?;
    git_commit(root, "grip the boundary")?;
    let repair = run_ripr(root, &["repair", FIXTURE_SEAM, "--root", &root_arg])?;
    assert_exit(&repair, 3, "repair with an explicit ineligible seam");
    assert!(
        stdout_text(&repair).is_empty(),
        "ineligibility refusal must leave stdout empty:\n{}",
        stdout_text(&repair)
    );
    let stderr = stderr_text(&repair);
    assert!(
        stderr.contains(FIXTURE_SEAM),
        "refusal must name the seam:\n{stderr}"
    );
    assert!(
        stderr.contains("not a headline repair target"),
        "refusal must name the ineligibility reason:\n{stderr}"
    );
    assert_no_repair_attempt_side_effects(root);
    Ok(())
}

/// #8166 Slice A: a canonical gap ID (and its seam ID) whose class is
/// configured off must refuse as a Decision (exit 3) naming the policy
/// reason, before any attempt or before-phase lock side effect.
#[test]
fn repair_explicit_policy_omitted_gap_and_seam_refuse_without_attempt_or_lock() -> Result<(), String>
{
    let fixture = Fixture::boundary("facade-ineligible-gap")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    std::fs::write(
        root.join("ripr.toml"),
        "[severity.seams]\nweakly_gripped = \"off\"\n",
    )
    .map_err(|e| e.to_string())?;
    run_git(root, &["add", "ripr.toml"])?;
    git_commit(root, "configure weakly_gripped off")?;
    for item in [FIXTURE_GAP, FIXTURE_SEAM] {
        let repair = run_ripr(root, &["repair", item, "--root", &root_arg])?;
        assert_exit(&repair, 3, "repair with a policy-omitted explicit item");
        assert!(
            stdout_text(&repair).is_empty(),
            "policy refusal must leave stdout empty:\n{}",
            stdout_text(&repair)
        );
        let stderr = stderr_text(&repair);
        assert!(
            stderr.contains(item),
            "refusal must name the item {item}:\n{stderr}"
        );
        assert!(
            stderr.contains("configured off"),
            "refusal must name the policy ineligibility reason:\n{stderr}"
        );
        assert_no_repair_attempt_side_effects(root);
    }
    Ok(())
}

/// #8767: from a workspace member directory `repair`, `status`, `continue`
/// and `agent stub` use the resolved workspace root (attempt store under the
/// workspace `target/ripr`, never the member), and `pilot` keeps `--root .`
/// and an explicit `--out` as typed. Unix-only like the other implicit-root
/// smoke tests: the disclosure compares canonicalized temp paths.
#[cfg(unix)]
#[test]
fn member_directory_commands_use_the_resolved_workspace_root() -> Result<(), String> {
    let fixture = Fixture::dir("facade-member-root")?;
    let outer = fixture.root.canonicalize().map_err(|e| e.to_string())?;
    let member = outer.join("pkg");
    std::fs::create_dir_all(member.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(member.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(
        outer.join("Cargo.toml"),
        "[workspace]\nmembers = [\"pkg\"]\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(member.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(member.join("src/lib.rs"), FIXTURE_LIB).map_err(|e| e.to_string())?;
    std::fs::write(member.join("tests/pricing.rs"), FIXTURE_WEAK_TEST)
        .map_err(|e| e.to_string())?;
    init_repo(&outer)?;
    run_git(&outer, &["add", "Cargo.toml", "pkg"])?;
    git_commit(&outer, "member fixture source")?;
    let disclosure = format!("ripr: resolved workspace root to {} ", outer.display());

    let repair = run_ripr(&member, &["repair"])?;
    assert_exit(&repair, 0, "repair from a member directory");
    assert!(
        stderr_text(&repair).contains(&disclosure),
        "{}",
        stderr_text(&repair)
    );
    let ids = attempt_ids(&outer);
    assert_eq!(ids.len(), 1, "attempt store must live under the workspace");
    assert!(
        !member.join("target/ripr").exists(),
        "no member-local target/ripr"
    );

    for args in [&["status"][..], &["continue"][..]] {
        let output = run_ripr(&member, args)?;
        let all = format!("{}{}", stdout_text(&output), stderr_text(&output));
        assert!(all.contains(&disclosure), "{args:?}:\n{all}");
        assert!(
            all.contains(&ids[0]),
            "{args:?} must read the attempt `repair` wrote:\n{all}"
        );
    }

    let stub = run_ripr(&member, &["agent", "stub", "--at", "pkg/src/lib.rs:2"])?;
    assert_exit(&stub, 0, "agent stub resolves against the workspace");
    assert!(stderr_text(&stub).contains(&disclosure));

    let pinned = run_ripr(&member, &["pilot", "--root", ".", "--quiet"])?;
    assert!(!stderr_text(&pinned).contains("resolved workspace root"));
    assert!(
        member.join("target/ripr").exists() && !outer.join("target/ripr/pilot").exists(),
        "`--root .` pilot artifacts stay in the member"
    );

    let custom = outer.join("custom-out");
    let custom_arg = custom.to_string_lossy().into_owned();
    let typed_out = run_ripr(&member, &["pilot", "--out", &custom_arg, "--quiet"])?;
    assert!(stderr_text(&typed_out).contains(&disclosure));
    assert!(custom.exists(), "explicit --out stays as typed");
    assert!(
        !outer.join("target/ripr/pilot").exists(),
        "explicit --out must not also write the default packet directory"
    );
    Ok(())
}

fn run_ripr_env(
    current_dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<Output, String> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(current_dir).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

/// Weaken the boundary predicate (`>=` becomes `>`) as an uncommitted
/// worktree edit, so `ripr check --worktree` reports the seam's gap.
fn weaken_boundary_predicate(root: &Path) -> Result<(), String> {
    let lib = root.join("src/lib.rs");
    let source = std::fs::read_to_string(&lib).map_err(|e| e.to_string())?;
    assert!(
        source.contains("amount >= discount_threshold"),
        "fixture predicate must be present before weakening:\n{source}"
    );
    std::fs::write(
        &lib,
        source.replace(
            "amount >= discount_threshold",
            "amount > discount_threshold",
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The seam identity a printed `ripr repair <identity>` drill-down line
/// names, when the bounded check output offers one.
fn printed_repair_identity(check_stdout: &str) -> Option<String> {
    check_stdout.lines().find_map(|line| {
        line.trim()
            .strip_prefix("ripr repair ")
            .and_then(|tail| tail.split_whitespace().next())
            .map(str::to_string)
    })
}

fn run_worktree_check(root: &Path, root_arg: &str) -> Result<Output, String> {
    run_ripr(
        root,
        &["check", "--root", root_arg, "--base", "HEAD", "--worktree"],
    )
}

/// #8166 Slice C (positive control): the bounded human check drill-down for
/// a repair-eligible top finding prints a `ripr repair` line naming the seam
/// identity the repair resolver accepts, and running that exact identity
/// starts the before phase for the same seam. The printed identity is never
/// the `probe:` finding ID the resolver refuses.
#[test]
fn check_drill_down_prints_repair_identity_that_starts_the_same_seam() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-check-drilldown-eligible")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    weaken_boundary_predicate(root)?;
    let check = run_worktree_check(root, &root_arg)?;
    let stdout = stdout_text(&check);
    assert!(
        stdout.contains("Next: drill into the top finding:"),
        "check must print the drill-down block:\n{stdout}\nstderr:\n{}",
        stderr_text(&check)
    );
    assert!(
        stdout.contains("ripr explain"),
        "check must keep the explain next step:\n{stdout}"
    );
    let identity = printed_repair_identity(&stdout).ok_or_else(|| {
        format!("eligible top finding must print a `ripr repair` next step:\n{stdout}")
    })?;
    assert!(
        !identity.starts_with("probe:"),
        "the repair line must not name the refused finding ID: {identity}"
    );
    assert!(
        !stdout.contains("ripr repair probe:"),
        "no repair line may name a `probe:` identity:\n{stdout}"
    );
    // The other formats carry no new repair next step: human-full keeps its
    // per-finding explain/context drill-in only, and JSON is unchanged.
    let full = run_ripr(
        root,
        &[
            "check",
            "--root",
            &root_arg,
            "--base",
            "HEAD",
            "--worktree",
            "--format",
            "human-full",
        ],
    )?;
    assert!(
        !stdout_text(&full).contains("ripr repair"),
        "human-full must not gain the bounded repair line:\n{}",
        stdout_text(&full)
    );
    let json = run_ripr(
        root,
        &[
            "check",
            "--root",
            &root_arg,
            "--base",
            "HEAD",
            "--worktree",
            "--format",
            "json",
        ],
    )?;
    assert!(
        !stdout_text(&json).contains("ripr repair"),
        "JSON must not gain a repair next step:\n{}",
        stdout_text(&json)
    );
    // Round-trip: the printed identity resolves through `ripr repair` to
    // the same seam and starts exactly one before-phase attempt for it.
    // The before phase pins the repository at HEAD, so the worktree edit
    // is committed first (the recovery `ripr repair` itself names for a
    // dirty production file); committing identical bytes keeps the seam
    // identity the check printed.
    run_git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "weaken the boundary predicate")?;
    let repair = run_ripr(root, &["repair", &identity, "--root", &root_arg])?;
    assert_exit(&repair, 0, "repair with the check-printed identity");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], identity);
    Ok(())
}

/// #8166 Slice C (negative control): when the top finding's seam is no
/// longer repair-eligible (its boundary is already gripped, so it is not a
/// headline repair target), the drill-down prints no `ripr repair` line.
#[test]
fn check_drill_down_omits_repair_when_top_seam_is_not_eligible() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-check-drilldown-ineligible")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{FIXTURE_WEAK_TEST}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|e| e.to_string())?;
    run_git(root, &["add", "tests/pricing.rs"])?;
    git_commit(root, "grip the boundary")?;
    // A semantics-preserving predicate edit keeps the seam gripped (the
    // Slice A ineligible state) while still producing its check finding:
    // parenthesizing the comparison changes the changed line, not the
    // behavior the boundary test discriminates.
    let lib = root.join("src/lib.rs");
    let source = std::fs::read_to_string(&lib).map_err(|e| e.to_string())?;
    std::fs::write(
        &lib,
        source.replace(
            "amount >= discount_threshold",
            "(amount) >= discount_threshold",
        ),
    )
    .map_err(|e| e.to_string())?;
    let check = run_worktree_check(root, &root_arg)?;
    let stdout = stdout_text(&check);
    assert!(
        stdout.contains("Next: drill into the top finding:"),
        "the gripped finding must still drill down to explain/context:\n{stdout}"
    );
    assert!(
        printed_repair_identity(&stdout).is_none() && !stdout.contains("ripr repair"),
        "an ineligible seam must print no repair line:\n{stdout}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "check must never start a repair attempt"
    );
    Ok(())
}

/// #8166 Slice C (negative control): a truncated seam inventory cannot
/// establish eligibility, so the drill-down prints no `ripr repair` line
/// even though the seam itself is eligible in a complete inventory.
#[test]
fn check_drill_down_omits_repair_when_seam_inventory_is_truncated() -> Result<(), String> {
    let fixture = Fixture::two_seam("facade-check-drilldown-truncated")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    weaken_boundary_predicate(root)?;
    let check = run_ripr_env(
        root,
        &["check", "--root", &root_arg, "--base", "HEAD", "--worktree"],
        &[("RIPR_REPO_EXPOSURE_SEAM_LIMIT", "1")],
    )?;
    let stdout = stdout_text(&check);
    assert!(
        stdout.contains("Next: drill into the top finding:"),
        "check must print the drill-down block:\n{stdout}\nstderr:\n{}",
        stderr_text(&check)
    );
    assert!(
        printed_repair_identity(&stdout).is_none() && !stdout.contains("ripr repair"),
        "a truncated inventory must print no repair line:\n{stdout}"
    );
    Ok(())
}

/// #7882: display narration must never feed stable text back into identity.
/// #8166: the `Canonical gap:` line `ripr check` prints for a Rust finding is
/// the producer gap ID, not the seam-derived `gap:<hash>`. It must start the
/// same attempt as the seam ID, and only for the seam it names.
#[test]
fn repair_accepts_the_producer_gap_id_check_prints() -> Result<(), String> {
    let fixture = Fixture::two_seam("facade-producer-gap")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    // Wrong discriminator: names no seam, starts nothing.
    let wrong = run_ripr(
        root,
        &[
            "repair",
            "gap:rust:src/lib.rs:shipping_fee:predicate_boundary:predicate:items==other",
            "--root",
            &root_arg,
        ],
    )?;
    assert_exit(&wrong, 3, "repair with a producer gap naming no seam");
    assert!(
        attempt_ids(root).is_empty(),
        "an unmatched producer gap must create no attempt"
    );
    // The second function's producer gap selects the second seam, not the
    // first match.
    let repair = run_ripr(
        root,
        &[
            "repair",
            "gap:rust:src/lib.rs:shipping_fee:predicate_boundary:predicate:items==free_threshold",
            "--root",
            &root_arg,
        ],
    )?;
    assert_exit(&repair, 0, "repair with a producer gap ID");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], SECOND_SEAM);
    Ok(())
}

/// #8166: two seams that share file, owner, kind and expression make the
/// producer gap ID ambiguous. It must refuse with the bounded list and start
/// nothing, never pick the first match.
#[test]
fn repair_refuses_a_producer_gap_id_that_names_several_seams() -> Result<(), String> {
    let fixture = Fixture::dir("facade-producer-gap-ambiguous")?;
    let root = &fixture.root;
    populate_duplicate_expression_fixture(root)?;
    let root_arg = root.to_string_lossy().into_owned();
    let repair = run_ripr(
        root,
        &[
            "repair",
            "gap:rust:src/lib.rs:tiered:predicate_boundary:predicate:amount==threshold",
            "--root",
            &root_arg,
        ],
    )?;
    assert_exit(&repair, 3, "repair with an ambiguous producer gap ID");
    let stderr = String::from_utf8_lossy(&repair.stderr);
    assert!(
        stderr.contains("names 2 seams; select one explicitly"),
        "expected the ambiguous refusal, got: {stderr}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "an ambiguous producer gap must create no attempt"
    );
    Ok(())
}

/// The `probe:` finding ID a printed check drill-down line names, when the
/// bounded check output offers one.
fn printed_probe_identity(check_stdout: &str) -> Option<String> {
    check_stdout.split_whitespace().find_map(|word| {
        word.starts_with("probe:")
            .then(|| word.trim_matches(|c| c == '"' || c == '\'').to_string())
    })
}

/// #8166 residual (F1/F2): the three IDs one check screen prints for one
/// finding must each work in the surface that names them — the `probe:`
/// finding ID in `ripr repair` and `ripr agent repair`, and the seam ID in
/// `ripr explain`. Stale cross-namespace IDs refuse with no attempt, and a
/// truly unknown agent `--seam-id` keeps its wrong-namespace hint.
#[test]
fn cross_namespace_ids_round_trip_between_check_explain_and_repair() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-cross-namespace")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    weaken_boundary_predicate(root)?;
    let check = run_worktree_check(root, &root_arg)?;
    let stdout = stdout_text(&check);
    let seam_id = printed_repair_identity(&stdout)
        .ok_or_else(|| format!("eligible top finding must print a repair line:\n{stdout}"))?;
    let probe_id = printed_probe_identity(&stdout)
        .ok_or_else(|| format!("check must print a probe finding ID:\n{stdout}"))?;

    // Reverse direction: the seam ID explains the same finding.
    let explained = run_ripr(
        root,
        &[
            "explain",
            "--root",
            &root_arg,
            "--base",
            "HEAD",
            "--worktree",
            &seam_id,
        ],
    )?;
    assert_exit(&explained, 0, "explain with the seam ID check printed");
    assert!(
        stdout_text(&explained).contains(&probe_id),
        "explain by seam ID must render the same finding:\n{}",
        stdout_text(&explained)
    );

    // Stale probe ID: no seam carries its fingerprint, so both repair
    // surfaces refuse and start nothing.
    let stale = "probe:src_lib.rs:predicate:00000000";
    let repair_stale = run_ripr(root, &["repair", stale, "--root", &root_arg])?;
    assert_exit(&repair_stale, 3, "repair with a stale probe ID");
    let stale_stderr = stderr_text(&repair_stale);
    assert!(
        stale_stderr.contains("names no seam") && stale_stderr.contains("stale"),
        "stale probe refusal must name the stale mapping:\n{stale_stderr}"
    );
    assert!(
        !stale_stderr.contains("not a repair subject"),
        "stale probe refusal must not use the legacy blanket rejection:\n{stale_stderr}"
    );
    let agent_stale = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            stale,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&agent_stale, 2, "agent repair with a stale probe ID");
    assert!(
        stderr_text(&agent_stale).contains("not a seam ID"),
        "truly unknown agent seam ID keeps the wrong-namespace hint:\n{}",
        stderr_text(&agent_stale)
    );
    assert!(
        attempt_ids(root).is_empty(),
        "stale cross-namespace IDs must create no attempt"
    );

    // Forward direction: commit the weakened predicate (the before phase
    // pins HEAD), then the probe ID starts the same seam check printed.
    run_git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "weaken the boundary predicate")?;
    let repair = run_ripr(root, &["repair", &probe_id, "--root", &root_arg])?;
    assert_exit(&repair, 0, "repair with the probe ID check printed");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], seam_id);
    Ok(())
}

/// #8166 review: an invented probe ordinal (`.999` on the valid fingerprint
/// of the sole seam) and a `.2` whose second occurrence is not among the
/// remaining seams are both stale: they refuse with no attempt instead of
/// resolving to the sole remaining seam.
#[test]
fn repair_refuses_unassignable_probe_ordinals() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-probe-ordinal")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    weaken_boundary_predicate(root)?;
    let check = run_worktree_check(root, &root_arg)?;
    let probe_id = printed_probe_identity(&stdout_text(&check)).ok_or_else(|| {
        format!(
            "check must print a probe finding ID:\n{}",
            stdout_text(&check)
        )
    })?;
    for forged in [format!("{probe_id}.999"), format!("{probe_id}.2")] {
        let repair = run_ripr(root, &["repair", &forged, "--root", &root_arg])?;
        assert_exit(&repair, 3, "repair with an unassignable probe ordinal");
        let stderr = stderr_text(&repair);
        assert!(
            stderr.contains("names no seam") && stderr.contains("stale"),
            "an unassignable ordinal must refuse as stale ({forged}):\n{stderr}"
        );
    }
    assert!(
        attempt_ids(root).is_empty(),
        "unassignable probe ordinals must create no attempt"
    );
    Ok(())
}

/// #8166 round 3: a diff `probe:` `.2` with two changed duplicates is
/// NOT rank-bound to the second seam. The diff producer ranked only the
/// changed-line probes, so from the full inventory the ordinal cannot be
/// distinguished from an unchanged-between case: it refuses as ambiguous
/// (bounded seam list, no attempt). Seam IDs are learned through the
/// public `file:line` surface on separate committed fixtures.
#[test]
fn repair_with_second_occurrence_probe_id_refuses_as_ambiguous() -> Result<(), String> {
    let seam_at_line = |label: &str, location: &str| -> Result<String, String> {
        let fixture = Fixture::dir(label)?;
        let root = &fixture.root;
        populate_duplicate_expression_fixture(root)?;
        let lib = root.join("src/lib.rs");
        let source = std::fs::read_to_string(&lib).map_err(|e| e.to_string())?;
        std::fs::write(
            &lib,
            source.replace("amount >= threshold", "amount > threshold"),
        )
        .map_err(|e| e.to_string())?;
        run_git(root, &["add", "src/lib.rs"])?;
        git_commit(root, "weaken both duplicate predicates")?;
        let root_arg = root.to_string_lossy().into_owned();
        let repair = run_ripr(root, &["repair", location, "--root", &root_arg])?;
        assert_exit(&repair, 0, "repair learning a duplicate seam by file:line");
        let ids = attempt_ids(root);
        assert_eq!(ids.len(), 1, "exactly one learning attempt must be created");
        read_manifest(root, &ids[0])?["seam_id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "learning attempt must name its seam".to_string())
    };
    let first_seam = seam_at_line("facade-probe-ordinal-first", "src/lib.rs:2")?;
    let second_seam = seam_at_line("facade-probe-ordinal-second", "src/lib.rs:7")?;
    assert_ne!(first_seam, second_seam);

    let fixture = Fixture::dir("facade-probe-ordinal-second-target")?;
    let root = &fixture.root;
    populate_duplicate_expression_fixture(root)?;
    let lib = root.join("src/lib.rs");
    let source = std::fs::read_to_string(&lib).map_err(|e| e.to_string())?;
    std::fs::write(
        &lib,
        source.replace("amount >= threshold", "amount > threshold"),
    )
    .map_err(|e| e.to_string())?;
    let root_arg = root.to_string_lossy().into_owned();
    let check = run_worktree_check(root, &root_arg)?;
    let probe_id = printed_probe_identity(&stdout_text(&check))
        .ok_or_else(|| format!("check must print a probe ID:\n{}", stdout_text(&check)))?;
    run_git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "weaken both duplicate predicates")?;
    for forged in [format!("{probe_id}.999"), format!("{probe_id}.2")] {
        let repair = run_ripr(root, &["repair", &forged, "--root", &root_arg])?;
        assert_exit(&repair, 3, "repair with a diff probe ordinal");
        let stderr = stderr_text(&repair);
        assert!(
            stderr.contains("names 2 seams; select one explicitly")
                && stderr.contains(&first_seam)
                && stderr.contains(&second_seam),
            "a diff probe ordinal must refuse as ambiguous listing both seams ({forged}):\n{stderr}"
        );
    }
    assert!(
        attempt_ids(root).is_empty(),
        "a diff probe ordinal must create no attempt"
    );
    Ok(())
}

/// #8166 round 3 (reviewer case): three identical predicates where the
/// diff touches only occurrences 1 and 3, so the second changed probe is
/// `.2`. Inventory rank 2 is the unchanged middle seam; `.2` must not
/// bind it. It refuses as ambiguous listing the real (changed) seams
/// 1 and 3, excludes the middle seam, and creates no attempt.
#[test]
fn repair_with_diff_probe_ordinal_does_not_bind_unchanged_middle_seam() -> Result<(), String> {
    let seam_at_line = |label: &str, location: &str| -> Result<String, String> {
        let fixture = Fixture::dir(label)?;
        let root = &fixture.root;
        populate_triple_expression_fixture(root)?;
        std::fs::write(root.join("src/lib.rs"), TRIPLE_LIB_WEAKENED_1_AND_3)
            .map_err(|e| e.to_string())?;
        run_git(root, &["add", "src/lib.rs"])?;
        git_commit(root, "weaken occurrences 1 and 3")?;
        let root_arg = root.to_string_lossy().into_owned();
        let repair = run_ripr(root, &["repair", location, "--root", &root_arg])?;
        assert_exit(&repair, 0, "repair learning a triple seam by file:line");
        let ids = attempt_ids(root);
        assert_eq!(ids.len(), 1, "exactly one learning attempt must be created");
        read_manifest(root, &ids[0])?["seam_id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "learning attempt must name its seam".to_string())
    };
    let first_seam = seam_at_line("facade-triple-first", "src/lib.rs:2")?;
    let middle_seam = seam_at_line("facade-triple-middle", "src/lib.rs:7")?;
    let third_seam = seam_at_line("facade-triple-third", "src/lib.rs:12")?;
    assert_ne!(first_seam, middle_seam);
    assert_ne!(middle_seam, third_seam);

    let fixture = Fixture::dir("facade-triple-diff-ordinal")?;
    let root = &fixture.root;
    populate_triple_expression_fixture(root)?;
    // Diff touches only occurrences 1 and 3.
    std::fs::write(root.join("src/lib.rs"), TRIPLE_LIB_WEAKENED_1_AND_3)
        .map_err(|e| e.to_string())?;
    let root_arg = root.to_string_lossy().into_owned();
    let check = run_worktree_check(root, &root_arg)?;
    let probe_id = printed_probe_identity(&stdout_text(&check))
        .ok_or_else(|| format!("check must print a probe ID:\n{}", stdout_text(&check)))?;
    assert!(
        !probe_id.contains(".2"),
        "the printed top probe ID must be the bare base ID: {probe_id}"
    );
    run_git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "weaken occurrences 1 and 3")?;
    let second_id = format!("{probe_id}.2");
    let repair = run_ripr(root, &["repair", &second_id, "--root", &root_arg])?;
    assert_exit(
        &repair,
        3,
        "repair with diff probe .2 over occurrences 1 and 3",
    );
    let stderr = stderr_text(&repair);
    assert!(
        stderr.contains("names 2 seams; select one explicitly"),
        "diff .2 must refuse as ambiguous, not bind the middle seam:\n{stderr}"
    );
    assert!(
        stderr.contains(&first_seam) && stderr.contains(&third_seam),
        "ambiguous refusal must list the real changed seams 1 and 3:\n{stderr}"
    );
    assert!(
        !stderr.contains(&middle_seam),
        "ambiguous refusal must not list/bind the unchanged middle seam:\n{stderr}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "diff .2 must not start an attempt (in particular not on the middle seam)"
    );
    Ok(())
}

/// #8166 residual (F1): `ripr agent repair --seam-id` accepts the `probe:`
/// finding ID when it uniquely maps to an eligible seam.
#[test]
fn agent_repair_accepts_the_probe_id_check_prints() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-agent-probe")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    weaken_boundary_predicate(root)?;
    let check = run_worktree_check(root, &root_arg)?;
    let stdout = stdout_text(&check);
    let seam_id = printed_repair_identity(&stdout)
        .ok_or_else(|| format!("eligible top finding must print a repair line:\n{stdout}"))?;
    let probe_id = printed_probe_identity(&stdout)
        .ok_or_else(|| format!("check must print a probe finding ID:\n{stdout}"))?;
    run_git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "weaken the boundary predicate")?;
    let repair = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            &probe_id,
            "--phase",
            "before",
        ],
    )?;
    assert_exit(&repair, 0, "agent repair with the probe ID check printed");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], seam_id);
    Ok(())
}

/// #8166 residual (F2): a `file:line` location naming exactly one seam is a
/// repair subject; one naming no seam refuses with no attempt.
#[test]
fn repair_accepts_a_unique_file_line_location() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-repair-file-line")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let stale = run_ripr(root, &["repair", "src/lib.rs:99", "--root", &root_arg])?;
    assert_exit(&stale, 3, "repair with a file:line naming no seam");
    assert!(
        stderr_text(&stale).contains("names no seam"),
        "stale location refusal must name the miss:\n{}",
        stderr_text(&stale)
    );
    assert!(
        attempt_ids(root).is_empty(),
        "a stale location starts nothing"
    );
    let repair = run_ripr(root, &["repair", "src/lib.rs:2", "--root", &root_arg])?;
    assert_exit(&repair, 0, "repair with a unique file:line location");
    let ids = attempt_ids(root);
    assert_eq!(ids.len(), 1, "exactly one attempt must be created");
    assert_eq!(read_manifest(root, &ids[0])?["seam_id"], FIXTURE_SEAM);
    Ok(())
}

/// #8166 residual: two seams sharing file, owner, kind and expression share
/// one re-minted probe ID; it must refuse with the bounded candidate list
/// and start nothing, never pick the first seam. (The same
/// several-matches branch refuses an ambiguous `file:line` location.)
#[test]
fn repair_refuses_an_ambiguous_probe_id() -> Result<(), String> {
    let fixture = Fixture::dir("facade-repair-probe-ambiguous")?;
    let root = &fixture.root;
    populate_duplicate_expression_fixture(root)?;
    // Weaken both duplicate predicates identically, so check mints one probe
    // ID that both seams still re-mint.
    let lib = root.join("src/lib.rs");
    let source = std::fs::read_to_string(&lib).map_err(|e| e.to_string())?;
    std::fs::write(
        &lib,
        source.replace("amount >= threshold", "amount > threshold"),
    )
    .map_err(|e| e.to_string())?;
    let root_arg = root.to_string_lossy().into_owned();
    let check = run_worktree_check(root, &root_arg)?;
    let probe_id = printed_probe_identity(&stdout_text(&check))
        .ok_or_else(|| format!("check must print a probe ID:\n{}", stdout_text(&check)))?;
    let repair = run_ripr(root, &["repair", &probe_id, "--root", &root_arg])?;
    assert_exit(&repair, 3, "repair with an ambiguous probe ID");
    let stderr = stderr_text(&repair);
    assert!(
        stderr.contains("names 2 seams; select one explicitly"),
        "ambiguous probe ID must refuse with the candidate list:\n{stderr}"
    );
    assert!(
        attempt_ids(root).is_empty(),
        "an ambiguous probe ID must create no attempt"
    );
    Ok(())
}

/// #8166 residual (F3): a static-limited top finding (no static test path)
/// keeps its limitation guidance and prints no runnable `ripr repair` line,
/// while its explain/context drill-down stays.
#[test]
fn check_drill_down_omits_repair_for_a_static_limited_finding() -> Result<(), String> {
    let fixture = Fixture::dir("facade-check-drilldown-static-limited")?;
    let root = &fixture.root;
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML).map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), FIXTURE_LIB).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        "#[test]\nfn unrelated() {\n    assert_eq!(1 + 1, 2);\n}\n",
    )
    .map_err(|e| e.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src", "tests"])?;
    git_commit(root, "fixture source")?;
    weaken_boundary_predicate(root)?;
    run_git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "weaken the boundary predicate")?;
    let root_arg = root.to_string_lossy().into_owned();
    let check = run_ripr(root, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
    let stdout = stdout_text(&check);
    assert!(
        stdout.contains("limited by static analysis"),
        "the finding must be static-limited:\n{stdout}"
    );
    assert!(
        stdout.contains("Next: drill into the top finding:") && stdout.contains("ripr explain"),
        "explain/context drill-down must stay:\n{stdout}"
    );
    assert!(
        printed_repair_identity(&stdout).is_none() && !stdout.contains("ripr repair"),
        "a static-limited finding must print no runnable repair line:\n{stdout}"
    );
    Ok(())
}

/// Both percent spellings exist, so an accidental codec substitution selects
/// a real, wrong repository instead of merely producing a missing-path error.
#[test]
fn repair_human_paths_preserve_percent_root_identity() -> Result<(), String> {
    repair_human_path_identity_journey("repo 100%", "repo 100%25")
}

#[cfg(unix)]
#[test]
fn repair_human_paths_preserve_literal_backslash_root_identity() -> Result<(), String> {
    repair_human_path_identity_journey(r"repo\literal", "repo/literal")
}

fn repair_human_path_identity_journey(name: &str, sibling: &str) -> Result<(), String> {
    let parent = Fixture::dir("facade-human-path")?;
    let root = parent.root.join(name);
    let other = parent.root.join(sibling);
    populate_boundary_fixture(&root)?;
    populate_boundary_fixture(&other)?;
    let canonical = root.canonicalize().map_err(|error| error.to_string())?;
    let other_canonical = other.canonicalize().map_err(|error| error.to_string())?;
    assert_ne!(canonical, other_canonical);
    let root_arg = root.to_string_lossy().into_owned();
    let expected_display = if cfg!(windows) {
        root_arg.replace('\\', "/")
    } else {
        root_arg.clone()
    };
    let before = run_ripr(&parent.root, &["repair", "--root", &root_arg])?;
    assert_exit(&before, 0, "prepare attempt under literal root");
    let ids = attempt_ids(&root);
    assert_eq!(ids.len(), 1);
    assert!(
        attempt_ids(&other).is_empty(),
        "the sibling must stay untouched"
    );
    let id = &ids[0];
    let manifest = read_manifest(&root, id)?;
    let persisted_root = manifest["root"].as_str().ok_or("missing root")?;
    assert_eq!(
        Path::new(persisted_root)
            .canonicalize()
            .map_err(|e| e.to_string())?,
        canonical
    );
    let expected_persisted = if cfg!(windows) {
        canonical.to_string_lossy().replace('\\', "/")
    } else {
        canonical.to_string_lossy().into_owned()
    };
    assert_eq!(
        persisted_root, expected_persisted,
        "canonical identity spelling must stay unchanged"
    );
    let command = manifest["next_command"].clone();
    let command_text = command.as_str().ok_or("missing command")?;
    let root_token = command_text
        .strip_prefix("ripr agent repair --root ")
        .and_then(|tail| tail.split_once(" --attempt ").map(|(root, _)| root))
        .ok_or_else(|| format!("missing command root: {command_text}"))?;
    // These fixture names contain neither quotes nor expansions. Accept either
    // host's enclosing quote without duplicating the product's shell parser.
    let command_root = root_token
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| {
            root_token
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
        })
        .unwrap_or(root_token);
    assert_eq!(
        command_root, expected_display,
        "command must select the lexical root, not a display-codec sibling"
    );
    let stderr = stderr_text(&before);
    for file in [
        "workflow.json",
        "commands.md",
        "agent-brief.json",
        "agent-packet.json",
    ] {
        assert!(
            stderr.lines().any(|line| line
                == format!("ripr: wrote {expected_display}/target/ripr/workflow/{file}")),
            "missing literal artifact path for {file}: {stderr}"
        );
    }
    let attempt_relative = format!("target/ripr/repair-attempts/{id}/attempt.json");
    assert!(
        stderr.contains(&format!(
            "ripr: attempt manifest: {expected_display}/{attempt_relative}"
        )),
        "{stderr}"
    );
    let attempt_dir = root.join("target/ripr/repair-attempts").join(id);
    let commitment =
        std::fs::read(attempt_dir.join("before-commitment.sha256")).map_err(|e| e.to_string())?;
    let manifest_before = manifest_bytes(&root, id)?;
    let missing_id = "repair-attempt-000000000000000000000000";
    let missing = run_ripr(
        &parent.root,
        &["continue", "--root", &root_arg, "--attempt", missing_id],
    )?;
    assert_exit(&missing, 2, "unknown attempt");
    let missing_stderr = stderr_text(&missing);
    assert!(missing_stderr.contains(&format!("repair attempt manifest not found for {missing_id} at {expected_display}/target/ripr/repair-attempts/{missing_id}/attempt.json")), "{missing_stderr}");
    assert_eq!(manifest_bytes(&root, id)?, manifest_before);
    let wrong_root = run_ripr(
        &parent.root,
        &[
            "continue",
            "--root",
            &other.to_string_lossy(),
            "--attempt",
            id,
        ],
    )?;
    assert_exit(
        &wrong_root,
        2,
        "the same attempt cannot select a sibling repository",
    );
    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{FIXTURE_WEAK_TEST}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|e| e.to_string())?;
    let after = run_ripr(
        &parent.root,
        &["continue", "--root", &root_arg, "--attempt", id],
    )?;
    assert_exit(&after, 0, "consume the exact attempt after a focused edit");
    let after_stderr = stderr_text(&after);
    assert!(
        after_stderr.contains(&format!(
            "ripr: consuming attempt manifest {expected_display}/{attempt_relative}"
        )),
        "{after_stderr}"
    );
    let finished = read_manifest(&root, id)?;
    assert_eq!(finished["state"], "ready_to_finish");
    assert_eq!(finished["root"], manifest["root"]);
    assert_eq!(finished["next_command"], command);
    assert_eq!(finished["repair_attempt_id"], manifest["repair_attempt_id"]);
    assert_eq!(
        std::fs::read(attempt_dir.join("before-commitment.sha256")).map_err(|e| e.to_string())?,
        commitment
    );
    assert!(
        root.join("target/ripr/reports/agent-receipt.json")
            .is_file()
    );
    assert!(attempt_ids(&other).is_empty());
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

fn compatibility_packet_target(root: &Path) -> Result<String, String> {
    let bytes = std::fs::read(root.join("target/ripr/workflow/agent-packet.json"))
        .map_err(|error| format!("read prepared packet: {error}"))?;
    let packet: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode prepared packet: {error}"))?;
    packet["packets"]
        .as_array()
        .and_then(|packets| {
            packets
                .iter()
                .find(|packet| packet["seam_id"] == FIXTURE_SEAM)
        })
        .and_then(|packet| packet["allowed_edit_surface"][0].as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            format!("prepared packet has no focused target for {FIXTURE_SEAM}: {packet}")
        })
}

/// #7881: recovery belongs to the selected retained attempt, even when a
/// genuine later packet replaces the repository-global compatibility file.
#[test]
fn continue_no_movement_uses_retained_target_and_recovers_after_edit() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-no-movement")?;
    let root = &fixture.root;
    let root_arg = root.to_string_lossy().into_owned();
    let before = run_ripr(root, &["repair", "--root", &root_arg])?;
    assert_exit(&before, 0, "prepare pricing attempt");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    assert_eq!(compatibility_packet_target(root)?, "tests/pricing.rs");

    // Independently prepare a valid packet with the same production seam
    // but a different test target. Only the compatibility projection in the
    // original root is replaced; its source and retained artifacts stay put.
    let decoy = Fixture::boundary("facade-decoy")?;
    std::fs::rename(
        decoy.root.join("tests/pricing.rs"),
        decoy.root.join("tests/decoy.rs"),
    )
    .map_err(|error| format!("rename decoy test: {error}"))?;
    run_git(&decoy.root, &["add", "tests"])?;
    git_commit(&decoy.root, "decoy test target")?;
    let decoy_before = run_ripr(&decoy.root, &["repair"])?;
    assert_exit(&decoy_before, 0, "prepare genuine decoy packet");
    assert_eq!(compatibility_packet_target(&decoy.root)?, "tests/decoy.rs");
    std::fs::copy(
        decoy.root.join("target/ripr/workflow/agent-packet.json"),
        root.join("target/ripr/workflow/agent-packet.json"),
    )
    .map_err(|error| format!("replace compatibility packet: {error}"))?;
    assert_eq!(compatibility_packet_target(root)?, "tests/decoy.rs");
    assert!(run_git(root, &["status", "--porcelain"])?.is_empty());

    let refused = run_ripr(
        root,
        &["continue", "--root", &root_arg, "--attempt", &attempt_id],
    )?;
    assert_exit(&refused, 3, "continue before focused edit");
    assert!(
        refused.stdout.is_empty(),
        "facade refusal stdout must stay empty"
    );
    let stderr = stderr_text(&refused);
    assert!(stderr.contains("no repository movement"), "{stderr}");
    assert!(stderr.contains("awaiting_edit"), "{stderr}");
    assert!(stderr.contains("tests/pricing.rs"), "{stderr}");
    assert!(!stderr.contains("tests/decoy.rs"), "{stderr}");
    assert!(stderr.contains("then rerun"), "{stderr}");
    let inspection = stderr
        .lines()
        .find(|line| line.contains("ripr agent status --root"))
        .ok_or_else(|| format!("missing bound status command: {stderr}"))?;
    let expected_root = if cfg!(windows) {
        root_arg.replace('\\', "/")
    } else {
        root_arg.clone()
    };
    assert!(inspection.contains(&expected_root), "{inspection}");
    assert!(
        inspection.contains(&format!("--attempt {attempt_id}")),
        "{inspection}"
    );
    assert!(
        stderr.contains(&format!("--attempt {attempt_id}")),
        "{stderr}"
    );
    let manifest = read_manifest(root, &attempt_id)?;
    assert_eq!(manifest["state"], "awaiting_edit");
    let reason = manifest["last_after_refusal"]["reason"]
        .as_str()
        .unwrap_or_default();
    assert!(
        reason.starts_with("agent verify no repository movement"),
        "{reason}"
    );
    assert!(reason.contains("tests/pricing.rs"), "{reason}");
    assert!(!reason.contains("tests/decoy.rs"), "{reason}");
    assert!(!root.join("target/ripr/reports/agent-receipt.json").exists());

    // Copy the named inspection route's arguments independently; it must
    // select the same awaiting attempt and expose no receipt.
    let status = run_ripr(
        root,
        &[
            "agent",
            "status",
            "--root",
            &root_arg,
            "--attempt",
            &attempt_id,
            "--json",
        ],
    )?;
    assert_exit(&status, 0, "inspect refused attempt");
    let report: serde_json::Value = serde_json::from_slice(&status.stdout)
        .map_err(|error| format!("refused status JSON: {error}"))?;
    assert_eq!(report["attempt"]["attempt_id"], attempt_id);
    assert_eq!(report["attempt"]["status_class"], "awaiting_edit");
    assert!(report["attempt"]["receipt"].is_null(), "{report}");

    std::fs::write(
        root.join("tests/pricing.rs"),
        format!("{FIXTURE_WEAK_TEST}{FIXTURE_BOUNDARY_TEST}"),
    )
    .map_err(|error| format!("write focused edit: {error}"))?;
    let continued = run_ripr(
        root,
        &["continue", "--root", &root_arg, "--attempt", &attempt_id],
    )?;
    assert_exit(
        &continued,
        0,
        "retry after focused edit with decoy projection still present",
    );
    let manifest = read_manifest(root, &attempt_id)?;
    assert_eq!(manifest["state"], "ready_to_finish");
    assert!(manifest.get("last_after_refusal").is_none(), "{manifest}");
    assert!(
        root.join("target/ripr/reports/agent-receipt.json")
            .is_file()
    );
    Ok(())
}

#[test]
fn continue_no_movement_keeps_inline_test_module_restrictions() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-inline-no-movement")?;
    let root = &fixture.root;
    std::fs::remove_file(root.join("tests/pricing.rs")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        format!("{FIXTURE_LIB}\n#[cfg(test)]\nmod tests {{\n    use super::discounted_total;\n    #[test]\n    fn below() {{ assert_eq!(discounted_total(50, 100), 50); }}\n}}\n"),
    ).map_err(|error| error.to_string())?;
    run_git(root, &["add", "src", "tests"])?;
    git_commit(root, "inline test target")?;
    let before = run_ripr(root, &["repair"])?;
    assert_exit(&before, 0, "prepare inline attempt");
    let attempt_id = attempt_id_from_before_stderr(&stderr_text(&before))?;
    assert_eq!(compatibility_packet_target(root)?, "src/lib.rs");
    let refused = run_ripr(root, &["continue", "--attempt", &attempt_id])?;
    assert_exit(&refused, 3, "continue before inline edit");
    assert!(refused.stdout.is_empty());
    let stderr = stderr_text(&refused);
    for expected in [
        "awaiting_edit",
        "src/lib.rs",
        "existing `#[cfg(test)]` module",
        "new test functions",
        "production code",
        "module declaration",
        "existing tests",
    ] {
        assert!(stderr.contains(expected), "missing {expected}: {stderr}");
    }
    assert_eq!(read_manifest(root, &attempt_id)?["state"], "awaiting_edit");
    assert!(!root.join("target/ripr/reports/agent-receipt.json").exists());
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
            stdout_text(&repeated).contains("Receipt: issued"),
            "a completed attempt must carry its issued receipt:\n{}",
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
        // The refusal carries the status-shaped facts on stderr while
        // stdout stays empty per `EXIT_TYPED_REFUSAL_EMPTY_STDOUT`. The
        // flipped fixture keeps its terminal evidence by design, so the
        // test pins the status markers, not receipt absence: the
        // selector keys on state.
        assert!(
            stdout_text(&repeated).is_empty(),
            "a refused continue must keep stdout empty:\n{}",
            stdout_text(&repeated)
        );
        for needle in [
            "# RIPR Repair Attempt Status",
            "Status: ",
            attempt_id.as_str(),
        ] {
            assert!(
                stderr.contains(needle),
                "a {state} refusal must carry the status facts ({needle:?}):\n{stderr}"
            );
        }
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

/// Review repair (#7032, ReadyToFinish receipt honesty): a `ready_to_finish`
/// attempt is already complete only when the status authority confirms a
/// compliant receipt. With the retained receipt file removed, status reads
/// the attempt `corrupt_or_unavailable`; `continue --attempt` must report
/// the facts and refuse with exit 3 instead of claiming completion.
#[test]
fn continue_on_unconfirmed_ready_to_finish_refuses_without_completion() -> Result<(), String> {
    let fixture = Fixture::boundary("facade-terminal-unconfirmed")?;
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
    // The finished attempt first reads already-complete with its issued
    // receipt, so the refusal below is caused by the removed receipt
    // alone, not by the fixture.
    let complete = run_ripr(
        root,
        &["continue", "--attempt", &attempt_id, "--root", &root_arg],
    )?;
    assert_exit(&complete, 0, "continue on the issued attempt");
    assert!(
        stdout_text(&complete).contains("Receipt: issued"),
        "the control state must carry its issued receipt:\n{}",
        stdout_text(&complete)
    );
    // Remove the retained terminal receipt: the manifest still declares
    // it, so the status authority's fail-closed projection downgrades the
    // attempt instead of reading a compliant receipt.
    let manifest: serde_json::Value = read_manifest(root, &attempt_id)?;
    let receipt_path = manifest["terminal_artifacts"]
        .as_array()
        .and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact["role"] == "agent_receipt")
        })
        .and_then(|artifact| artifact["path"].as_str())
        .ok_or("finished manifest must retain an agent_receipt artifact")?;
    let receipt_file = root.join(receipt_path);
    std::fs::remove_file(&receipt_file)
        .map_err(|error| format!("remove {}: {error}", receipt_file.display()))?;
    let status = run_ripr(
        root,
        &["status", "--root", &root_arg, "--attempt", &attempt_id],
    )?;
    assert_exit(&status, 0, "status still reports the attempt");
    assert!(
        stdout_text(&status).contains("corrupt_or_unavailable"),
        "the receipt-less attempt must downgrade:\n{}",
        stdout_text(&status)
    );
    let sealed = manifest_bytes(root, &attempt_id)?;
    let refused = run_ripr(
        root,
        &["continue", "--attempt", &attempt_id, "--root", &root_arg],
    )?;
    assert_exit(
        &refused,
        3,
        "continue on a receipt-less finished attempt must refuse",
    );
    let stderr = stderr_text(&refused);
    assert!(
        stderr.contains("without confirming completion")
            && stderr.contains("no completion is claimed"),
        "an unconfirmed finish must not claim completion:\n{stderr}"
    );
    assert!(
        !stderr.contains("already complete"),
        "an unconfirmed finish is never already complete:\n{stderr}"
    );
    assert!(
        !stdout_text(&refused).contains("already complete"),
        "an unconfirmed finish must not claim completion on stdout:\n{}",
        stdout_text(&refused)
    );
    assert!(
        stdout_text(&refused).is_empty(),
        "a refused continue must keep stdout empty:\n{}",
        stdout_text(&refused)
    );
    for needle in [
        "# RIPR Repair Attempt Status",
        "corrupt_or_unavailable",
        attempt_id.as_str(),
    ] {
        assert!(
            stderr.contains(needle),
            "an unconfirmed refusal must carry the status facts ({needle:?}):\n{stderr}"
        );
    }
    assert_eq!(
        manifest_bytes(root, &attempt_id)?,
        sealed,
        "a refused continue must not rewrite terminal evidence"
    );
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
        stderr.contains("ripr check --root") && stderr.contains("ripr pilot --root"),
        "an ineligible repair must keep both the check and pilot routes:\n{stderr}"
    );
    assert!(
        stderr.contains("when this branch has changed code"),
        "the check route must stay conditional on a diff, since repair lists saved seams:\n{stderr}"
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

// Trust-bound exclusion is demonstrated beside the selector
// (`app::task_first::tests::implicit_continue_skips_trust_bound_attempts`),
// not here: the attempt commitment forbids staging a binding by editing
// a manifest, and the full trust ceremony belongs to the binding
// authority's own suite (`python_repair_attempt.rs`).
