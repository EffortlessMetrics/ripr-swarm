//! #14072: text after a diff's last complete hunk is not hunk body.
//!
//! `ripr check --diff` refused a valid patch as `unsupported_input` with a
//! `malformed_diff` limitation when the file ended in a blank line, and for
//! `git format-patch` output it also reported findings at lines taken from
//! the mail signature and the next message's headers. Each variant here must
//! produce the same typed outcome and the same findings as the bare patch.

#![cfg(feature = "lang-rust")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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

const MAIL_HEAD: &str = "\
From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001
From: A U Thor <author@example.invalid>
Date: Mon, 1 Jan 2024 00:00:00 +0000
Subject: [PATCH] discount at the boundary

- applies at exactly 100
---
 src/lib.rs | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

";

const MAIL_SIGNATURE: &str = "-- \n2.43.0\n\n";

fn write_workspace() -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!(
        "ripr-diff-trailing-text-{}-{call}",
        std::process::id()
    ));
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
    Ok(root)
}

/// The typed outcome kind, its limitation kinds, and each finding's
/// `(file, line, classification)`.
type Observed = (String, Vec<String>, Vec<(String, u64, String)>);

fn check_diff(root: &Path, name: &str, diff: &str) -> Result<Observed, String> {
    fs::write(root.join(name), diff).map_err(|error| format!("write {name}: {error}"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args([
            "check", "--root", ".", "--diff", name, "--format", "json", "--quiet",
        ])
        .output()
        .map_err(|error| format!("spawn ripr for {name} failed: {error}"))?;
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

#[test]
fn text_after_the_last_complete_hunk_does_not_change_the_result() -> Result<(), String> {
    let root = write_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };

    // Control: the bare patch is a complete run with the one changed line.
    let bare = check_diff(&root, "bare.diff", PATCH)?;
    let (kind, limitations, findings) = &bare;
    if kind != "complete_with_findings"
        || !limitations.is_empty()
        || findings.len() != 1
        || findings[0].1 != 2
    {
        return Err(format!("bare control is not the expected run: {bare:?}"));
    }

    // The series' second message adds the comment that closes the fixture.
    let series_tail = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -16,0 +17 @@
+// the boundary is inclusive
";
    let variants = [
        ("blank.diff", format!("{PATCH}\n")),
        ("blanks.diff", format!("{PATCH}\n\n\n")),
        ("crlf-blank.diff", format!("{PATCH}\r\n")),
        ("mail.diff", format!("{MAIL_HEAD}{PATCH}{MAIL_SIGNATURE}")),
        ("mail-nosig.diff", format!("{MAIL_HEAD}{PATCH}")),
        (
            "series.diff",
            format!("{MAIL_HEAD}{PATCH}{MAIL_SIGNATURE}{MAIL_HEAD}{series_tail}{MAIL_SIGNATURE}"),
        ),
        (
            "series-nosig.diff",
            format!("{MAIL_HEAD}{PATCH}{MAIL_HEAD}{series_tail}"),
        ),
    ];
    let mut failures = Vec::new();
    for (name, diff) in &variants {
        let observed = check_diff(&root, name, diff)?;
        if observed != bare {
            failures.push(format!("{name}: {observed:?}"));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "text after a complete hunk changed the result (control {bare:?}):\n{}",
            failures.join("\n")
        ));
    }

    // Fail-closed control: a body line beyond the declared counts is still a
    // malformed diff, also behind a blank line.
    let excess = check_diff(&root, "excess.diff", &format!("{PATCH}\n+    excess();\n"))?;
    if excess.0 != "unsupported_input" || excess.1 != ["malformed_diff"] {
        return Err(format!("excess body was accepted: {excess:?}"));
    }
    Ok(())
}
