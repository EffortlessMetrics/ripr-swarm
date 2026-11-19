//! #14175: a changed line whose code is unchanged seeds no finding.
//!
//! Adding or rewording a trailing `//` comment, rewriting a file's line
//! endings, and adding or removing the final newline all leave the program
//! text as it was, yet `ripr check` reported the touched lines as changed
//! behavior, including an actionable missing-boundary finding. Each case runs
//! beside a control that changes code on the same line and must keep its
//! finding.

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

const LINES: [&str; 16] = [
    "pub fn discount(total: u32) -> u32 {",
    "    if total > 100 {",
    "        total - 10",
    "    } else {",
    "        total",
    "    }",
    "}",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    use super::*;",
    "    #[test]",
    "    fn small() {",
    "        assert_eq!(discount(5), 5);",
    "    }",
    "}",
];

fn write_workspace(source: &str) -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!(
        "ripr-unchanged-code-edits-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir: {error}"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    fs::write(root.join("src/lib.rs"), source).map_err(|error| format!("write source: {error}"))?;
    Ok(root)
}

/// The typed outcome kind, its limitation kinds, and each finding's
/// `(line, classification)`.
type Observed = (String, Vec<String>, Vec<(u64, String)>);

/// Run `ripr check --diff` over a workspace whose `src/lib.rs` is `source`
/// (the diff's post-image).
fn check(source: &str, diff_body: &str) -> Result<Observed, String> {
    let root = write_workspace(source)?;
    let _guard = FixtureGuard { root: root.clone() };
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n{diff_body}"
    );
    fs::write(root.join("change.diff"), diff).map_err(|error| format!("write diff: {error}"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(Path::new(&root))
        .args([
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .map_err(|error| format!("spawn ripr failed: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!(
            "check output is not JSON: {error}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let text = |value: Option<&serde_json::Value>, what: &str| {
        value
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("missing {what}\nstdout: {stdout}"))
    };
    let kind = text(document.pointer("/analysis_outcome/outcome/kind"), "kind")?;
    let limitations = document
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("missing limitations\nstdout: {stdout}"))?
        .iter()
        .map(|limitation| text(limitation.get("kind"), "limitation kind"))
        .collect::<Result<Vec<_>, _>>()?;
    let findings = document
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("missing findings\nstdout: {stdout}"))?
        .iter()
        .map(|finding| {
            Ok((
                finding
                    .pointer("/probe/line")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| format!("missing probe line\nstdout: {stdout}"))?,
                text(finding.get("classification"), "classification")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((kind, limitations, findings))
}

fn source_with(line_two: &str, ending: &str, final_newline: bool) -> String {
    let mut lines = LINES.to_vec();
    lines[1] = line_two;
    let mut source = lines.join(ending);
    if final_newline {
        source.push_str(ending);
    }
    source
}

/// A one-line replacement of line 2.
fn line_two_hunk(removed: &str, added: &str) -> String {
    format!("@@ -2 +2 @@\n-{removed}\n+{added}\n")
}

fn expect_no_findings(label: &str, observed: &Observed, limitations: &[&str]) -> Option<String> {
    (observed.0 != "no_behavioral_candidates"
        || observed.1 != limitations
        || !observed.2.is_empty())
    .then(|| format!("{label}: unchanged code was reported: {observed:?}"))
}

fn expect_line_two_only(label: &str, observed: &Observed) -> Option<String> {
    (observed.0 != "complete_with_findings"
        || observed.2.len() != 1
        || observed.2[0] != (2, "weakly_exposed".to_string()))
        .then(|| format!("{label}: the code edit on line 2 was not the one finding: {observed:?}"))
}

#[test]
fn trailing_comment_edits_report_nothing_and_code_edits_beside_them_still_do() -> Result<(), String>
{
    let plain = "    if total > 100 {";
    let commented = "    if total > 100 { // big orders";
    let reworded = "    if total > 100 { // large orders";
    let mut failures = Vec::new();
    for (label, removed, added) in [
        ("comment added", plain, commented),
        ("comment removed", commented, plain),
        ("comment reworded", commented, reworded),
    ] {
        let observed = check(
            &source_with(added, "\n", true),
            &line_two_hunk(removed, added),
        )?;
        failures.extend(expect_no_findings(label, &observed, &[]));
    }
    // Controls: the comparison changes too, with a comment added or kept.
    for (label, removed, added) in [
        (
            "code and comment",
            plain,
            "    if total >= 100 { // big orders",
        ),
        (
            "code beside a kept comment",
            commented,
            "    if total >= 100 { // big orders",
        ),
    ] {
        let observed = check(
            &source_with(added, "\n", true),
            &line_two_hunk(removed, added),
        )?;
        failures.extend(expect_line_two_only(label, &observed));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn line_ending_only_rewrites_report_nothing_and_an_edit_inside_one_still_does() -> Result<(), String>
{
    let whole_file = |removed_line_two: &str, added_line_two: &str, added_ending: &str| {
        let mut body = String::from("@@ -1,16 +1,16 @@\n");
        for (at, line) in LINES.iter().enumerate() {
            let line = if at == 1 { removed_line_two } else { line };
            body.push_str(&format!("-{line}\n"));
        }
        for (at, line) in LINES.iter().enumerate() {
            let line = if at == 1 { added_line_two } else { line };
            body.push_str(&format!("+{line}{added_ending}\n"));
        }
        body
    };
    let mut failures = Vec::new();

    // LF -> CRLF across the whole file: every line is removed and added.
    let churn = check(
        &source_with(LINES[1], "\r\n", true),
        &whole_file(LINES[1], LINES[1], "\r"),
    )?;
    failures.extend(expect_no_findings(
        "line endings rewritten",
        &churn,
        &["eol_only_churn"],
    ));

    // The same rewrite with the comparison on line 2 changed as well.
    let edited = "    if total >= 100 {";
    let mixed = check(
        &source_with(edited, "\r\n", true),
        &whole_file(LINES[1], edited, "\r"),
    )?;
    failures.extend(expect_line_two_only(
        "edit inside a line-ending rewrite",
        &mixed,
    ));

    // The final newline removed: git repeats the last line on both sides.
    let final_newline = check(
        &source_with(LINES[1], "\n", false),
        "@@ -16 +16 @@\n-}\n+}\n\\ No newline at end of file\n",
    )?;
    failures.extend(expect_no_findings(
        "final newline removed",
        &final_newline,
        &["eol_only_churn"],
    ));

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Equal text is not unchanged code when another edited line moved a comment
/// boundary or turned a comment into a documentation attribute.
#[test]
fn equal_text_that_changed_meaning_keeps_its_finding() -> Result<(), String> {
    let mut failures = Vec::new();

    // A block comment replaced by line comments during an LF -> CRLF rewrite:
    // the statement between them pairs with identical text but is now live.
    let old = [
        "pub fn f() {",
        "    /* disabled",
        "    println!(\"live\");",
        "    */",
        "}",
    ];
    let new = [
        "pub fn f() {",
        "    // disabled",
        "    println!(\"live\");",
        "    //",
        "}",
    ];
    let mut body = String::from("@@ -1,5 +1,5 @@\n");
    for line in old {
        body.push_str(&format!("-{line}\n"));
    }
    for line in new {
        body.push_str(&format!("+{line}\r\n"));
    }
    let activated = check(&format!("{}\r\n", new.join("\r\n")), &body)?;
    if !activated.2.iter().any(|(line, _)| *line == 3) {
        failures.push(format!(
            "a statement activated by a moved comment boundary lost its finding: {activated:?}"
        ));
    }

    // `//` -> `///` inside a macro invocation adds a `#[doc]` token the macro
    // can match.
    let source = |comment: &str| {
        format!(
            "macro_rules! flag {{\n    (0) => {{ 0 }};\n    (0 #[$m:meta]) => {{ 1 }};\n}}\n\npub fn f() -> u32 {{\n    flag!(0 {comment} note\n    )\n}}\n"
        )
    };
    let documented = check(
        &source("///"),
        "@@ -7 +7 @@\n-    flag!(0 // note\n+    flag!(0 /// note\n",
    )?;
    if !documented.2.iter().any(|(line, _)| *line == 7) {
        failures.push(format!(
            "a comment turned into a documentation attribute lost its finding: {documented:?}"
        ));
    }
    // Control: rewording the ordinary comment in the same place stays quiet.
    let reworded = check(
        &source("//"),
        "@@ -7 +7 @@\n-    flag!(0 // remark\n+    flag!(0 // note\n",
    )?;
    failures.extend(expect_no_findings(
        "ordinary comment in a macro",
        &reworded,
        &[],
    ));

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// #15232: a block comment inside the line added, removed or reworded is
/// not code either. A comment that held two tokens apart, a documentation
/// block comment and a code edit beside a kept comment keep their finding.
#[test]
fn inline_block_comment_edits_report_nothing_and_code_edits_beside_them_still_do()
-> Result<(), String> {
    let plain = "    if total > 100 {";
    let commented = "    if total > 100 /* threshold */ {";
    let reworded = "    if total > 100 /* the threshold for big orders */ {";
    let mut failures = Vec::new();
    for (label, removed, added) in [
        ("block comment added", plain, commented),
        ("block comment removed", commented, plain),
        ("block comment reworded", commented, reworded),
        (
            "block comment beside punctuation removed",
            "    if total > 100 /* threshold */{",
            "    if total > 100{",
        ),
        (
            "line comment turned block comment",
            "    if total > 100 { // threshold",
            "    if total > 100 /* threshold */ {",
        ),
    ] {
        let observed = check(
            &source_with(added, "\n", true),
            &line_two_hunk(removed, added),
        )?;
        failures.extend(expect_no_findings(label, &observed, &[]));
    }
    for (label, removed, added) in [
        (
            "code beside a kept block comment",
            commented,
            "    if total >= 100 /* threshold */ {",
        ),
        (
            "code and block comment",
            plain,
            "    if total >= 100 /* threshold */ {",
        ),
    ] {
        let observed = check(
            &source_with(added, "\n", true),
            &line_two_hunk(removed, added),
        )?;
        failures.extend(expect_line_two_only(label, &observed));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
