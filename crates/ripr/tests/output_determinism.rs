//! Rendered-output determinism through the `ripr` binary (#14230).
//!
//! `verdict_invariants.rs` compares findings in process. This test compares
//! the bytes users and agents actually read: for each enabled language it
//! writes one workspace with several independently changed owners, runs
//! `ripr check --diff` in several formats, and requires byte-identical stdout
//! across
//!
//! 1. a cold run on one worker (`RAYON_NUM_THREADS=1`), each format on a
//!    freshly written tree,
//! 2. a warm repeat on four workers over the same tree and report state,
//! 3. a cold run on four workers with the tree deleted and rewritten at the
//!    same path in reverse file order (creation-ordered filesystems then
//!    list it differently).
//!
//! Each run must exit successfully with non-empty output (`missed-txt`
//! included: every workspace has weakly tested owners), and the baseline
//! must name every changed owner, so the relation cannot pass over an empty
//! or misparsed workspace. Formats whose documented contract stamps a
//! per-run time (`check-findings-json`, `repo-*`) are left out of that
//! relation; `check-findings-json` is instead pinned under the opt-in
//! `RIPR_GENERATED_AT` stamp, which makes its bytes repeatable too.

#![cfg(any(
    feature = "lang-rust",
    feature = "lang-python",
    feature = "lang-typescript"
))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const FORMATS: &[&str] = &[
    "human",
    "human-full",
    "json",
    "json-terse",
    "agent",
    "github",
    "sarif",
    "missed-txt",
];

/// Owners per workspace: enough independent probes that a parallel
/// classify pass has work to interleave.
const OWNERS: usize = 6;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Workspace {
    /// (relative path, text) in creation order.
    files: Vec<(String, String)>,
    /// Owner paths the baseline output must name.
    owners: Vec<String>,
}

fn hunk(path: &str, line: usize, before: &str, after: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{line} +{line} @@\n-{before}\n+{after}\n"
    )
}

/// Test strength varies by owner (exact boundary, weak smoke, none) so
/// several finding classes and repair routes are rendered.
#[cfg(feature = "lang-rust")]
fn rust_workspace() -> Workspace {
    let mut files = vec![
        (
            "Cargo.toml".to_string(),
            "[package]\nname = \"seed_rs\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n"
                .to_string(),
        ),
        (
            "ripr.toml".to_string(),
            "[languages]\nenabled = [\"rust\"]\n".to_string(),
        ),
    ];
    let mut lib = String::new();
    let mut diff = String::new();
    let mut owners = Vec::new();
    for k in 0..OWNERS {
        lib.push_str(&format!("pub mod m{k};\n"));
        let limit = (k + 1) * 10;
        let path = format!("src/m{k}.rs");
        files.push((
            path.clone(),
            format!("pub fn over_{k}(n: u32) -> bool {{\n    n >= {limit}\n}}\n"),
        ));
        diff.push_str(&hunk(
            &path,
            2,
            &format!("    n > {limit}"),
            &format!("    n >= {limit}"),
        ));
        let body = match k % 3 {
            0 => format!("    assert_eq!(over_{k}({limit}), true);\n"),
            1 => format!("    assert!(over_{k}(1000));\n"),
            _ => format!("    let _ = over_{k}(0);\n"),
        };
        files.push((
            format!("tests/t{k}.rs"),
            format!("use seed_rs::m{k}::over_{k};\n\n#[test]\nfn checks_{k}() {{\n{body}}}\n"),
        ));
        owners.push(path);
    }
    files.push(("src/lib.rs".to_string(), lib));
    files.push(("change.diff".to_string(), diff));
    Workspace { files, owners }
}

#[cfg(feature = "lang-python")]
fn python_workspace() -> Workspace {
    let mut files = vec![(
        "ripr.toml".to_string(),
        "[languages]\nenabled = [\"python\"]\n".to_string(),
    )];
    let mut diff = String::new();
    let mut owners = Vec::new();
    for k in 0..OWNERS {
        let limit = (k + 1) * 10;
        let path = format!("src/m{k}.py");
        files.push((
            path.clone(),
            format!("def over_{k}(n):\n    return n >= {limit}\n"),
        ));
        diff.push_str(&hunk(
            &path,
            2,
            &format!("    return n > {limit}"),
            &format!("    return n >= {limit}"),
        ));
        let body = match k % 3 {
            0 => format!("    assert over_{k}({limit}) == True\n"),
            1 => format!("    assert over_{k}(1000)\n"),
            _ => format!("    over_{k}(0)\n"),
        };
        files.push((
            format!("tests/test_m{k}.py"),
            format!("from src.m{k} import over_{k}\n\n\ndef test_checks_{k}():\n{body}"),
        ));
        owners.push(path);
    }
    files.push(("change.diff".to_string(), diff));
    Workspace { files, owners }
}

#[cfg(feature = "lang-typescript")]
fn typescript_workspace() -> Workspace {
    let mut files = vec![(
        "ripr.toml".to_string(),
        "[languages]\nenabled = [\"typescript\"]\n".to_string(),
    )];
    let mut diff = String::new();
    let mut owners = Vec::new();
    for k in 0..OWNERS {
        let limit = (k + 1) * 10;
        let path = format!("src/m{k}.ts");
        files.push((
            path.clone(),
            format!("export function over{k}(n: number): boolean {{\n  return n >= {limit};\n}}\n"),
        ));
        diff.push_str(&hunk(
            &path,
            2,
            &format!("  return n > {limit};"),
            &format!("  return n >= {limit};"),
        ));
        let body = match k % 3 {
            0 => format!("  expect(over{k}({limit})).toBe(true);\n"),
            1 => format!("  expect(over{k}(1000)).toBeTruthy();\n"),
            _ => format!("  over{k}(0);\n"),
        };
        files.push((
            format!("tests/m{k}.test.ts"),
            format!(
                "import {{ expect, test }} from 'vitest';\nimport {{ over{k} }} from '../src/m{k}';\n\ntest('checks {k}', () => {{\n{body}}});\n"
            ),
        ));
        owners.push(path);
    }
    files.push(("change.diff".to_string(), diff));
    Workspace { files, owners }
}

fn write_tree(root: &Path, files: &[(String, String)], reversed: bool) -> Result<(), String> {
    if root.exists() {
        std::fs::remove_dir_all(root).map_err(|err| format!("clear {}: {err}", root.display()))?;
    }
    std::fs::create_dir_all(root).map_err(|err| format!("create {}: {err}", root.display()))?;
    let ordered: Vec<&(String, String)> = if reversed {
        files.iter().rev().collect()
    } else {
        files.iter().collect()
    };
    for (path, text) in ordered {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("mkdir {path}: {err}"))?;
        }
        std::fs::write(&full, text).map_err(|err| format!("write {path}: {err}"))?;
    }
    Ok(())
}

fn check(root: &Path, format: &str, threads: &str) -> Result<Vec<u8>, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            format,
        ])
        .env("RAYON_NUM_THREADS", threads)
        .env_remove("RIPR_LOG")
        // Keep the run cache under the scratch root so a fresh tree is cold.
        .env_remove("RIPR_CACHE_DIR")
        .output()
        .map_err(|err| format!("spawn ripr: {err}"))?;
    // `check` exits 1 when it reports gaps; only a crash or usage error is
    // a failed run here.
    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(format!(
            "ripr check --format {format} exited {:?}:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

/// The human digest prints the full Evidence legend once per report tree
/// and a one-line pointer afterwards (#10481). That is intended first-run
/// state, not drift, so each fresh tree takes one discarded human run
/// before its outputs are compared.
fn prime_first_run_state(root: &Path) -> Result<(), String> {
    check(root, "human", "1").map(|_| ())
}

/// Writes the tree from scratch (dropping any report or cache state under
/// it) and runs one format there. Only `human` is primed first, so every
/// other format's output comes from a cold run.
fn fresh_check(
    root: &Path,
    files: &[(String, String)],
    reversed: bool,
    format: &str,
    threads: &str,
) -> Result<Vec<u8>, String> {
    write_tree(root, files, reversed)?;
    if format == "human" {
        prime_first_run_state(root)?;
    }
    check(root, format, threads)
}

fn first_difference(left: &[u8], right: &[u8]) -> String {
    let left = String::from_utf8_lossy(left);
    let right = String::from_utf8_lossy(right);
    for (index, (a, b)) in left.lines().zip(right.lines()).enumerate() {
        if a != b {
            return format!("line {}:\n  - {a}\n  + {b}", index + 1);
        }
    }
    format!(
        "one output is a prefix of the other ({} vs {} lines)",
        left.lines().count(),
        right.lines().count()
    )
}

fn assert_deterministic(name: &str, workspace: &Workspace) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-output-determinism-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _scratch = Scratch(root.clone());

    // 1. Cold, one worker: each format on a freshly written tree.
    let mut baselines = Vec::new();
    for format in FORMATS {
        baselines.push(fresh_check(&root, &workspace.files, false, format, "1")?);
    }
    // Baseline sanity: the JSON report names every changed owner, and every
    // format (including the gap-only `missed-txt`) printed something.
    let json_index = FORMATS
        .iter()
        .position(|format| *format == "json")
        .ok_or("json format missing from FORMATS")?;
    let json = String::from_utf8_lossy(&baselines[json_index]);
    for owner in &workspace.owners {
        if !json.contains(owner.as_str()) {
            return Err(format!(
                "{name}: baseline json does not name changed owner {owner}; the workspace did not analyze:\n{json}"
            ));
        }
    }
    for (format, bytes) in FORMATS.iter().zip(&baselines) {
        if bytes.is_empty() {
            return Err(format!("{name}: --format {format} printed nothing"));
        }
    }

    // 2. Warm, four workers: the tree and its report state from step 1's
    // last run, with first-run state primed for `human`.
    prime_first_run_state(&root)?;
    for format_index in 0..FORMATS.len() {
        let format = FORMATS[format_index];
        let warm = check(&root, format, "4")?;
        if warm != baselines[format_index] {
            return Err(format!(
                "{name} --format {format}: warm 4-worker run differs from cold 1-worker run at {}",
                first_difference(&baselines[format_index], &warm)
            ));
        }
    }

    // 3. Cold, four workers, tree written in reverse order.
    for format_index in 0..FORMATS.len() {
        let format = FORMATS[format_index];
        let reversed = fresh_check(&root, &workspace.files, true, format, "4")?;
        if reversed != baselines[format_index] {
            return Err(format!(
                "{name} --format {format}: cold 4-worker run on a reverse-written tree differs at {}",
                first_difference(&baselines[format_index], &reversed)
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn rust_check_output_is_byte_identical_across_runs_workers_and_write_order() -> Result<(), String> {
    assert_deterministic("rust", &rust_workspace())
}

#[cfg(feature = "lang-python")]
#[test]
fn python_check_output_is_byte_identical_across_runs_workers_and_write_order() -> Result<(), String>
{
    assert_deterministic("python", &python_workspace())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_check_output_is_byte_identical_across_runs_workers_and_write_order()
-> Result<(), String> {
    assert_deterministic("typescript", &typescript_workspace())
}

/// One `check-findings-json` run on a freshly written tree, with
/// `RIPR_GENERATED_AT` set to `stamp` or removed for `None`.
#[cfg(feature = "lang-rust")]
fn findings_artifact(
    root: &Path,
    files: &[(String, String)],
    stamp: Option<&str>,
) -> Result<std::process::Output, String> {
    write_tree(root, files, false)?;
    stamped_ripr(
        root,
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            "check-findings-json",
        ],
        stamp,
    )
}

/// The value of the artifact's `generated_at` member.
#[cfg(feature = "lang-rust")]
fn generated_at(stdout: &[u8]) -> Option<String> {
    const KEY: &str = "\"generated_at\":\"";
    let text = String::from_utf8_lossy(stdout);
    let start = text.find(KEY)? + KEY.len();
    let end = text[start..].find('"')?;
    Some(text[start..start + end].to_string())
}

/// `RIPR_GENERATED_AT` pins the one field that made `check-findings-json`
/// differ between runs, so the artifact and the digest covering it repeat
/// (#14230). A different pin changes the bytes, the default still reads the
/// clock, and a value that is not `unix_ms:<millis>` is refused instead of
/// being replaced by the clock.
#[cfg(feature = "lang-rust")]
#[test]
fn rust_check_findings_json_repeats_under_a_pinned_generated_at() -> Result<(), String> {
    const PIN: &str = "unix_ms:1778179200000";
    const OTHER_PIN: &str = "unix_ms:1778179200001";
    let workspace = rust_workspace();
    let root = std::env::temp_dir().join(format!(
        "ripr-output-determinism-pinned-stamp-{}",
        std::process::id()
    ));
    let _scratch = Scratch(root.clone());

    let first = findings_artifact(&root, &workspace.files, Some(PIN))?;
    if !matches!(first.status.code(), Some(0 | 1)) {
        return Err(format!(
            "pinned run exited {:?}:\n{}",
            first.status.code(),
            String::from_utf8_lossy(&first.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&first.stdout);
    for owner in &workspace.owners {
        if !text.contains(owner.as_str()) {
            return Err(format!(
                "pinned artifact does not name changed owner {owner}:\n{text}"
            ));
        }
    }
    if generated_at(&first.stdout).as_deref() != Some(PIN) {
        return Err(format!(
            "pinned artifact must carry generated_at {PIN}, got {:?}",
            generated_at(&first.stdout)
        ));
    }

    let second = findings_artifact(&root, &workspace.files, Some(PIN))?;
    if second.stdout != first.stdout {
        return Err(format!(
            "two runs under the same pin differ at {}",
            first_difference(&first.stdout, &second.stdout)
        ));
    }

    let other = findings_artifact(&root, &workspace.files, Some(OTHER_PIN))?;
    if other.stdout == first.stdout {
        return Err("a different pin must change the artifact bytes".to_string());
    }

    let unpinned = findings_artifact(&root, &workspace.files, None)?;
    let own_stamp = generated_at(&unpinned.stdout);
    if !own_stamp
        .as_deref()
        .is_some_and(|stamp| stamp.starts_with("unix_ms:") && stamp != PIN)
    {
        return Err(format!(
            "an unpinned run must stamp its own time, got {own_stamp:?}"
        ));
    }

    let refused = findings_artifact(&root, &workspace.files, Some("1778179200000"))?;
    // The refusal is the ordinary machine-readable one: exit 2, a
    // `refused` document without an artifact envelope, and the reason on
    // stderr.
    let stderr = String::from_utf8_lossy(&refused.stderr);
    let document = String::from_utf8_lossy(&refused.stdout);
    if refused.status.code() != Some(2)
        || !document.contains("\"status\": \"refused\"")
        || document.contains("\"artifact\"")
        || generated_at(&refused.stdout).is_some()
        || !stderr.contains("RIPR_GENERATED_AT must be unix_ms:")
    {
        return Err(format!(
            "a malformed pin must be refused with no artifact; exit {:?}, stdout:\n{document}\nstderr:\n{stderr}",
            refused.status.code()
        ));
    }
    Ok(())
}

/// `ripr` with `RIPR_GENERATED_AT` set to `stamp` or removed for `None`.
fn stamped_ripr(
    cwd: &Path,
    args: &[&str],
    stamp: Option<&str>,
) -> Result<std::process::Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .current_dir(cwd)
        .args(args)
        .env_remove("RIPR_LOG")
        .env_remove("RIPR_CACHE_DIR");
    match stamp {
        Some(stamp) => command.env("RIPR_GENERATED_AT", stamp),
        None => command.env_remove("RIPR_GENERATED_AT"),
    };
    command.output().map_err(|err| format!("spawn ripr: {err}"))
}

/// A stamped command that must exit 0 and leave `written` on disk; returns
/// the written bytes.
fn stamped_write(
    cwd: &Path,
    args: &[&str],
    stamp: Option<&str>,
    written: &Path,
) -> Result<Vec<u8>, String> {
    let _ = std::fs::remove_file(written);
    let output = stamped_ripr(cwd, args, stamp)?;
    if output.status.code() != Some(0) {
        return Err(format!(
            "ripr {args:?} under {stamp:?} exited {:?}:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::read(written).map_err(|err| format!("read {}: {err}", written.display()))
}

/// A malformed pin must stop the command before it writes `written`.
fn assert_stamp_refused(
    cwd: &Path,
    args: &[&str],
    stamp: &str,
    written: &Path,
    reason: &str,
) -> Result<(), String> {
    let _ = std::fs::remove_file(written);
    let output = stamped_ripr(cwd, args, Some(stamp))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(2) || !stderr.contains(reason) || written.exists() {
        return Err(format!(
            "ripr {args:?} under {stamp:?} must exit 2 naming `{reason}` and write nothing; exit {:?}, wrote: {}, stderr:\n{stderr}",
            output.status.code(),
            written.exists()
        ));
    }
    Ok(())
}

fn fixture_git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=determinism",
            "-c",
            "user.email=determinism@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .map_err(|err| format!("spawn git: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// `RIPR_GENERATED_AT` pins a receipt's `written_at`, the one field that
/// made two writes of the same receipt differ (#14230). The pin is truncated
/// to the receipt's whole seconds, a different second changes the bytes, the
/// default still reads the clock, and a malformed or unspellable pin is
/// refused with no receipt written.
#[test]
fn receipt_write_repeats_under_a_pinned_generated_at() -> Result<(), String> {
    const PIN: &str = "unix_ms:1778179200999";
    const PINNED_WRITTEN_AT: &str = "\"written_at\": \"2026-05-07T18:40:00Z\"";
    const NEXT_SECOND: &str = "unix_ms:1778179201000";
    let root = std::env::temp_dir().join(format!(
        "ripr-output-determinism-receipt-stamp-{}",
        std::process::id()
    ));
    let _scratch = Scratch(root.clone());
    std::fs::create_dir_all(&root).map_err(|err| format!("create {}: {err}", root.display()))?;
    std::fs::write(root.join("README.md"), "receipt stamp fixture\n")
        .map_err(|err| format!("write fixture: {err}"))?;
    fixture_git(&root, &["init", "-q", "-b", "main"])?;
    fixture_git(&root, &["add", "-A"])?;
    fixture_git(&root, &["commit", "-q", "-m", "base"])?;
    let receipt = root.join("receipt.json");
    let args = [
        "receipt",
        "write",
        "--gap",
        "gap:rust:src/lib.rs:total:predicate_boundary:predicate:a==1",
        "--verify-command",
        "cargo test",
        "--status",
        "passed",
        "--out",
        "receipt.json",
    ];

    let first = stamped_write(&root, &args, Some(PIN), &receipt)?;
    let text = String::from_utf8_lossy(&first);
    if !text.contains(PINNED_WRITTEN_AT) || !text.contains("\"verify_command\": \"cargo test\"") {
        return Err(format!(
            "pinned receipt must carry {PINNED_WRITTEN_AT} and the verify command:\n{text}"
        ));
    }
    let second = stamped_write(&root, &args, Some(PIN), &receipt)?;
    if second != first {
        return Err(format!(
            "two receipts under the same pin differ at {}",
            first_difference(&first, &second)
        ));
    }
    if stamped_write(&root, &args, Some(NEXT_SECOND), &receipt)? == first {
        return Err("a pin in a different second must change the receipt bytes".to_string());
    }
    let unpinned = stamped_write(&root, &args, None, &receipt)?;
    let unpinned_text = String::from_utf8_lossy(&unpinned);
    if !unpinned_text.contains("\"written_at\": \"") || unpinned_text.contains(PINNED_WRITTEN_AT) {
        return Err(format!(
            "an unpinned receipt must stamp its own time:\n{unpinned_text}"
        ));
    }

    assert_stamp_refused(
        &root,
        &args,
        "2026-05-07T18:40:00Z",
        &receipt,
        "RIPR_GENERATED_AT must be unix_ms:",
    )?;
    // 10000-01-01T00:00:00Z: `written_at` has four year digits.
    assert_stamp_refused(
        &root,
        &args,
        "unix_ms:253402300800000",
        &receipt,
        "past year 9999",
    )
}

/// The report commands stamp `generated_at` through one helper; `first-action`
/// stands for them. Under the pin its JSON repeats byte for byte and carries
/// the pinned stamp, the default still reads the clock, and a malformed pin
/// is refused with no report written (#14230).
#[test]
fn first_action_report_repeats_under_a_pinned_generated_at() -> Result<(), String> {
    const PIN: &str = "unix_ms:1778179200000";
    const PINNED_GENERATED_AT: &str = "\"generated_at\": \"unix_ms:1778179200000\"";
    let root = std::env::temp_dir().join(format!(
        "ripr-output-determinism-report-stamp-{}",
        std::process::id()
    ));
    let _scratch = Scratch(root.clone());
    std::fs::create_dir_all(&root).map_err(|err| format!("create {}: {err}", root.display()))?;
    let guidance = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/boundary_gap/expected/pr-guidance/exact-line/comments.json");
    let guidance = guidance
        .to_str()
        .ok_or_else(|| format!("fixture path is not UTF-8: {}", guidance.display()))?;
    let report = root.join("first-useful-action.json");
    let args = [
        "first-action",
        "--root",
        ".",
        "--pr-guidance",
        guidance,
        "--out",
        "first-useful-action.json",
        "--out-md",
        "first-useful-action.md",
    ];

    let first = stamped_write(&root, &args, Some(PIN), &report)?;
    let text = String::from_utf8_lossy(&first);
    // The report must have read the guidance, not only written an envelope.
    if !text.contains(PINNED_GENERATED_AT) || !text.contains("src/pricing.rs") {
        return Err(format!(
            "pinned report must carry {PINNED_GENERATED_AT} and the guided file:\n{text}"
        ));
    }
    let markdown = root.join("first-useful-action.md");
    let read_markdown =
        || std::fs::read(&markdown).map_err(|err| format!("read {}: {err}", markdown.display()));
    let first_markdown = read_markdown()?;
    let second = stamped_write(&root, &args, Some(PIN), &report)?;
    if second != first {
        return Err(format!(
            "two reports under the same pin differ at {}",
            first_difference(&first, &second)
        ));
    }
    let second_markdown = read_markdown()?;
    if second_markdown != first_markdown {
        return Err(format!(
            "two Markdown reports under the same pin differ at {}",
            first_difference(&first_markdown, &second_markdown)
        ));
    }
    let unpinned = stamped_write(&root, &args, None, &report)?;
    let unpinned_text = String::from_utf8_lossy(&unpinned);
    if !unpinned_text.contains("\"generated_at\": \"unix_ms:")
        || unpinned_text.contains(PINNED_GENERATED_AT)
    {
        return Err(format!(
            "an unpinned report must stamp its own time:\n{unpinned_text}"
        ));
    }
    std::fs::remove_file(&markdown)
        .map_err(|err| format!("remove {}: {err}", markdown.display()))?;
    assert_stamp_refused(
        &root,
        &args,
        "1778179200000",
        &report,
        "RIPR_GENERATED_AT must be unix_ms:",
    )?;
    if markdown.exists() {
        return Err("a refused pin must not write the Markdown report".to_string());
    }
    Ok(())
}

/// `ripr zero status` judges review deadlines against its own stamp, so the
/// pin must not reach it: a pin from before a review's deadline would report
/// an overdue review as current. The fixture review fell due 2026-08-08.
#[test]
fn zero_status_keeps_the_wall_clock_under_a_pinned_generated_at() -> Result<(), String> {
    // 2026-05-07, three months before the fixture review's deadline.
    const PIN: &str = "unix_ms:1778179200000";
    let root = std::env::temp_dir().join(format!(
        "ripr-output-determinism-zero-status-{}",
        std::process::id()
    ));
    let _scratch = Scratch(root.clone());
    std::fs::create_dir_all(&root).map_err(|err| format!("create {}: {err}", root.display()))?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/boundary_gap/expected/baseline-debt-delta/mixed");
    let baseline = fixture.join("baseline.json");
    let delta = fixture.join("baseline-debt-delta.json");
    let (Some(baseline), Some(delta)) = (baseline.to_str(), delta.to_str()) else {
        return Err(format!("fixture path is not UTF-8: {}", fixture.display()));
    };
    let report = root.join("zero-status.json");
    let args = [
        "zero",
        "status",
        "--baseline",
        baseline,
        "--delta",
        delta,
        "--out",
        "zero-status.json",
        "--out-md",
        "zero-status.md",
    ];
    let pinned = stamped_write(&root, &args, Some(PIN), &report)?;
    let text = String::from_utf8_lossy(&pinned);
    if text.contains(PIN) || !text.contains("1 baseline entries have stale review metadata") {
        return Err(format!(
            "zero status under a pin must stamp the run time and keep the overdue review stale:\n{text}"
        ));
    }
    Ok(())
}
