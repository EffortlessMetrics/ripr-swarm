//! #15103: a patch file git wrote under `diff.mnemonicPrefix=true` names its
//! sides `i/`, `w/`, `c/` or `o/`. `ripr check --diff` read the prefix as part
//! of the path, reported every changed file as absent from the working tree
//! and produced no findings. Each mnemonic spelling must give the result of
//! the default `a/`/`b/` patch, while prefix-free patches that only look
//! similar keep their own reading. A patch with unrecognized custom prefixes
//! still reports the file as absent, but names the prefix cause.

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

const SOURCE: &str = "pub fn discount(total: u32) -> u32 {\n    if total >= 100 {\n        total - 10\n    } else {\n        total\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn small() {\n        assert_eq!(discount(5), 5);\n    }\n}\n";

const HUNK: &str = "@@ -2 +2 @@\n-    if total > 100 {\n+    if total >= 100 {\n";

/// The typed outcome kind, its limitation kinds, and each finding's
/// `(file, line, classification)`.
type Observed = (String, Vec<String>, Vec<(String, u64, String)>);

/// Run `ripr check --diff` over a one-library workspace whose source file
/// lives at `source_path` (the diff's post-image).
fn check(source_path: &str, diff: &str) -> Result<Observed, String> {
    Ok(check_with_limitations(source_path, diff)?.0)
}

/// [`check`], plus each limitation's `(path, recovery detail, bounded detail)`.
type LimitationText = (String, String, String);

fn check_with_limitations(
    source_path: &str,
    diff: &str,
) -> Result<(Observed, Vec<LimitationText>), String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!(
        "ripr-mnemonic-prefixes-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let _guard = FixtureGuard { root: root.clone() };
    let source = root.join(source_path);
    let parent = source.parent().ok_or("source path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| format!("mkdir: {error}"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"{source_path}\"\n"
        ),
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    fs::write(&source, SOURCE).map_err(|error| format!("write source: {error}"))?;
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
                text(finding.pointer("/probe/file"), "probe file")?,
                finding
                    .pointer("/probe/line")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| format!("missing probe line\nstdout: {stdout}"))?,
                text(finding.get("classification"), "classification")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let texts = document
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|limitation| {
            Ok((
                text(limitation.get("path"), "limitation path")?,
                text(limitation.pointer("/recovery/detail"), "recovery detail")?,
                text(limitation.get("bounded_detail"), "bounded detail")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(((kind, limitations, findings), texts))
}

/// A one-file patch in git's layout with the given side tokens.
fn patch(old: &str, new: &str) -> String {
    format!("diff --git {old} {new}\nindex 1111111..2222222 100644\n--- {old}\n+++ {new}\n{HUNK}")
}

fn one_finding_at(file: &str) -> Observed {
    (
        "complete_with_findings".to_string(),
        Vec::new(),
        vec![(file.to_string(), 2, "weakly_exposed".to_string())],
    )
}

#[test]
fn mnemonic_side_prefixes_read_like_the_default_prefixes() -> Result<(), String> {
    let control = check("src/lib.rs", &patch("a/src/lib.rs", "b/src/lib.rs"))?;
    if control != one_finding_at("./src/lib.rs") {
        return Err(format!(
            "the default-prefix control is not the one finding on line 2: {control:?}"
        ));
    }
    let mut failures = Vec::new();
    for (old, new) in [
        ("i/", "w/"),
        ("c/", "w/"),
        ("c/", "i/"),
        ("o/", "w/"),
        ("w/", "i/"),
    ] {
        let observed = check(
            "src/lib.rs",
            &patch(&format!("{old}src/lib.rs"), &format!("{new}src/lib.rs")),
        )?;
        if observed != control {
            failures.push(format!(
                "{old} {new}: differs from the default-prefix result: {observed:?}"
            ));
        }
    }
    // The same patch with CRLF line endings in its header lines.
    let crlf = patch("i/src/lib.rs", "w/src/lib.rs").replacen(
        "\nindex 1111111..2222222 100644\n--- i/src/lib.rs\n+++ w/src/lib.rs\n",
        "\r\nindex 1111111..2222222 100644\r\n--- i/src/lib.rs\r\n+++ w/src/lib.rs\r\n",
        1,
    );
    let observed = check("src/lib.rs", &crlf)?;
    if observed != control {
        failures.push(format!(
            "CRLF headers: differs from the default-prefix result: {observed:?}"
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Prefix-free patches whose first directory is a mnemonic letter are real
/// paths and keep their reading.
#[test]
fn prefix_free_paths_under_a_mnemonic_letter_stay_real_paths() -> Result<(), String> {
    let mut failures = Vec::new();

    // `git diff --no-prefix` for a file under a real `i/` directory.
    let same_directory = check("i/lib.rs", &patch("i/lib.rs", "i/lib.rs"))?;
    let located: Vec<_> = same_directory
        .2
        .iter()
        .map(|(file, line, _)| (file.as_str(), *line))
        .collect();
    if same_directory.0 != "complete_with_findings"
        || !same_directory.1.is_empty()
        || located != [("./i/lib.rs", 2)]
    {
        failures.push(format!(
            "a prefix-free patch under a real i/ directory: {same_directory:?}"
        ));
    }

    // A prefix-free rename from `i/lib.rs` to `w/lib.rs` with an edit: the
    // header alone looks mnemonic, the rename metadata says otherwise.
    let rename = format!(
        "diff --git i/lib.rs w/lib.rs\nsimilarity index 90%\nrename from i/lib.rs\nrename to w/lib.rs\nindex 1111111..2222222 100644\n--- i/lib.rs\n+++ w/lib.rs\n{HUNK}"
    );
    let renamed = check("w/lib.rs", &rename)?;
    if !renamed
        .2
        .iter()
        .any(|(file, line, _)| file == "./w/lib.rs" && *line == 2)
        || renamed
            .1
            .iter()
            .any(|kind| kind == "changed_file_absent_from_worktree")
    {
        failures.push(format!(
            "a prefix-free rename between i/ and w/ lost its file: {renamed:?}"
        ));
    }

    // Unrecognized prefixes are not guessed.
    let custom = check("src/lib.rs", &patch("old/src/lib.rs", "new/src/lib.rs"))?;
    if custom.1 != ["changed_file_absent_from_worktree"] || !custom.2.is_empty() {
        failures.push(format!(
            "custom prefixes changed their disclosed reading: {custom:?}"
        ));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Plan step 2: custom prefixes are still not guessed, but when the changed
/// path without its first component is present, the limitation names the
/// prefix cause and the git option that fixes it instead of asking for a
/// checkout of a file that is there.
#[test]
fn unread_custom_prefix_names_the_prefix_cause() -> Result<(), String> {
    let mut failures = Vec::new();
    let (observed, texts) =
        check_with_limitations("src/lib.rs", &patch("old/src/lib.rs", "new/src/lib.rs"))?;
    if observed.1 != ["changed_file_absent_from_worktree"] || !observed.2.is_empty() {
        failures.push(format!(
            "custom prefixes changed their reading: {observed:?}"
        ));
    }
    match texts.as_slice() {
        [(path, recovery, detail)] => {
            if path != "new/src/lib.rs"
                || !recovery.contains("custom side prefix (`new/`)")
                || !recovery.contains("git diff --src-prefix=a/ --dst-prefix=b/")
                || !recovery.contains("check out the missing file or disable sparse checkout")
                || !detail.contains("without its first component (`new/`)")
            {
                failures.push(format!("prefix cause not named: {texts:?}"));
            }
        }
        _ => failures.push(format!("expected one limitation: {texts:?}")),
    }

    // A truly absent file keeps the checkout recovery: the source lives at
    // `lib/lib.rs`, so neither `new/src/lib.rs` nor `src/lib.rs` exists.
    let (_, texts) =
        check_with_limitations("lib/lib.rs", &patch("old/src/lib.rs", "new/src/lib.rs"))?;
    match texts.as_slice() {
        [(_, recovery, _)]
            if recovery.contains("Check out the missing file")
                && !recovery.contains("side prefix") => {}
        _ => failures.push(format!("absent file lost its checkout recovery: {texts:?}")),
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
