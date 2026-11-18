//! #15110: an item moved within its file, with no other change, seeds no
//! finding.
//!
//! Git writes the move as a removed run and an added run. `ripr check`
//! reported every probe-bearing line of the moved function as changed
//! behavior on both sides. Each control beside the pure move changes what
//! the program does and must keep a finding.

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

fn write_workspace(source: &str) -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!("ripr-moved-item-{}-{call}", std::process::id()));
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

/// `old` with its lines `first..=last` (1-based) moved to just before old
/// line `before` (which must lie after the block), each moved line passed
/// through `edit`, and the zero-context patch git writes for that move.
fn moved(
    old: &[&str],
    first: usize,
    last: usize,
    before: usize,
    edit: impl Fn(&str) -> String,
) -> (String, String) {
    let count = last - first + 1;
    let block: Vec<String> = old[first - 1..last].iter().map(|line| edit(line)).collect();
    let mut new: Vec<String> = Vec::new();
    for (at, line) in old.iter().enumerate() {
        let number = at + 1;
        if number == before {
            new.extend(block.iter().cloned());
        }
        if !(first..=last).contains(&number) {
            new.push((*line).to_string());
        }
    }
    let mut diff = format!("@@ -{first},{count} +{},0 @@\n", first - 1);
    for line in &old[first - 1..last] {
        diff.push_str(&format!("-{line}\n"));
    }
    diff.push_str(&format!(
        "@@ -{},0 +{},{count} @@\n",
        before - 1,
        before - count
    ));
    for line in &block {
        diff.push_str(&format!("+{line}\n"));
    }
    (format!("{}\n", new.join("\n")), diff)
}

const TOP_LEVEL: [&str; 29] = [
    "pub fn discount(total: u32) -> u32 {",
    "    if total > 100 {",
    "        total - 10",
    "    } else {",
    "        total",
    "    }",
    "}",
    "",
    "pub fn shipping(weight: u32) -> u32 {",
    "    if weight > 20 {",
    "        15",
    "    } else {",
    "        5",
    "    }",
    "}",
    "",
    "pub fn tax(total: u32) -> u32 {",
    "    total / 10",
    "}",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    use super::*;",
    "    #[test]",
    "    fn small() {",
    "        assert_eq!(discount(5), 5);",
    "        assert_eq!(shipping(5), 5);",
    "    }",
    "}",
];

const TWO_IMPLS: [&str; 28] = [
    "pub struct Small;",
    "pub struct Large;",
    "",
    "impl Small {",
    "    pub fn discount(&self, total: u32) -> u32 {",
    "        if total > 100 {",
    "            total - 10",
    "        } else {",
    "            total",
    "        }",
    "    }",
    "",
    "    pub fn name(&self) -> &'static str {",
    "        \"small\"",
    "    }",
    "}",
    "",
    "impl Large {",
    "    pub fn name(&self) -> &'static str {",
    "        \"large\"",
    "    }",
    "}",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    #[test]",
    "    fn nothing() {}",
    "}",
];

#[test]
fn a_function_moved_within_its_file_reports_nothing() -> Result<(), String> {
    // `discount` and its blank line move below `tax`.
    let (source, diff) = moved(&TOP_LEVEL, 1, 8, 21, str::to_owned);
    let observed = check(&source, &diff)?;
    if observed.0 != "no_behavioral_candidates" || !observed.1.is_empty() || !observed.2.is_empty()
    {
        return Err(format!("a pure move was reported: {observed:?}"));
    }
    Ok(())
}

#[test]
fn a_move_that_changes_the_program_keeps_its_finding() -> Result<(), String> {
    let mut failures = Vec::new();

    // The moved copy also changes its comparison (new line 14).
    let (source, diff) = moved(&TOP_LEVEL, 1, 8, 21, |line| {
        line.replace("total > 100", "total >= 100")
    });
    let edited = check(&source, &diff)?;
    if !edited.2.iter().any(|(line, _)| *line == 14) {
        failures.push(format!(
            "a moved and edited comparison lost its finding: {edited:?}"
        ));
    }

    // The method leaves `impl Small` for `impl Large` (its comparison is new line 15).
    let (source, diff) = moved(&TWO_IMPLS, 5, 12, 22, str::to_owned);
    let other_owner = check(&source, &diff)?;
    if !other_owner.2.iter().any(|(line, _)| *line == 15) {
        failures.push(format!(
            "a method moved into another impl lost its finding: {other_owner:?}"
        ));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
