//! Path and artifact-shape failures on swarm ingest, cache export, pilot
//! `--out`, agent verify, outcome and calibrate say what failed in user words,
//! name the flag and path, and give the next step (#14313). No bare
//! `os error N` tail and no serde struct names; exit codes stay `2`.
//!
//! A directory named where a file belongs is refused by the shared bounded
//! reader and the atomic writer in one plain sentence on every platform
//! (#14694), so commands without their own directory check inherit it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// A minimal Cargo workspace with an `art/` directory of fixture inputs.
fn workspace(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ripr-raw-error-{label}-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(dir.join("src")).map_err(|error| format!("create src: {error}"))?;
    fs::create_dir_all(dir.join("art/adir")).map_err(|error| format!("create art: {error}"))?;
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    fs::write(
        dir.join("src/lib.rs"),
        "pub fn f(x: i32) -> i32 {\n    x + 1\n}\n",
    )
    .map_err(|error| format!("write lib: {error}"))?;
    fs::write(dir.join("art/arr.json"), "[]\n").map_err(|error| format!("write arr: {error}"))?;
    fs::write(dir.join("art/file.json"), "{}\n").map_err(|error| format!("write file: {error}"))?;
    // canonicalize so expectations match the root spelling ripr resolves.
    fs::canonicalize(&dir).map_err(|error| format!("canonicalize workspace: {error}"))
}

fn run(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .current_dir(dir)
        .env_remove("RIPR_CACHE_DIR")
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Exact stderr, exit 2, and the shared floor: no errno tail, no serde type.
fn assert_refusal(dir: &Path, args: &[&str], expected: &str) -> Result<(), String> {
    let output = run(dir, args)?;
    let label = format!("ripr {}", args.join(" "));
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(2) {
        return Err(format!(
            "{label}: exit {:?}, wanted 2; stderr: {stderr}",
            output.status.code()
        ));
    }
    let wanted = format!("ripr: {expected}\n");
    if stderr != wanted {
        return Err(format!("{label}: stderr\n{stderr}\nwanted\n{wanted}"));
    }
    if stderr.contains("os error") || stderr.contains("struct ") {
        return Err(format!("{label}: raw OS or serde text leaked: {stderr}"));
    }
    Ok(())
}

#[test]
fn swarm_ingest_missing_result_names_the_route() -> Result<(), String> {
    let dir = workspace("swarm")?;
    let result = assert_refusal(
        &dir,
        &[
            "swarm",
            "ingest",
            "--root",
            ".",
            "--result",
            "nonexist.json",
        ],
        &format!(
            "swarm ingest --result nonexist.json was not found (relative paths resolve against --root {}); pass the agent result JSON your agent run wrote, for example `--result target/ripr/workflow/agent-result.json`",
            slash(&dir)
        ),
    );
    let _ = fs::remove_dir_all(&dir);
    result
}

#[test]
fn cache_export_out_problems_name_the_flag_and_fix() -> Result<(), String> {
    let dir = workspace("cache")?;
    // The export resolves the source cache first; an empty one is enough to
    // reach the destination checks.
    fs::create_dir_all(dir.join("target/ripr/cache"))
        .map_err(|error| format!("create cache: {error}"))?;
    let missing_parent = assert_refusal(
        &dir,
        &["cache", "export", "--out", "nodir/x/y"],
        &format!(
            "cache export --out {}: its parent directory {} does not exist; create that directory first, or pass an --out inside an existing directory",
            slash(&dir.join("nodir/x/y")),
            slash(&dir.join("nodir/x"))
        ),
    );
    let existing = assert_refusal(
        &dir,
        &["cache", "export", "--out", "art/adir"],
        &format!(
            "cache export --out {} already exists; --out must name a new directory that does not exist yet, so pass a fresh path (the export never merges into or replaces an existing one)",
            slash(&dir.join("art/adir"))
        ),
    );
    let _ = fs::remove_dir_all(&dir);
    missing_parent?;
    existing
}

#[test]
fn pilot_out_naming_a_file_says_out_is_a_directory() -> Result<(), String> {
    let dir = workspace("pilot")?;
    let result = assert_refusal(
        &dir,
        &["pilot", "--root", ".", "--out", "art/file.json"],
        "--out art/file.json is an existing file, but --out names the directory the artifacts are written into; pass a directory path (new or existing) with --out PATH",
    );
    let _ = fs::remove_dir_all(&dir);
    result
}

#[test]
fn agent_verify_non_object_snapshot_names_the_expected_artifact() -> Result<(), String> {
    let dir = workspace("verify")?;
    let result = assert_refusal(
        &dir,
        &[
            "agent",
            "verify",
            "--root",
            ".",
            "--before",
            "art/arr.json",
            "--after",
            "art/arr.json",
            "--json",
        ],
        "agent verify before artifact is not a canonical repo-exposure artifact: the file holds a JSON array; expected a repo-exposure JSON object (from `ripr pilot` or `ripr check --format repo-exposure-json`). Regenerate the before snapshot with the current ripr (`ripr agent verify --help` shows the loop)",
    );
    let _ = fs::remove_dir_all(&dir);
    result
}

#[test]
fn directory_inputs_name_the_flag_and_the_file_to_pass() -> Result<(), String> {
    let dir = workspace("dirs")?;
    let outcome = assert_refusal(
        &dir,
        &[
            "outcome",
            "--before",
            "art/adir",
            "--after",
            "art/file.json",
        ],
        "outcome --before art/adir is a directory, not a JSON file; pass the file itself. Run `ripr check --format repo-exposure-json > snapshot.json` first, or pass a `ripr check --json` output.",
    );
    let calibrate = assert_refusal(
        &dir,
        &[
            "calibrate",
            "cargo-mutants",
            "--mutants-json",
            "art/adir",
            "--repo-exposure-json",
            "art/file.json",
        ],
        "calibrate cargo-mutants --mutants-json art/adir is a directory but contains neither outcomes.json nor mutants.json; pass the cargo-mutants output directory (usually `mutants.out`) or its `outcomes.json`",
    );
    let repo_exposure = assert_refusal(
        &dir,
        &[
            "calibrate",
            "cargo-mutants",
            "--mutants-json",
            "art/file.json",
            "--repo-exposure-json",
            "art/adir",
        ],
        "calibrate cargo-mutants --repo-exposure-json art/adir is a directory, not a JSON file; pass the file itself. Run `ripr check --format repo-exposure-json > snapshot.json` first.",
    );
    let _ = fs::remove_dir_all(&dir);
    outcome?;
    calibrate?;
    repo_exposure
}

#[test]
fn shared_reader_and_writer_name_a_directory_without_the_host_errno() -> Result<(), String> {
    let dir = workspace("shared-dir")?;
    fs::write(dir.join("art/empty.diff"), "").map_err(|error| format!("write diff: {error}"))?;
    let context = assert_refusal(
        &dir,
        &[
            "context",
            "--diff",
            "art/empty.diff",
            "--from",
            "art/adir",
            "--at",
            "src/lib.rs:1",
        ],
        "check artifact at art/adir could not be read: is a directory, not a file; pass the path of a file",
    );
    let queue = assert_refusal(
        &dir,
        &["swarm", "queue", "--gap-ledger", "art/adir"],
        "swarm queue --gap-ledger art/adir is invalid: is a directory, not a file; pass the path of a file",
    );
    let write = assert_refusal(
        &dir,
        &["feedback", "export", "--out", "art/adir"],
        "failed to finalize usefulness-feedback join art/adir: the destination is a directory, not a file; pass a file path, such as one inside that directory",
    );
    let _ = fs::remove_dir_all(&dir);
    context?;
    queue?;
    write
}

#[test]
fn context_from_a_missing_artifact_names_the_command_that_writes_it() -> Result<(), String> {
    let dir = workspace("context-missing")?;
    fs::write(dir.join("art/empty.diff"), "").map_err(|error| format!("write diff: {error}"))?;
    let result = assert_refusal(
        &dir,
        &[
            "context",
            "--diff",
            "art/empty.diff",
            "--from",
            "art/none.json",
            "--at",
            "src/lib.rs:1",
        ],
        "check artifact at art/none.json could not be read: not found; write it with `ripr check --write-artifact art/none.json` using this run's root, scope (--diff or --base) and analysis options, then rerun",
    );
    let _ = fs::remove_dir_all(&dir);
    result
}

#[test]
fn cache_export_missing_root_names_the_flag() -> Result<(), String> {
    let dir = workspace("cache-root")?;
    let result = assert_refusal(
        &dir,
        &["cache", "export", "--root", "nope", "--out", "fresh"],
        "cache export root nope not found; pass the workspace root with --root",
    );
    let _ = fs::remove_dir_all(&dir);
    result
}
