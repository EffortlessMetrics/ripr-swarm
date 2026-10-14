//! End-to-end witness for guard-only match-arm edits (RIPR-SPEC-0229).
//!
//! An edit inside an arm guard (`n if 0 > n =>` from `n if n < 0 =>`) is
//! the guard's predicate finding: arm selection never evaluates guards, so
//! the arm head is not probed, and the old line is not probed again as
//! deleted behavior. A bare binding in
//! the body's only `match` on a parameter reads that parameter's inputs.

use ripr::{CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
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
            "ripr-match-guard-edit-{}-{sequence}",
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
            "[package]\nname = \"guard_edit\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The findings on `line` of `src/lib.rs` after applying `old` -> `new`.
fn findings_on_changed_line(
    lib: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    let (diff, line) = diff_at("src/lib.rs", lib, new, old)?;
    let repo = TempRepo::create(lib, &[], test, &diff)?;
    let output = repo.check()?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect())
}

const SIGN: &str = "/// The sign of `n`.\npub fn sign(n: i64) -> i8 {\n    match n {\n        n if 0 > n => -1,\n        0 => 0,\n        _ => 1,\n    }\n}\n";

#[test]
fn a_guard_only_edit_with_boundary_inputs_reads_exposed() -> Result<(), String> {
    let test = "use guard_edit::sign;\n\n#[test]\nfn signs() {\n    assert_eq!(sign(-5), -1);\n    assert_eq!(sign(0), 0);\n    assert_eq!(sign(9), 1);\n}\n";
    let findings = findings_on_changed_line(
        SIGN,
        "        n if 0 > n => -1,",
        "        n if n < 0 => -1,",
        test,
    )?;
    let seen = findings
        .iter()
        .map(|finding| {
            (
                finding.probe.family.as_str().to_string(),
                finding.probe.expression.clone(),
                finding.class.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seen,
        vec![(
            "predicate".to_string(),
            "0 > n".to_string(),
            ExposureClass::Exposed
        )]
    );
    Ok(())
}

#[test]
fn a_guard_only_edit_without_the_boundary_input_stays_weak() -> Result<(), String> {
    // Control: without `sign(0)` no test sits on the guard's boundary.
    let test = "use guard_edit::sign;\n\n#[test]\nfn signs() {\n    assert_eq!(sign(-5), -1);\n    assert_eq!(sign(9), 1);\n}\n";
    let findings = findings_on_changed_line(
        SIGN,
        "        n if 0 > n => -1,",
        "        n if n < 0 => -1,",
        test,
    )?;
    assert!(!findings.is_empty());
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

fn tier(prelude: &str) -> String {
    format!(
        "#[derive(Debug, PartialEq)]\npub enum Tier {{\n    Gold,\n    Silver,\n    Bronze,\n}}\n\npub fn tier(points: i64) -> Tier {{\n{prelude}    match points {{\n        p if 100 <= p => Tier::Gold,\n        p if p >= 50 => Tier::Silver,\n        _ => Tier::Bronze,\n    }}\n}}\n"
    )
}

const SILVER: &str = "use guard_edit::{Tier, tier};\n\n#[test]\nfn sixty_is_silver() {\n    assert_eq!(tier(60), Tier::Silver);\n}\n";

fn missing_values(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .flat_map(|finding| {
            finding
                .activation
                .missing_discriminators
                .iter()
                .map(|fact| fact.value.clone())
        })
        .collect()
}

#[test]
fn a_guard_binding_reads_the_scrutinee_parameter_inputs() -> Result<(), String> {
    let findings = findings_on_changed_line(
        &tier(""),
        "        p if 100 <= p => Tier::Gold,",
        "        p if p >= 100 => Tier::Gold,",
        SILVER,
    )?;
    assert!(
        findings
            .iter()
            .all(|finding| finding.probe.family.as_str() == "predicate"),
        "{findings:?}"
    );
    assert_eq!(missing_values(&findings), vec!["p == 100".to_string()]);
    assert!(
        findings
            .iter()
            .all(|finding| finding.class == ExposureClass::WeaklyExposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_second_match_leaves_the_guard_binding_unread() -> Result<(), String> {
    // Control: with two `match`es the arm could belong to either, so the
    // binding is not read as the parameter and no boundary is named.
    let lib = tier("    let _bonus = match points {\n        0 => 1,\n        _ => 0,\n    };\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 <= p => Tier::Gold,",
        "        p if p >= 100 => Tier::Gold,",
        SILVER,
    )?;
    assert!(!findings.is_empty());
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_loop_variable_named_like_a_guard_binding_is_not_the_parameter() -> Result<(), String> {
    // Control (CodeRabbit on #7811): the loop's `p` is not the arm binding,
    // so `tier(10)` is not boundary evidence for `p >= 10`.
    let lib = tier(
        "    for p in 0..3 {\n        if p >= 10 {\n            return Tier::Bronze;\n        }\n    }\n",
    );
    let test = "use guard_edit::{Tier, tier};\n\n#[test]\nfn ten_is_bronze() {\n    assert_eq!(tier(10), Tier::Bronze);\n}\n";
    let findings =
        findings_on_changed_line(&lib, "        if p >= 10 {", "        if p > 10 {", test)?;
    assert!(!findings.is_empty());
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn an_identical_arm_deleted_elsewhere_keeps_its_probe() -> Result<(), String> {
    // Control (CodeRabbit on #7811): only the guard edit's own old line is
    // skipped; the same text deleted from `sign2` is still probed.
    let lib = format!(
        "{SIGN}\n/// Second.\npub fn sign2(n: i64) -> i8 {{\n    match n {{\n        0 => 0,\n        _ => 1,\n    }}\n}}\n"
    );
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,7 +1,7 @@\n /// The sign of `n`.\n pub fn sign(n: i64) -> i8 {\n     match n {\n-        n if n < 0 => -1,\n+        n if 0 > n => -1,\n         0 => 0,\n         _ => 1,\n     }\n@@ -10,6 +10,5 @@\n /// Second.\n pub fn sign2(n: i64) -> i8 {\n     match n {\n-        n if n < 0 => -1,\n         0 => 0,\n         _ => 1,\n";
    let test = "use guard_edit::{sign, sign2};\n\n#[test]\nfn signs() {\n    assert_eq!(sign(-5), -1);\n    assert_eq!(sign(0), 0);\n    assert_eq!(sign2(9), 1);\n}\n";
    let repo = TempRepo::create(&lib, &[], test, diff)?;
    let output = repo.check()?;
    let deleted = output
        .findings
        .iter()
        .filter(|finding| finding.probe.expression == "n if n < 0 => -1,")
        .count();
    assert!(deleted >= 1, "{:?}", output.findings);
    assert!(
        output
            .findings
            .iter()
            .any(|finding| finding.probe.expression == "0 > n"),
        "{:?}",
        output.findings
    );
    Ok(())
}

const BOUNDARY_TESTS: &str = "use guard_edit::{Tier, tier};\n\n#[test]\nfn around_one_hundred() {\n    assert_eq!(tier(99), Tier::Silver);\n    assert_eq!(tier(100), Tier::Silver);\n    assert_eq!(tier(101), Tier::Silver);\n    assert_eq!(tier(400), Tier::Gold);\n}\n";

#[test]
fn a_rebound_parameter_is_not_the_scrutinee() -> Result<(), String> {
    // Control (review on #7811): `let points = points - 50;` moves the real
    // boundary to `points == 150`, so `tier(100)` is no boundary evidence.
    let lib = tier("    let points = points - 50;\n").replace("p if 100 <= p", "p if 100 < p");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => Tier::Gold,",
        "        p if 100 <= p => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(!findings.is_empty());
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn the_unrebound_parameter_control_reads_exposed() -> Result<(), String> {
    // Positive twin of the rebinding control: with the parameter matched
    // directly, `tier(100)` sits on the boundary.
    let lib = tier("").replace("p if 100 <= p", "p if 100 < p");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => Tier::Gold,",
        "        p if 100 <= p => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_rebinding_line_that_mentions_fn_is_not_the_signature() -> Result<(), String> {
    // Control (review on #7811): only the owner's signature binds the
    // parameter; `fn ` elsewhere on a rebinding line (a nested fn, a string)
    // never exempts it.
    for prelude in [
        "    let points = { fn h(x: i64) -> i64 { x - 50 } h(points) };\n",
        "    let points = points - 50; let _s = \"fn \";\n",
        // A comment or a string opener before the rebinding on its line,
        // or a `*`-led continuation line, must not hide it (review on
        // #7811).
        "    /* shift */ let points = points - 50;\n",
        "    let _s = \"//\"; let points = points - 50;\n",
        "    let _s = \"/*\"; let points = points - 50;\n",
        "    let _k = 2\n        * 3; let points = points - 50;\n",
        "    /* x */ let points = match points { 0 => 0, q => q - 50 };\n",
    ] {
        let lib = tier(prelude).replace("p if 100 <= p", "p if 100 < p");
        let findings = findings_on_changed_line(
            &lib,
            "        p if 100 < p => Tier::Gold,",
            "        p if 100 <= p => Tier::Gold,",
            BOUNDARY_TESTS,
        )?;
        assert!(!findings.is_empty(), "{prelude}");
        assert!(
            findings
                .iter()
                .all(|finding| finding.class != ExposureClass::Exposed),
            "{prelude}: {findings:?}"
        );
    }
    Ok(())
}

const ONE_LINE_HEAD: &str = "pub fn tier(points: i64) -> Tier {\n    match points {";

#[test]
fn a_rebinding_on_the_signature_line_is_not_the_scrutinee() -> Result<(), String> {
    // Control (review on #7811): the signature ends at its `{`, so a
    // statement after it on the same line is body code that rebinds the
    // parameter before the `match`.
    let lib = tier("").replace(
        ONE_LINE_HEAD,
        "pub fn tier(points: i64) -> Tier { let points = points - 50; match points {",
    );
    assert_ne!(lib, tier(""));
    let lib = lib.replace("p if 100 <= p", "p if 100 < p");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => Tier::Gold,",
        "        p if 100 <= p => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(!findings.is_empty());
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_guard_pattern_naming_a_constant_is_not_the_parameter() -> Result<(), String> {
    // Control (review on #7811): a bare pattern name that resolves to a
    // constant compares the constant, not the scrutinee, so the test
    // arguments say nothing about the guard's boundary.
    let lib = format!(
        "#[allow(non_upper_case_globals)]\nconst p: i64 = 0;\n\n{}",
        tier("").replace("p if 100 <= p", "p if 100 < p")
    );
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => Tier::Gold,",
        "        p if 100 <= p => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(!findings.is_empty());
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

fn body_predicate_owner(arm_body: &str) -> String {
    format!(
        "/// The tier of `points`.\npub fn tier(points: i64) -> u8 {{\n    match points {{\n        p if p > 100 => {arm_body},\n        _ => 0,\n    }}\n}}\n"
    )
}

const AT_TWO_HUNDRED: &str =
    "use guard_edit::tier;\n\n#[test]\nfn two_hundred() {\n    assert_eq!(tier(200), 0);\n}\n";

#[test]
fn an_arm_body_binding_on_the_guard_line_is_not_the_guard_binding() -> Result<(), String> {
    // Control (review on #7811, #7875): a local or closure parameter `p`
    // in the arm body on the guard's line is another binding, so
    // `tier(200)` is no evidence for the body predicate `p >= 200`.
    for (after, before) in [
        (
            "{ let p = 10; if p >= 200 { 1 } else { 0 } }",
            "{ let p = 10; if p > 200 { 1 } else { 0 } }",
        ),
        (
            "{ let f = |p: i64| if p >= 200 { 1 } else { 0 }; f(10) }",
            "{ let f = |p: i64| if p > 200 { 1 } else { 0 }; f(10) }",
        ),
    ] {
        let lib = body_predicate_owner(after);
        let findings = findings_on_changed_line(
            &lib,
            &format!("        p if p > 100 => {after},"),
            &format!("        p if p > 100 => {before},"),
            AT_TWO_HUNDRED,
        )?;
        assert!(!findings.is_empty(), "{after}");
        assert!(
            missing_values(&findings).is_empty(),
            "{after}: {findings:?}"
        );
        assert!(
            findings
                .iter()
                .all(|finding| finding.class != ExposureClass::Exposed),
            "{after}: {findings:?}"
        );
    }
    Ok(())
}

#[test]
fn a_closure_in_the_guard_is_not_the_guard_binding() -> Result<(), String> {
    // Control: a closure parameter named like the binding inside the guard
    // shadows it there, so the guard's boundary reads no test input.
    let lib = tier("").replace(
        "p if 100 <= p => Tier::Gold,",
        "p if (|p: i64| 100 < p)(p - 50) => Tier::Gold,",
    );
    let findings = findings_on_changed_line(
        &lib,
        "        p if (|p: i64| 100 < p)(p - 50) => Tier::Gold,",
        "        p if (|p: i64| 100 <= p)(p - 50) => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}
