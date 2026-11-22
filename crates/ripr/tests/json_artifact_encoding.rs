//! #15936: a check artifact written by a Windows PowerShell 5.1 redirect is
//! the same artifact.
//!
//! `ripr check --format json > check.json` in Windows PowerShell 5.1 writes
//! UTF-16LE with a byte-order mark, and `Out-File -Encoding utf8` writes a
//! UTF-8 mark. `ripr baseline create --from-check` refused both, and its
//! recovery text told the user to run the same redirect again. Each spelling
//! here must produce the baseline the plain UTF-8 file produces; UTF-16 that
//! cannot be decoded exactly is refused with the encoding named.

#![cfg(feature = "lang-rust")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static WORKSPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The repo's `[env] TEMP=target` redirect puts fixtures inside the outer git
/// worktree; remove the root on scope exit, success or failure.
struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<Output, String> {
    Command::new(program)
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn {program} {args:?} failed: {error}"))
}

fn stdout_of(root: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let output = run(root, program, args)?;
    if !output.status.success() {
        return Err(format!(
            "{program} {args:?} failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("{program} stdout: {error}"))
}

/// A one-crate git workspace whose only test leaves the boundary unchecked,
/// and the whole-tree check artifact for it as UTF-8 text.
fn workspace_and_check_json() -> Result<(PathBuf, String), String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!(
        "ripr-json-artifact-encoding-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir: {error}"))?;
    fs::create_dir_all(root.join("tests")).map_err(|error| format!("mkdir: {error}"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    fs::write(
        root.join("src/lib.rs"),
        "pub fn discount(total: u32) -> u32 {\n    if total >= 100 {\n        total - 10\n    } else {\n        total\n    }\n}\n",
    )
    .map_err(|error| format!("write source: {error}"))?;
    fs::write(
        root.join("tests/pricing.rs"),
        "use pricing::discount;\n\n#[test]\nfn big_orders_get_discount() {\n    assert_eq!(discount(200), 190);\n}\n",
    )
    .map_err(|error| format!("write test: {error}"))?;
    stdout_of(&root, "git", &["init", "--quiet", "."])?;
    stdout_of(&root, "git", &["add", "--all"])?;
    let tree = stdout_of(&root, "git", &["write-tree"])?;
    let check_json = stdout_of(
        &root,
        env!("CARGO_BIN_EXE_ripr"),
        &[
            "check",
            "--root",
            ".",
            "--candidate-tree",
            tree.trim(),
            "--format",
            "json",
            "--quiet",
        ],
    )?;
    Ok((root, check_json))
}

/// Run `baseline create --from-check check.json --dry-run` on `bytes`.
fn baseline_from(root: &Path, bytes: &[u8]) -> Result<Output, String> {
    fs::write(root.join("check.json"), bytes).map_err(|error| format!("write: {error}"))?;
    run(
        root,
        env!("CARGO_BIN_EXE_ripr"),
        &[
            "baseline",
            "create",
            "--from-check",
            "check.json",
            "--dry-run",
        ],
    )
}

/// The identity of each printed ledger entry. The entries also carry the
/// run time, which differs between runs.
fn ledger_entries(name: &str, output: &Output) -> Result<Vec<serde_json::Value>, String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "{name}: baseline create failed\n{stdout}\n{stderr}"
        ));
    }
    let document: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("{name}: ledger is not JSON: {error}\n{stdout}\n{stderr}"))?;
    document
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{name}: ledger has no entries array\n{stdout}"))?
        .iter()
        .map(|entry| {
            entry
                .get("identity")
                .cloned()
                .ok_or_else(|| format!("{name}: entry has no identity\n{stdout}"))
        })
        .collect()
}

/// `text` as UTF-16LE with a byte-order mark and CRLF line ends.
fn powershell_redirect_bytes(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.replace('\n', "\r\n").encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

#[test]
fn a_check_artifact_in_a_powershell_encoding_gives_the_utf8_baseline() -> Result<(), String> {
    let (root, check_json) = workspace_and_check_json()?;
    let _guard = FixtureGuard { root: root.clone() };

    // Control: the UTF-8 artifact baselines at least one gap.
    let utf8 = ledger_entries("utf8", &baseline_from(&root, check_json.as_bytes())?)?;
    if utf8.is_empty() {
        return Err(format!(
            "the UTF-8 control baselined no entry\n{check_json}"
        ));
    }

    let mut utf8_with_mark = vec![0xEF, 0xBB, 0xBF];
    utf8_with_mark.extend(check_json.as_bytes());
    let mut failures = Vec::new();
    for (name, bytes) in [
        ("UTF-8 with a byte-order mark", utf8_with_mark),
        ("UTF-16LE redirect", powershell_redirect_bytes(&check_json)),
    ] {
        match baseline_from(&root, &bytes).and_then(|output| ledger_entries(name, &output)) {
            Ok(entries) if entries == utf8 => {}
            Ok(entries) => failures.push(format!("{name}: entries differ: {entries:?}")),
            Err(error) => failures.push(error),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn a_truncated_utf16_check_artifact_is_refused_by_name() -> Result<(), String> {
    let (root, check_json) = workspace_and_check_json()?;
    let _guard = FixtureGuard { root: root.clone() };

    let mut truncated = powershell_redirect_bytes(&check_json);
    truncated.pop();
    let output = baseline_from(&root, &truncated)?;
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        return Err(format!("a truncated artifact must not baseline\n{said}"));
    }
    if !said.contains("UTF-16 byte-order mark") {
        return Err(format!("the refusal does not name the encoding\n{said}"));
    }
    Ok(())
}

/// `baseline diff --baseline baseline.json --current current.json`, where
/// each file holds `baseline` or `current` in its own encoding. Returns the
/// exit code, the `Items:` line and stderr.
fn baseline_diff_of(
    root: &Path,
    baseline: &[u8],
    current: &[u8],
) -> Result<(Option<i32>, String, String), String> {
    fs::write(root.join("baseline.json"), baseline).map_err(|error| format!("write: {error}"))?;
    fs::write(root.join("current.json"), current).map_err(|error| format!("write: {error}"))?;
    let output = run(
        root,
        env!("CARGO_BIN_EXE_ripr"),
        &[
            "baseline",
            "diff",
            "--baseline",
            "baseline.json",
            "--current",
            "current.json",
            "--out",
            "delta.json",
            "--out-md",
            "delta.md",
        ],
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let items = stdout
        .lines()
        .find(|line| line.starts_with("Items:"))
        .unwrap_or_default()
        .to_string();
    Ok((
        output.status.code(),
        items,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

/// `baseline diff` reads its two JSON flags through the report-input reader,
/// a different path from `baseline create` (Devin review on #16142). Each
/// spelling of either input must compare the way the UTF-8 pair does.
#[test]
fn baseline_diff_reads_both_inputs_in_a_powershell_encoding() -> Result<(), String> {
    let (root, check_json) = workspace_and_check_json()?;
    let _guard = FixtureGuard { root: root.clone() };
    let baseline_output = baseline_from(&root, check_json.as_bytes())?;
    if !baseline_output.status.success() {
        return Err(format!(
            "baseline create failed\n{}",
            String::from_utf8_lossy(&baseline_output.stderr)
        ));
    }
    let baseline = String::from_utf8(baseline_output.stdout)
        .map_err(|error| format!("baseline stdout: {error}"))?;
    let current = "{}\n";

    let control = baseline_diff_of(&root, baseline.as_bytes(), current.as_bytes())?;
    if control.2.contains("could not read") {
        return Err(format!("the UTF-8 control was refused\n{}", control.2));
    }

    let with_mark = |text: &str| {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(text.as_bytes());
        bytes
    };
    let mut failures = Vec::new();
    for (name, baseline_bytes, current_bytes) in [
        (
            "UTF-8 marked baseline",
            with_mark(&baseline),
            current.as_bytes().to_vec(),
        ),
        (
            "UTF-16LE baseline",
            powershell_redirect_bytes(&baseline),
            current.as_bytes().to_vec(),
        ),
        (
            "UTF-8 marked current",
            baseline.as_bytes().to_vec(),
            with_mark(current),
        ),
        (
            "UTF-16LE current",
            baseline.as_bytes().to_vec(),
            powershell_redirect_bytes(current),
        ),
    ] {
        let got = baseline_diff_of(&root, &baseline_bytes, &current_bytes)?;
        if got.0 != control.0 || got.1 != control.1 || got.2.contains("could not read") {
            failures.push(format!(
                "{name}: exit {:?} {:?}, UTF-8 gave exit {:?} {:?}\n{}",
                got.0, got.1, control.0, control.1, got.2
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
