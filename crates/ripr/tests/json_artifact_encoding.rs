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
