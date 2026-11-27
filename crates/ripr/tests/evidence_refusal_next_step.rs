//! Refusals for a missing or broken input file name where a usable one comes
//! from, not just the read error: `ripr pr-evidence --check`,
//! `ripr impacted-evidence --check`, `ripr baseline update`,
//! `ripr calibrate cargo-mutants`, `ripr check --suppression-policy`,
//! `ripr agent receipt --verify-json` and the `ripr agent repair` attempt
//! selectors. Each case runs the real binary in a
//! scratch directory and pins the next-step clause, so a regression that drops
//! it fails here rather than only in the looser action-word floor
//! (`refusal_next_step.rs`).

#[path = "common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

fn scratch() -> Result<ScratchDir, String> {
    let dir = std::env::temp_dir().join(format!(
        "ripr-evidence-refusal-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch dir: {error}"))?;
    Ok(ScratchDir(dir))
}

/// Fixture git goes through the shared helper, which drops inherited `GIT_*`
/// selectors (for example a hook's `GIT_INDEX_FILE`) so setup cannot touch
/// the caller's repository.
fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let mut full = vec![
        "-c",
        "user.name=ripr",
        "-c",
        "user.email=ripr@example.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    full.extend_from_slice(args);
    common::fixture_git::fixture_git_ok(dir, &full)
}

/// `ripr` in `dir` with inherited `GIT_*` selectors removed, so the binary
/// answers for the scratch directory rather than the caller's repository.
fn ripr(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(dir).args(args);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
}

fn committed_repo() -> Result<ScratchDir, String> {
    let dir = scratch()?;
    git(&dir.0, &["init", "-q"])?;
    std::fs::write(dir.0.join("README.md"), "fixture\n")
        .map_err(|error| format!("write README.md: {error}"))?;
    git(&dir.0, &["add", "README.md"])?;
    git(&dir.0, &["commit", "-q", "-m", "fixture"])?;
    Ok(dir)
}

/// Runs `ripr` in `dir` and returns stderr when the exit code is 2.
fn refusal(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = ripr(dir, args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.code() == Some(2) {
        Ok(stderr)
    } else {
        Err(format!(
            "ripr {args:?}: expected exit 2, got {:?}:\n{stderr}",
            output.status.code()
        ))
    }
}

fn stderr_has_all(stderr: &str, wanted: &[&str]) -> Result<(), String> {
    match wanted.iter().find(|clause| !stderr.contains(*clause)) {
        None => Ok(()),
        Some(missing) => Err(format!("stderr lacks `{missing}`:\n{stderr}")),
    }
}

const CHECK: &[&str] = &["pr-evidence", "--base", "HEAD", "--head", "HEAD", "--check"];

#[test]
fn pr_evidence_check_without_a_packet_names_the_command_that_writes_it() -> Result<(), String> {
    let repo = committed_repo()?;
    let stderr = refusal(&repo.0, CHECK)?;
    stderr_has_all(
        &stderr,
        &[
            "missing or unreadable target/ripr/pr/repo-exposure.json: ",
            "; run `ripr pr-evidence` with the same --base, --head and --root to write it, then rerun with --check",
        ],
    )
}

#[test]
fn pr_evidence_check_with_a_broken_packet_names_the_regeneration() -> Result<(), String> {
    let repo = committed_repo()?;
    let packet = repo.0.join("target/ripr/pr");
    std::fs::create_dir_all(&packet).map_err(|error| format!("create packet dir: {error}"))?;
    std::fs::write(packet.join("repo-exposure.json"), "{ not json")
        .map_err(|error| format!("write packet: {error}"))?;
    let stderr = refusal(&repo.0, CHECK)?;
    stderr_has_all(
        &stderr,
        &[
            "target/ripr/pr/repo-exposure.json is not valid JSON: ",
            "; regenerate it with `ripr pr-evidence` (same --base, --head and --root), then rerun with --check",
        ],
    )
}

/// The advice keeps the caller's own options: following it with a
/// non-default `--pr-evidence` path writes outputs the original check accepts.
#[test]
fn impacted_evidence_check_without_outputs_names_the_command_that_writes_them() -> Result<(), String>
{
    let dir = scratch()?;
    std::fs::write(
        dir.0.join("review.json"),
        r#"{"summary":{"ripr_severe_gap":false,"requires_targeted_mutation":false}}"#,
    )
    .map_err(|error| format!("write packet: {error}"))?;
    let check = [
        "impacted-evidence",
        "--pr-evidence",
        "review.json",
        "--check",
    ];
    let stderr = refusal(&dir.0, &check)?;
    stderr_has_all(
        &stderr,
        &[
            "missing or unreadable target/xtask/impacted-evidence/latest.json: ",
            "; rerun the same `ripr impacted-evidence` command without --check to write it",
        ],
    )?;
    success(&dir.0, &check[..3])?;
    success(&dir.0, &check)
}

fn success(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = ripr(dir, args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ripr {args:?}: expected success, got {:?}:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Every `baseline update` input refusal names that input's source: the
/// reviewed ledger (restored, since `ripr baseline create` starts an
/// unreviewed one) or `ripr gate evaluate` output. A `baseline create`
/// ledger is still accepted, so the fallback the advice names works.
#[test]
fn baseline_update_refusals_name_the_command_that_writes_each_input() -> Result<(), String> {
    let dir = scratch()?;
    let write = |name: &str, text: &str| {
        std::fs::write(dir.0.join(name), text).map_err(|error| format!("write {name}: {error}"))
    };
    write("current.json", r#"{"schema_version":"0.1","decisions":[]}"#)?;
    write("baseline.json", "{ not json")?;
    let update = [
        "baseline",
        "update",
        "--baseline",
        "baseline.json",
        "--current",
        "current.json",
        "--remove-resolved",
    ];
    const BASELINE_SOURCE: &str = "; pass the reviewed baseline ledger, restored from version control if this copy is damaged (`ripr baseline create` starts a new, unreviewed one)";
    const GATE_SOURCE: &str = "; pass the gate-decision JSON that `ripr gate evaluate` writes";
    let stderr = refusal(&dir.0, &update)?;
    stderr_has_all(
        &stderr,
        &["parse baseline baseline.json failed: ", BASELINE_SOURCE],
    )?;
    for (baseline, current, first) in [
        (
            r#"{"schema_version":"9","kind":"gate_baseline","entries":[]}"#,
            r#"{"schema_version":"0.1","decisions":[]}"#,
            "baseline baseline.json has unsupported schema_version `9`; expected `0.1`",
        ),
        (
            r#"{"schema_version":"0.1","entries":[]}"#,
            r#"{"schema_version":"0.1","decisions":[]}"#,
            "baseline baseline.json missing kind",
        ),
        (
            r#"{"schema_version":"0.1","kind":"other","entries":[]}"#,
            r#"{"schema_version":"0.1","decisions":[]}"#,
            "baseline baseline.json has unsupported kind `other`; expected `gate_baseline`",
        ),
    ] {
        write("baseline.json", baseline)?;
        write("current.json", current)?;
        stderr_has_all(&refusal(&dir.0, &update)?, &[first, BASELINE_SOURCE])?;
    }
    write("current.json", r#"{"schema_version":"0.1","decisions":[]}"#)?;
    success(
        &dir.0,
        &[
            "baseline",
            "create",
            "--from",
            "current.json",
            "--out",
            "baseline.json",
            "--force",
        ],
    )?;
    success(&dir.0, &update)?;

    write("current.json", r#"{"schema_version":"9","decisions":[]}"#)?;
    stderr_has_all(
        &refusal(&dir.0, &update)?,
        &[
            "current gate-decision current.json has unsupported schema_version `9`; expected `0.1`",
            GATE_SOURCE,
        ],
    )?;
    write("current.json", r#"{"decisions":[]}"#)?;
    stderr_has_all(
        &refusal(&dir.0, &update)?,
        &[
            "current gate-decision current.json missing schema_version",
            GATE_SOURCE,
        ],
    )
}

#[test]
fn calibrate_refusal_names_the_snapshot_command() -> Result<(), String> {
    let dir = scratch()?;
    std::fs::write(dir.0.join("mutants.json"), "[]")
        .map_err(|error| format!("write mutants.json: {error}"))?;
    let calibrate = [
        "calibrate",
        "cargo-mutants",
        "--repo-exposure-json",
        "repo.json",
        "--mutants-json",
        "mutants.json",
    ];
    for (repo, first) in [
        (
            r#"{"seams":[{"seam_id":"a"}]}"#,
            "repo exposure seam is missing string field `kind`",
        ),
        (
            r#"{"seams":[{"seam_id":"a","kind":"k","file":"src/a.rs"}]}"#,
            "repo exposure seam is missing numeric field `line`",
        ),
        (
            r#"{"schema_version":"0.1"}"#,
            "repo exposure JSON is missing `seams` array",
        ),
    ] {
        std::fs::write(dir.0.join("repo.json"), repo)
            .map_err(|error| format!("write repo.json: {error}"))?;
        stderr_has_all(
            &refusal(&dir.0, &calibrate)?,
            &[
                first,
                "; pass a file written by `ripr check --format repo-exposure-json`",
            ],
        )?;
    }
    Ok(())
}

/// A verify JSON whose shape is not what `ripr agent verify` writes names
/// that command, and a seam id it does not list says to pick one it does.
#[test]
fn agent_receipt_verify_json_refusals_name_the_producer() -> Result<(), String> {
    let dir = committed_repo()?;
    let receipt = [
        "agent",
        "receipt",
        "--root",
        ".",
        "--verify-json",
        "verify.json",
        "--seam-id",
        "seam-a",
        "--json",
    ];
    let producer = "; pass the unedited JSON that `ripr agent verify` wrote as --verify-json";
    for (verify, first) in [
        (
            r#"{"schema_version":"0.3"}"#,
            "agent verify JSON is missing `inputs`",
        ),
        (
            r#"{"schema_version":"0.3","inputs":{"before":"before.json"}}"#,
            "agent verify inputs is missing string field `after`",
        ),
    ] {
        std::fs::write(dir.0.join("verify.json"), verify)
            .map_err(|error| format!("write verify.json: {error}"))?;
        stderr_has_all(&refusal(&dir.0, &receipt)?, &[first, producer])?;
    }
    Ok(())
}

#[test]
fn suppression_policy_refusals_name_the_fix() -> Result<(), String> {
    let repo = committed_repo()?;
    let stderr = refusal(&repo.0, &["check", "--suppression-policy", "nope.toml"])?;
    stderr_has_all(
        &stderr,
        &[
            "failed to read suppression policy `",
            "nope.toml`: ",
            "; pass an existing TOML file as `--suppression-policy`",
        ],
    )?;
    std::fs::write(repo.0.join("bad.toml"), "[[suppressions]]\nbogus = 1\n")
        .map_err(|error| format!("write bad.toml: {error}"))?;
    let stderr = refusal(&repo.0, &["check", "--suppression-policy", "bad.toml"])?;
    stderr_has_all(
        &stderr,
        &[
            "bad.toml` is invalid; fix each problem listed below, then rerun:\n",
            "unsupported `[[suppressions]]` field `bogus`",
        ],
    )
}

/// An unknown `--attempt` names how to find the right one, and a repository
/// with no commit says to commit before a repair attempt can bind HEAD.
#[test]
fn agent_repair_attempt_refusals_name_the_fix() -> Result<(), String> {
    let repo = committed_repo()?;
    std::fs::create_dir_all(repo.0.join("target/ripr/team-attempts"))
        .map_err(|error| format!("create store: {error}"))?;
    let stderr = refusal(
        &repo.0,
        &[
            "agent",
            "repair",
            "--root",
            ".",
            "--attempt",
            "repair-attempt-0123456789abcdef01234567",
            "--store",
            "target/ripr/team-attempts",
            "--phase",
            "after",
        ],
    )?;
    // The status follow-up must search the same root and store as the lookup
    // that refused; a bare `ripr agent status` lists a different store.
    stderr_has_all(
        &stderr,
        &[
            "repair attempt manifest not found for repair-attempt-0123456789abcdef01234567 at ",
            "; check the --attempt ID and --store",
            "inspect it with `ripr agent status --root ",
            " --store target/ripr/team-attempts --attempt repair-attempt-0123456789abcdef01234567`",
            "No new repair attempt was started.",
        ],
    )?;
    if stderr.matches("ripr agent status").count() != 1 {
        return Err(format!("expected one status command:\n{stderr}"));
    }

    let fresh = scratch()?;
    git(&fresh.0, &["init", "-q"])?;
    std::fs::write(
        fresh.0.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    std::fs::create_dir_all(fresh.0.join("src")).map_err(|error| format!("create src: {error}"))?;
    std::fs::write(
        fresh.0.join("src/lib.rs"),
        "pub fn f(x: i32) -> i32 { x + 1 }\n",
    )
    .map_err(|error| format!("write lib.rs: {error}"))?;
    let stderr = refusal(
        &fresh.0,
        &[
            "agent",
            "repair",
            "--root",
            ".",
            "--seam-id",
            "x",
            "--phase",
            "before",
        ],
    )?;
    stderr_has_all(
        &stderr,
        &[
            "repair attempt requires a concrete repository HEAD: ",
            "; commit the repository at least once, then rerun",
        ],
    )?;

    // Control: a committed checkout whose HEAD ripr cannot use keeps Git's
    // diagnosis and is not told to commit again.
    let sha256 = scratch()?;
    git(&sha256.0, &["init", "-q", "--object-format=sha256"])?;
    std::fs::write(
        sha256.0.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    git(&sha256.0, &["add", "Cargo.toml"])?;
    git(&sha256.0, &["commit", "-q", "-m", "fixture"])?;
    let stderr = refusal(
        &sha256.0,
        &[
            "agent",
            "repair",
            "--root",
            ".",
            "--seam-id",
            "x",
            "--phase",
            "before",
        ],
    )?;
    stderr_has_all(
        &stderr,
        &[
            "repair attempt requires a concrete repository HEAD: ",
            "; fix the Git problem it names, then rerun",
        ],
    )?;
    if stderr.contains("commit the repository at least once") {
        return Err(format!(
            "committed SHA-256 checkout was told to commit again:\n{stderr}"
        ));
    }
    Ok(())
}

/// Input files that parse but do not hold what the command reads name the
/// command that writes a usable one (#16019).
#[test]
fn invalid_input_file_refusals_name_the_producer() -> Result<(), String> {
    let dir = committed_repo()?;
    let write = |name: &str, contents: &str| {
        std::fs::write(dir.0.join(name), contents).map_err(|error| format!("write {name}: {error}"))
    };
    write("check.json", r#"{"tool":"ripr"}"#)?;
    stderr_has_all(
        &refusal(
            &dir.0,
            &[
                "review-comments",
                "--root",
                ".",
                "--base",
                "HEAD",
                "--head",
                "HEAD",
                "--check-output",
                "check.json",
            ],
        )?,
        &[
            "is invalid: missing producer schema_version",
            "; rerun `ripr check --json` for the same root, base and diff, then pass its output to `--check-output`",
        ],
    )?;
    write("ledger.json", r#"{"schema_version":"0.1"}"#)?;
    let regenerate =
        "; regenerate it with `ripr reports gap-ledger` or pass the ledger that command wrote";
    stderr_has_all(
        &refusal(&dir.0, &["swarm", "queue", "--gap-ledger", "ledger.json"])?,
        &[
            "swarm queue --gap-ledger ledger.json is invalid: ",
            regenerate,
        ],
    )?;
    stderr_has_all(
        &refusal(
            &dir.0,
            &[
                "agent",
                "packet",
                "--root",
                ".",
                "--gap-ledger",
                "ledger.json",
                "--gap-id",
                "gap:demo",
                "--json",
            ],
        )?,
        &[
            "agent packet --gap-ledger ledger.json is invalid: ",
            regenerate,
        ],
    )?;
    write("receipt.json", "{ not json")?;
    stderr_has_all(
        &refusal(&dir.0, &["receipt", "check", "--path", "receipt.json"])?,
        &[
            "receipt at receipt.json is malformed: invalid JSON: ",
            "; rewrite it with `ripr receipt write`",
        ],
    )
}

/// `receipt write` names the checkout fix only when Git could not resolve
/// HEAD. A HEAD that resolves to an id receipts cannot bind (SHA-256) keeps
/// the resolver's own advice instead of being told to make a first commit.
#[test]
fn receipt_write_head_refusal_names_the_fix_that_applies() -> Result<(), String> {
    let write = [
        "receipt",
        "write",
        "--gap",
        "gap:demo",
        "--verify-command",
        "cargo test",
        "--status",
        "passed",
    ];
    let checkout_fix = "; run ripr from inside a Git checkout that has at least one commit";

    let unborn = scratch()?;
    git(&unborn.0, &["init", "-q"])?;
    let stderr = refusal(&unborn.0, &write)?;
    stderr_has_all(
        &stderr,
        &[
            "receipt requires a Git repository with a resolvable HEAD: ",
            checkout_fix,
        ],
    )?;

    let sha256 = scratch()?;
    git(&sha256.0, &["init", "-q", "--object-format=sha256"])?;
    git(
        &sha256.0,
        &["commit", "-q", "--allow-empty", "-m", "fixture"],
    )?;
    let stderr = refusal(&sha256.0, &write)?;
    stderr_has_all(&stderr, &["returned an invalid commit"])?;
    if stderr.contains(checkout_fix) {
        return Err(format!(
            "a committed SHA-256 checkout got the first-commit advice:\n{stderr}"
        ));
    }
    Ok(())
}
