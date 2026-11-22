//! #15907: a UTF-16 diff with a byte-order mark is a diff.
//!
//! Windows PowerShell 5.1 writes `git diff > change.diff` as UTF-16LE with a
//! byte-order mark and CRLF line ends. `ripr check --diff` read those bytes
//! as UTF-8, found no diff line, and refused the file as `unsupported_input`.
//! Each encoding here must produce the same typed outcome and the same
//! findings as the UTF-8 patch, from a file and from stdin; UTF-16 that
//! cannot be decoded exactly is refused with the encoding named.

#![cfg(feature = "lang-rust")]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

const PATCH: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 0000000..1111111 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,5 @@
 pub fn discount(total: u32) -> u32 {
-    if total > 100 {
+    if total >= 100 {
         total - 10
     } else {
         total
";

fn write_workspace() -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!("ripr-diff-utf16-{}-{call}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir: {error}"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    fs::write(
        root.join("src/lib.rs"),
        "\
pub fn discount(total: u32) -> u32 {
    if total >= 100 {
        total - 10
    } else {
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small() {
        assert_eq!(discount(5), 5);
    }
}
// the boundary is inclusive
",
    )
    .map_err(|error| format!("write source: {error}"))?;
    fs::write(root.join("README.md"), "demo\nthe boundary is inclusive\n")
        .map_err(|error| format!("write readme: {error}"))?;
    Ok(root)
}

/// The typed outcome kind, its limitation kinds, and each finding's
/// `(file, line, classification)`.
type Observed = (String, Vec<String>, Vec<(String, u64, String)>);

/// Run `ripr check --diff` on `diff`, written to the file `name` or piped on
/// stdin when `from_stdin` is set.
fn run_check(
    root: &Path,
    name: &str,
    diff: &[u8],
    from_stdin: bool,
) -> Result<std::process::Output, String> {
    let diff_arg = if from_stdin { "-" } else { name };
    if !from_stdin {
        fs::write(root.join(name), diff).map_err(|error| format!("write {name}: {error}"))?;
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args([
            "check", "--root", ".", "--diff", diff_arg, "--format", "json", "--quiet",
        ])
        .stdin(if from_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn ripr for {name} failed: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(diff)
            .map_err(|error| format!("{name}: write stdin: {error}"))?;
    }
    child
        .wait_with_output()
        .map_err(|error| format!("{name}: wait for ripr failed: {error}"))
}

fn check_diff(root: &Path, name: &str, diff: &[u8], from_stdin: bool) -> Result<Observed, String> {
    let output = run_check(root, name, diff, from_stdin)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!(
            "{name}: check output is not JSON: {error}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let text = |value: Option<&serde_json::Value>, what: &str| {
        value
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("{name}: missing {what}\nstdout: {stdout}"))
    };
    let kind = text(document.pointer("/analysis_outcome/outcome/kind"), "kind")?;
    let limitations = document
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{name}: missing limitations\nstdout: {stdout}"))?
        .iter()
        .map(|limitation| text(limitation.get("kind"), "limitation kind"))
        .collect::<Result<Vec<_>, _>>()?;
    let findings = document
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{name}: missing findings\nstdout: {stdout}"))?
        .iter()
        .map(|finding| {
            Ok((
                text(finding.pointer("/probe/file"), "probe file")?,
                finding
                    .pointer("/probe/line")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| format!("{name}: missing probe line\nstdout: {stdout}"))?,
                text(finding.get("classification"), "classification")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((kind, limitations, findings))
}

/// `text` as UTF-16 with a byte-order mark and CRLF line ends.
fn utf16(text: &str, bom: [u8; 2], unit_bytes: fn(u16) -> [u8; 2]) -> Vec<u8> {
    let mut bytes = bom.to_vec();
    for unit in text.replace('\n', "\r\n").encode_utf16() {
        bytes.extend(unit_bytes(unit));
    }
    bytes
}

#[test]
fn a_utf16_diff_with_a_byte_order_mark_gives_the_utf8_result() -> Result<(), String> {
    let root = write_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };

    // Control: the UTF-8 patch is a complete run with the one changed line.
    let utf8 = check_diff(&root, "utf8.diff", PATCH.as_bytes(), false)?;
    let (kind, limitations, findings) = &utf8;
    if kind != "complete_with_findings"
        || !limitations.is_empty()
        || findings.len() != 1
        || findings[0].1 != 2
    {
        return Err(format!("UTF-8 control is not the expected run: {utf8:?}"));
    }

    let little = utf16(PATCH, [0xFF, 0xFE], u16::to_le_bytes);
    let big = utf16(PATCH, [0xFE, 0xFF], u16::to_be_bytes);
    let mut failures = Vec::new();
    for (name, diff, from_stdin) in [
        ("powershell-5.1.diff", &little, false),
        ("powershell-5.1 on stdin", &little, true),
        ("utf16-be.diff", &big, false),
    ] {
        let observed = check_diff(&root, name, diff, from_stdin)?;
        if observed != utf8 {
            failures.push(format!("{name}: {observed:?}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "UTF-16 input changed the result\nUTF-8: {utf8:?}\n{}",
            failures.join("\n")
        ))
    }
}

#[test]
fn utf16_that_cannot_be_decoded_exactly_is_refused_by_name() -> Result<(), String> {
    let root = write_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };

    // One byte short: the last UTF-16 unit is incomplete.
    let mut truncated = utf16(PATCH, [0xFF, 0xFE], u16::to_le_bytes);
    truncated.pop();
    let output = run_check(&root, "truncated.diff", &truncated, false)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let said = format!("{stdout}\n{stderr}");
    if output.status.success() {
        return Err(format!("a truncated UTF-16 diff must not succeed\n{said}"));
    }
    for needle in ["UTF-16 byte-order mark", "git diff --output=<file>"] {
        if !said.contains(needle) {
            return Err(format!("the refusal does not say {needle:?}\n{said}"));
        }
    }
    if stdout.contains("complete_with_findings") || stdout.contains("\"complete\"") {
        return Err(format!("the refusal reads as a complete run\n{said}"));
    }
    Ok(())
}
