//! End-to-end witness for #15539: an owner call inside an empty
//! catch-all macro (`macro_rules! discard { ($($t:tt)*) => {}; }`) never
//! runs, so its boundary literal is no test input, whether the macro is
//! defined at file level or inside the test body. A macro whose expansion
//! keeps its tokens still credits the input.

use ripr::domain::StageState;
use ripr::{CheckInput, CheckOutput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// `lib` is the new `src/lib.rs` and `files` the other sources by path;
    /// `diff` changes one line of one of them.
    fn create(lib: &str, files: &[(&str, &str)], test: &str, diff: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-empty-macro-boundary-{}-{sequence}",
            std::process::id()
        ));
        // Built before any write so `Drop` removes a partly written root.
        let repo = Self { root };
        let root = &repo.root;
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        let write = |path: &str, text: &str| {
            if let Some(parent) = root.join(path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"pricing\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", lib)?;
        for (path, text) in files {
            write(path, text)?;
        }
        write("tests/it.rs", test)?;
        write("diff.patch", diff)?;
        Ok(repo)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn diff_at(path: &str, text: &str, new: &str, old: &str) -> Result<(String, usize), String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut matches = lines.iter().enumerate().filter(|(_, line)| **line == new);
    let (index, _) = matches
        .next()
        .ok_or_else(|| format!("{path} has no line {new:?}"))?;
    if matches.next().is_some() || index < 3 || index + 3 > lines.len() {
        return Err(format!(
            "{path} needs one {new:?} with three lines before and two after"
        ));
    }
    let context = |range: std::ops::Range<usize>| {
        lines[range]
            .iter()
            .map(|line| format!(" {line}\n"))
            .collect::<String>()
    };
    let start = index - 2;
    Ok((
        format!(
            "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -{start},6 +{start},6 @@\n{}-{old}\n+{new}\n{}",
            context(index - 3..index),
            context(index + 1..index + 3)
        ),
        index + 1,
    ))
}

const LIB: &str = "/// Discount for an order.\n///\n/// Ten off at or above the threshold.\npub fn discount(amount: u32) -> u32 {\n    if amount >= 100 { 10 } else { 0 }\n}\n\npub fn tail() {}\n";

/// The infection states of the predicate findings on the changed boundary
/// for `test` as `tests/it.rs`.
fn boundary_infection(test: &str) -> Result<Vec<StageState>, String> {
    let (diff, line) = diff_at(
        "src/lib.rs",
        LIB,
        "    if amount >= 100 { 10 } else { 0 }",
        "    if amount > 100 { 10 } else { 0 }",
    )?;
    let repo = TempRepo::create(LIB, &[], test, &diff)?;
    let output = repo.check()?;
    let findings = output
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == line && finding.probe.family.as_str() == "predicate"
        })
        .collect::<Vec<Finding>>();
    if findings.is_empty() {
        return Err(format!("no predicate finding on line {line}"));
    }
    Ok(findings
        .iter()
        .map(|finding| finding.ripr.infect.state.clone())
        .collect())
}

const DISCARD: &str = "macro_rules! discard {\n    ($($t:tt)*) => {};\n}\n";

#[test]
fn a_boundary_call_inside_a_test_local_empty_macro_is_no_input() -> Result<(), String> {
    let test = format!(
        "use pricing::discount;\n\n#[test]\nfn below() {{\n{DISCARD}    discard!(discount(100));\n    assert_eq!(discount(50), 0);\n}}\n"
    );
    assert_eq!(boundary_infection(&test)?, vec![StageState::Weak], "{test}");
    Ok(())
}

#[test]
fn a_boundary_call_inside_a_file_level_empty_macro_is_no_input() -> Result<(), String> {
    let test = format!(
        "use pricing::discount;\n\n{DISCARD}\n#[test]\nfn below() {{\n    discard!(discount(100));\n    assert_eq!(discount(50), 0);\n}}\n"
    );
    assert_eq!(boundary_infection(&test)?, vec![StageState::Weak], "{test}");
    Ok(())
}

/// Alternates: a test-local macro that keeps its tokens, and the same
/// empty macro used only after the boundary call, still credit the input.
#[test]
fn a_boundary_call_a_macro_keeps_still_reaches_the_change() -> Result<(), String> {
    for test in [
        "use pricing::discount;\n\n#[test]\nfn at_threshold() {\n    macro_rules! keep {\n        ($($t:tt)*) => { $($t)* };\n    }\n    assert_eq!(keep!(discount(100)), 10);\n}\n".to_string(),
        format!(
            "use pricing::discount;\n\n#[test]\nfn at_threshold() {{\n    assert_eq!(discount(100), 10);\n{DISCARD}    discard!(discount(50));\n}}\n"
        ),
    ] {
        assert_eq!(boundary_infection(&test)?, vec![StageState::Yes], "{test}");
    }
    Ok(())
}
