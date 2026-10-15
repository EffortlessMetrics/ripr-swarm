//! End-to-end witness for guard-only match-arm edits (RIPR-SPEC-0229).
//!
//! An edit inside an arm guard (`n if 0 > n =>` from `n if n < 0 =>`) is
//! the guard's predicate finding: arm selection never evaluates guards, so
//! the arm head is not probed, and the old line is not probed again as
//! deleted behavior. A bare binding in
//! the body's only `match` on a parameter reads that parameter's inputs.
//! An edit that adds or removes a `&&`/`||` operand, in a guard or a plain
//! `if`, is not credited by the surviving comparison's boundary (#7898).

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

const OTHER_TESTS: &str = "use guard_edit::tier;\n\n#[test]\nfn around_one_hundred() {\n    assert_eq!(tier(99, 0), 0);\n    assert_eq!(tier(100, 0), 0);\n    assert_eq!(tier(101, 0), 0);\n    assert_eq!(tier(400, 0), 0);\n}\n";

fn two_input_owner(arms: &str) -> String {
    format!(
        "/// The tier of `points`.\npub fn tier(points: i64, other: i64) -> u8 {{\n    let _ = other;\n    match points {{\n{arms}        _ => 0,\n    }}\n}}\n"
    )
}

fn assert_not_exposed(findings: &[Finding], label: &str) {
    assert!(!findings.is_empty(), "{label}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{label}: {findings:?}"
    );
}

#[test]
fn a_nested_match_spelled_without_a_space_is_a_second_match() -> Result<(), String> {
    // Control (review on #7811): `match(other)` and a `match` that ends its
    // line are second matches too, so the arm may bind `other`.
    for nested in ["match(other) {", "match\n            other {"] {
        let arms = format!(
            "        _ if other > 1 => match_value(),\n        _ => {nested}\n            p if 100 < p => 1,\n            _ => 0,\n        }},\n"
        );
        let lib = two_input_owner(&arms).replace(
            "    }\n}\n",
            "    }\n}\n\nfn match_value() -> u8 {\n    2\n}\n",
        );
        let findings = findings_on_changed_line(
            &lib,
            "            p if 100 < p => 1,",
            "            p if 100 <= p => 1,",
            OTHER_TESTS,
        )?;
        assert!(
            missing_values(&findings).is_empty(),
            "{nested}: {findings:?}"
        );
        assert_not_exposed(&findings, nested);
    }
    Ok(())
}

#[test]
fn a_block_guard_may_rebind_the_guard_binding() -> Result<(), String> {
    // Control (review on #7811): `{ let p = other; 100 < p }` compares
    // `other`, so the scrutinee's inputs are no boundary evidence.
    let lib = two_input_owner("        p if { let p = other; 100 < p } => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if { let p = other; 100 < p } => 1,",
        "        p if { let p = other; 100 <= p } => 1,",
        OTHER_TESTS,
    )?;
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "block guard");
    Ok(())
}

#[test]
fn a_deleted_guard_conjunct_keeps_the_arm_head() -> Result<(), String> {
    // Control (review on #7811): deleting `other > 0 &&` is more than the
    // comparison's edit, so the arm head stays, unexposed. The predicate's
    // own reading is pinned by `a_deleted_guard_conjunct_is_not_credited`.
    let lib = two_input_owner("        p if 100 < p => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => 1,",
        "        p if other > 0 && 100 <= p => 1,",
        OTHER_TESTS,
    )?;
    let arm_heads = findings
        .iter()
        .filter(|finding| finding.probe.family.as_str() == "match_arm")
        .collect::<Vec<_>>();
    assert!(!arm_heads.is_empty(), "{findings:?}");
    assert!(
        arm_heads
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

/// #7898's witness: `tier(100, 0)` is false under both the old guard
/// (`other > 0` fails) and the new one (`100 < 100` fails), so it
/// discriminates neither the dropped conjunct nor the boundary.
const CONJUNCT_WITNESS: &str =
    "use guard_edit::tier;\n\n#[test]\nfn witness() {\n    assert_eq!(tier(100, 0), 0);\n}\n";

/// Both sides of the `other == 0 && 100 <= points` boundary with the
/// unchanged operand true.
const CHAIN_BOUNDARY_TESTS: &str = "use guard_edit::tier;\n\n#[test]\nfn around_one_hundred() {\n    assert_eq!(tier(100, 0), 0);\n    assert_eq!(tier(101, 0), 1);\n}\n";

fn two_input_if(condition: &str) -> String {
    format!(
        "/// The tier of `points`.\npub fn tier(points: i64, other: i64) -> u8 {{\n    let _ = other;\n    if {condition} {{ 1 }} else {{ 0 }}\n}}\n\npub fn unused() {{}}\n"
    )
}

fn predicates(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|finding| finding.probe.family.as_str() == "predicate")
        .collect()
}

/// The predicate reads `infection_unknown`, not some other non-exposed class.
fn assert_predicates_infection_unknown(findings: &[Finding]) {
    let predicates = predicates(findings);
    assert!(!predicates.is_empty(), "{findings:?}");
    assert!(
        predicates
            .iter()
            .all(|finding| finding.class == ExposureClass::InfectionUnknown),
        "{findings:?}"
    );
}

#[test]
fn a_deleted_guard_conjunct_is_not_credited() -> Result<(), String> {
    // Control (#7898): the predicate `100 < p` was `other > 0 && 100 <= p`.
    // An input at `p == 100` says nothing about the dropped `other > 0`.
    let lib = two_input_owner("        p if 100 < p => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => 1,",
        "        p if other > 0 && 100 <= p => 1,",
        CONJUNCT_WITNESS,
    )?;
    assert!(!predicates(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "deleted guard conjunct");
    assert_predicates_infection_unknown(&findings);
    Ok(())
}

#[test]
fn a_deleted_if_conjunct_is_not_credited() -> Result<(), String> {
    // Control (#7898): the same deletion in a plain `if`.
    let lib = two_input_if("100 < points");
    let findings = findings_on_changed_line(
        &lib,
        "    if 100 < points { 1 } else { 0 }",
        "    if other > 0 && 100 <= points { 1 } else { 0 }",
        CONJUNCT_WITNESS,
    )?;
    assert!(!predicates(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "deleted if conjunct");
    assert_predicates_infection_unknown(&findings);
    // The reason is the changed operands, not some other unknown.
    assert!(
        predicates(&findings).iter().all(|finding| finding
            .ripr
            .infect
            .summary
            .contains("changes the predicate's `&&`/`||` operands")),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn an_added_if_disjunct_is_not_credited() -> Result<(), String> {
    // Control (#7898): adding `other > 0 ||` widens the predicate where no
    // test passes a positive `other`.
    let lib = two_input_if("other > 0 || 100 < points");
    let findings = findings_on_changed_line(
        &lib,
        "    if other > 0 || 100 < points { 1 } else { 0 }",
        "    if 100 <= points { 1 } else { 0 }",
        CONJUNCT_WITNESS,
    )?;
    assert!(!predicates(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "added if disjunct");
    assert_predicates_infection_unknown(&findings);
    Ok(())
}

#[test]
fn an_edit_inside_one_operand_of_an_unchanged_if_chain_reads_exposed() -> Result<(), String> {
    // Positive twin (#7898): the `&&` chain is unchanged, so the inputs on
    // both sides of `100` with `other == 0` true discriminate the edit.
    let lib = two_input_if("other == 0 && 100 < points");
    let findings = findings_on_changed_line(
        &lib,
        "    if other == 0 && 100 < points { 1 } else { 0 }",
        "    if other == 0 && 100 <= points { 1 } else { 0 }",
        CHAIN_BOUNDARY_TESTS,
    )?;
    assert!(
        predicates(&findings)
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn an_edit_inside_one_operand_of_an_unchanged_guard_chain_reads_exposed() -> Result<(), String> {
    // Positive twin (#7898) in a guard.
    let lib = two_input_owner("        p if other == 0 && 100 < p => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if other == 0 && 100 < p => 1,",
        "        p if other == 0 && 100 <= p => 1,",
        CHAIN_BOUNDARY_TESTS,
    )?;
    assert!(
        predicates(&findings)
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn an_edit_inside_one_operand_of_a_compound_guard_drops_the_arm_head() -> Result<(), String> {
    // Positive twin of the deleted-conjunct control (review on #7922): the
    // `&&` chain is unchanged, so the edit is one comparison's, owned by
    // the guard's predicate.
    let lib = two_input_owner("        p if other > 0 && 100 < p => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if other > 0 && 100 < p => 1,",
        "        p if other > 0 && 100 <= p => 1,",
        OTHER_TESTS,
    )?;
    assert!(!findings.is_empty());
    assert!(
        findings
            .iter()
            .all(|finding| finding.probe.family.as_str() == "predicate"),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_raw_identifier_named_match_is_not_a_second_match() -> Result<(), String> {
    // Review on #7922: `r#match` is a name, so the guard binding still
    // reads the scrutinee's boundary inputs.
    let lib = tier("    let r#match = 0;\n    let _ = r#match;\n")
        .replace("p if 100 <= p", "p if 100 < p");
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
fn a_macro_may_declare_the_guard_binding_as_a_constant() -> Result<(), String> {
    // Control (review on #7922, #7863): `def!(p)` declares a constant `p`,
    // so the pattern compares it, not the scrutinee.
    let arms = "        p if 100 < p => 1,\n";
    let lib = format!(
        "macro_rules! def {{\n    ($n:ident) => {{\n        #[allow(non_upper_case_globals)]\n        const $n: i64 = 7;\n    }};\n}}\ndef!(p);\n\n{}",
        two_input_owner(arms)
    );
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => 1,",
        "        p if 100 <= p => 1,",
        OTHER_TESTS,
    )?;
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "macro constant");
    Ok(())
}

#[test]
fn a_statement_macro_in_the_owner_may_declare_the_guard_binding() -> Result<(), String> {
    // Control (review on #7922): `consts!();`, defined in another file,
    // may expand to `const p: i64 = 7;` scoped to the match.
    let lib = tier("    consts!();\n").replace("p if 100 <= p", "p if 100 < p");
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => Tier::Gold,",
        "        p if 100 <= p => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "statement macro");
    Ok(())
}

#[test]
fn an_item_macro_in_a_nested_module_may_declare_the_guard_binding() -> Result<(), String> {
    // Control (review on #7922): the call sits inside `mod inner`, below
    // the file's top level, next to the owner it may give a `const p`.
    let lib = format!(
        "pub use inner::{{Tier, tier}};\n\nmod inner {{\nconsts!();\n\n{}}}\n",
        tier("").replace("p if 100 <= p", "p if 100 < p")
    );
    let findings = findings_on_changed_line(
        &lib,
        "        p if 100 < p => Tier::Gold,",
        "        p if 100 <= p => Tier::Gold,",
        BOUNDARY_TESTS,
    )?;
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "nested item macro");
    Ok(())
}

#[test]
fn a_statement_macro_in_another_function_keeps_the_guard_binding() -> Result<(), String> {
    // Twin of the statement-macro control: a macro inside another
    // function scopes its items to that function, so the binding still
    // reads the scrutinee's boundary inputs.
    let lib = format!(
        "{}\npub fn helper() {{\n    consts!();\n}}\n",
        tier("").replace("p if 100 <= p", "p if 100 < p")
    );
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
fn a_let_in_the_guard_may_rebind_the_guard_binding() -> Result<(), String> {
    // Control (review on #7922): `p if let p = other && 100 < p` compares
    // `other`, and the guard has no brace to reject it by.
    let lib = two_input_owner("        p if let p = other && 100 < p => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if let p = other && 100 < p => 1,",
        "        p if let p = other && 100 <= p => 1,",
        OTHER_TESTS,
    )?;
    assert!(missing_values(&findings).is_empty(), "{findings:?}");
    assert_not_exposed(&findings, "let guard");
    Ok(())
}

#[test]
fn an_added_guard_conjunct_keeps_the_arm_head() -> Result<(), String> {
    // Twin of the deleted-conjunct control: adding `other > 0 &&` changes
    // the connectives, so the arm head stays.
    let lib = two_input_owner("        p if other > 0 && 100 < p => 1,\n");
    let findings = findings_on_changed_line(
        &lib,
        "        p if other > 0 && 100 < p => 1,",
        "        p if 100 <= p => 1,",
        OTHER_TESTS,
    )?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.probe.family.as_str() == "match_arm"),
        "{findings:?}"
    );
    Ok(())
}
