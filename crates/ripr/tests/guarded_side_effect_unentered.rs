//! End-to-end witness for RIPR-SPEC-0260 (#7132).
//!
//! A changed `v.push(x)` inside `if x > 10` runs only for inputs above 10.
//! When every related test calls the owner with a literal that leaves the
//! guard false, no mutant of the push executes, so an exact assertion over
//! the collection cannot reveal it: the side effect must not read exposed,
//! and the guard is named as the missing input. Any input that enters the
//! guard, or any input the reading cannot evaluate, keeps today's verdict.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const UNENTERED_PREFIX: &str = "No related test call enters guard";

fn source(guard: &str, push: &str) -> String {
    format!(
        "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if {guard} {{\n        {push}\n    }}\n}}\n"
    )
}

fn diff(guard: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn add(v: &mut Vec<u32>, x: u32) {{\n     if {guard} {{\n-        v.push(x);\n+        v.push(x * 1);\n     }}\n }}\n"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(guard: &str, test_source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-guarded-side-effect-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"guarded-side-effect\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source(guard, "v.push(x * 1);"))
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/add.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff(guard))
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
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

/// The side-effect and call-deletion findings on the changed push.
fn push_findings(guard: &str, test_source: &str) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create(guard, test_source)?;
    let output = repo.check()?;
    if format!("{output:?}").contains("MalformedDiff") {
        return Err("the fixture diff must parse cleanly".to_string());
    }
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            ) && finding.probe.location.line == 3
        })
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!(
            "no side-effect finding on the push; observed {observed}"
        ));
    }
    Ok(findings)
}

fn names_unentered_guard(finding: &ripr::Finding) -> bool {
    finding.ripr.infect.summary.contains(UNENTERED_PREFIX)
        || finding
            .evidence
            .iter()
            .any(|line| line.contains(UNENTERED_PREFIX))
}

fn test_calling(calls: &str) -> String {
    format!(
        "use guarded_side_effect::add;\n\n#[test]\nfn adds() {{\n    let mut v = Vec::new();\n{calls}\n    assert_eq!(v, Vec::<u32>::new());\n}}\n"
    )
}

#[test]
fn a_literal_that_misses_the_guard_names_it_and_is_not_exposed() -> Result<(), String> {
    for guard in ["x > 10", "10 < x", "(x >= 11)"] {
        for finding in push_findings(guard, &test_calling("    add(&mut v, 4);"))? {
            assert_eq!(
                finding.class,
                ExposureClass::WeaklyExposed,
                "{guard}: {finding:?}"
            );
            assert!(
                names_unentered_guard(&finding),
                "{guard}: the guard must be named: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}

#[test]
fn an_input_that_enters_the_guard_keeps_the_exposed_reading() -> Result<(), String> {
    let test = "use guarded_side_effect::add;\n\n#[test]\nfn adds() {\n    let mut v = Vec::new();\n    add(&mut v, 11);\n    assert_eq!(v, vec![11]);\n}\n";
    let findings = push_findings("x > 10", test)?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "an entering input keeps exposed: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|finding| !names_unentered_guard(finding))
    );
    Ok(())
}

#[test]
fn one_entering_call_or_one_unread_input_names_no_guard() -> Result<(), String> {
    for calls in [
        // One call enters the guard, so the tests do run the push.
        "    add(&mut v, 4);\n    add(&mut v, 11);",
        // A computed input is not evaluated.
        "    let n = 4;\n    add(&mut v, n);",
        // A non-literal expression is not evaluated either.
        "    add(&mut v, 2 + 2);",
    ] {
        for finding in push_findings("x > 10", &test_calling(calls))? {
            assert!(
                !names_unentered_guard(&finding),
                "{calls}: no guard may be named: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}

#[test]
fn an_unreadable_guard_names_nothing() -> Result<(), String> {
    for guard in ["x > 10 && x < 100", "x + 1 > 10", "x > LIMIT"] {
        let test = test_calling("    add(&mut v, 4);");
        let repo_guard = guard.to_string();
        let findings = if guard.contains("LIMIT") {
            // `LIMIT` must exist for the owner to read as Rust; the reading
            // still refuses a named bound.
            push_findings_with_prelude(&repo_guard, &test, "pub const LIMIT: u32 = 10;\n")?
        } else {
            push_findings(&repo_guard, &test)?
        };
        for finding in findings {
            assert!(
                !names_unentered_guard(&finding),
                "{guard}: an unread guard names nothing: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}

/// [`push_findings`] with extra items before the owner. The diff's line
/// numbers move with them.
fn push_findings_with_prelude(
    guard: &str,
    test_source: &str,
    prelude: &str,
) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create(guard, test_source)?;
    let lines = prelude.lines().count();
    std::fs::write(
        repo.root.join("src/lib.rs"),
        format!("{prelude}{}", source(guard, "v.push(x * 1);")),
    )
    .map_err(|error| format!("write source failed: {error}"))?;
    let shifted = diff(guard).replace(
        "@@ -1,5 +1,5 @@",
        &format!("@@ -{0},5 +{0},5 @@", 1 + lines),
    );
    std::fs::write(repo.root.join("diff.patch"), shifted)
        .map_err(|error| format!("write diff failed: {error}"))?;
    let output = repo.check()?;
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err(format!("{guard}: no side-effect finding on the push"));
    }
    Ok(findings)
}

/// The side-effect findings on `changed_line` of a crate whose `src/lib.rs`
/// moves from `before` to `after` (one changed line), tested by
/// `test_source`.
fn custom_findings(
    before: &str,
    after: &str,
    changed_line: usize,
    test_source: &str,
) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create("x > 10", test_source)?;
    std::fs::write(repo.root.join("src/lib.rs"), after)
        .map_err(|error| format!("write source failed: {error}"))?;
    let old = before.lines().collect::<Vec<_>>();
    let new = after.lines().collect::<Vec<_>>();
    if old.len() != new.len() {
        return Err("the fixture must change exactly one line".to_string());
    }
    let mut body = String::new();
    for (old_line, new_line) in old.iter().zip(&new) {
        if old_line == new_line {
            body.push_str(&format!(" {old_line}\n"));
        } else {
            body.push_str(&format!("-{old_line}\n+{new_line}\n"));
        }
    }
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{0} +1,{0} @@\n{body}",
        old.len()
    );
    std::fs::write(repo.root.join("diff.patch"), diff)
        .map_err(|error| format!("write diff failed: {error}"))?;
    let output = repo.check()?;
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            ) && finding.probe.location.line == changed_line
        })
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err(format!("no side-effect finding on line {changed_line}"));
    }
    Ok(findings)
}

#[test]
fn recursion_a_same_line_else_or_a_helper_names_no_guard() -> Result<(), String> {
    // The owner calls itself after the guard: `walk(&mut v, 0)` reaches
    // depth 4, which enters `depth > 3`, although the test passes 0.
    let recursive = |push: &str| {
        format!(
            "pub fn walk(v: &mut Vec<u32>, depth: u32) {{\n    if depth > 3 {{\n        {push}\n    }}\n    if depth < 5 {{\n        walk(v, depth + 1);\n    }}\n}}\n"
        )
    };
    let walk_test = "use guarded_side_effect::walk;\n\n#[test]\nfn walks() {\n    let mut v = Vec::new();\n    walk(&mut v, 0);\n    assert_eq!(v, vec![4, 5]);\n}\n";
    // The changed push sits in an `else` closed on the guard's own line,
    // so input 4 does run it.
    let same_line_else = |push: &str| {
        format!(
            "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        v.clear();\n    }} else {{ {push} }}\n}}\n"
        )
    };
    // A helper passes 20, which enters the guard; only the direct call
    // passes 4.
    let helper_test = "use guarded_side_effect::add;\n\nfn add_large(v: &mut Vec<u32>) {\n    add(v, 20);\n}\n\n#[test]\nfn adds() {\n    let mut v = Vec::new();\n    add(&mut v, 4);\n    add_large(&mut v);\n    assert_eq!(v, vec![20]);\n}\n";
    // The owner calls a helper that calls the owner back with 50, which
    // enters the guard: only a transitive walk of the owner's calls sees it.
    let mutual = |push: &str| {
        format!(
            "pub fn add(v: &mut Vec<u32>, x: u32) {{\n    if x > 10 {{\n        {push}\n    }}\n    fixup(v);\n}}\n\nfn fixup(v: &mut Vec<u32>) {{\n    if v.is_empty() {{\n        add(v, 50);\n    }}\n}}\n"
        )
    };
    let cases = [
        (
            "mutual recursion",
            mutual("v.push(x);"),
            mutual("v.push(x * 1);"),
            3,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "recursion",
            recursive("v.push(depth);"),
            recursive("v.push(depth * 1);"),
            3,
            walk_test.to_string(),
        ),
        (
            "same-line else",
            same_line_else("v.push(x);"),
            same_line_else("v.push(x * 1);"),
            4,
            test_calling("    add(&mut v, 4);"),
        ),
        (
            "helper",
            source("x > 10", "v.push(x);"),
            source("x > 10", "v.push(x * 1);"),
            3,
            helper_test.to_string(),
        ),
    ];
    for (label, before, after, line, test) in cases {
        for finding in custom_findings(&before, &after, line, &test)? {
            assert!(
                !names_unentered_guard(&finding),
                "{label}: no guard may be named: {:?}",
                finding.ripr.infect
            );
        }
    }
    Ok(())
}
