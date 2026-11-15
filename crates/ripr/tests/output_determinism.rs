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
//! per-run time (`check-findings-json`, `repo-*`) are left out.

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
