//! Public-API controls for an `if` condition forced to a literal `true` or
//! `false` (#14816, Stryker's ConditionalExpression operator).
//!
//! The literal spells no boundary. When the diff changes only that condition
//! in the owner, ripr folds the base-side owner and the current one at the
//! related tests' literal calls: an exact pin at an input that took the other
//! branch on the base side reaches the change, and a pin at an input that
//! already took the forced branch does not. An argument ripr cannot fold, a
//! line that changed more than the condition, or an owner name another
//! function may answer keeps `infection_unknown`.

use ripr::domain::StageState;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` holding `owner`, `extra` items and one test whose body is
/// `test_body`, under `#[test]` and the extra test `attribute` line.
fn source(owner: &str, extra: &str, attribute: &str, test_body: &str) -> String {
    format!(
        "{owner}
{extra}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    {attribute}
    fn checks_owner() {{
        {test_body}
    }}
}}
"
    )
}

/// The owner `fn f(x: i32) -> i32 { LINE }` on lines 1-3.
fn owner_lines(line: &str) -> Vec<String> {
    vec![
        "pub fn f(x: i32) -> i32 {".to_string(),
        format!("    {line}"),
        "}".to_string(),
    ]
}

/// A one-hunk diff of `src/lib.rs` from the `old` owner lines to the `new`
/// ones, both starting at line 1, with their shared lines as context.
fn owner_diff(old: &[String], new: &[String]) -> String {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut hunk = format!("@@ -1,{} +1,{} @@\n", old.len(), new.len());
    for line in &old[..prefix] {
        hunk.push_str(&format!(" {line}\n"));
    }
    for line in &old[prefix..old.len() - suffix] {
        hunk.push_str(&format!("-{line}\n"));
    }
    for line in &new[prefix..new.len() - suffix] {
        hunk.push_str(&format!("+{line}\n"));
    }
    for line in &old[old.len() - suffix..] {
        hunk.push_str(&format!(" {line}\n"));
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n{hunk}"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-literal-condition-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"literal-condition\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The one predicate finding for the forced literal on `line` when the owner changes from the
/// `old` lines to the `new` ones. Failing to find exactly one is a broken
/// fixture premise, not a verdict.
fn finding_on(
    old: &[String],
    new: &[String],
    line: usize,
    extra: &str,
    attribute: &str,
    test_body: &str,
) -> Result<Finding, String> {
    let findings = TempRepo::create(
        &source(&new.join("\n"), extra, attribute, test_body),
        &owner_diff(old, new),
    )?
    .findings()?;
    let mut matching = findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate)
        .filter(|finding| finding.probe.location.line == line)
        // A base-side line the diff removed keeps its own predicate probe
        // (`if x` of a condition that spanned lines); the forced literal's
        // probe is the subject.
        .filter(|finding| matches!(finding.probe.expression.trim(), "true" | "false"));
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one forced-literal predicate finding on line {line}, got {other:?} in {findings:?}"
        )),
    }
}

/// [`finding_on`] for line 2 of [`owner_lines`] changing from `removed` to
/// `added`, with `extra` items beside the owner.
fn condition_finding(
    removed: &str,
    added: &str,
    extra: &str,
    test_body: &str,
) -> Result<Finding, String> {
    finding_on(
        &owner_lines(removed),
        &owner_lines(added),
        2,
        extra,
        "",
        test_body,
    )
}

/// Before #14816 these read `infection_unknown`. The infection stage now
/// credits the pin; the predicate's reveal pairing still reads its check as
/// unconfirmed, so the class is `weakly_exposed`, not `exposed`.
#[test]
fn an_exact_pin_at_an_input_on_the_other_branch_infects_a_forced_condition() -> Result<(), String> {
    for (removed, added, test_body) in [
        (
            "if x > 0 { 1 } else { 2 }",
            "if true { 1 } else { 2 }",
            "assert_eq!(f(-1), 1);",
        ),
        (
            "if x > 0 { 1 } else { 2 }",
            "if false { 1 } else { 2 }",
            "assert_eq!(f(5), 2);",
        ),
    ] {
        let finding = condition_finding(removed, added, "", test_body)?;
        assert_eq!(
            finding.ripr.infect.state,
            StageState::Yes,
            "{added}: {finding:?}"
        );
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{added}: {finding:?}"
        );
    }
    Ok(())
}

/// `let _ = f(-1);` reaches the change (the old version takes the other
/// branch at `-1`), but nothing observes the result.
#[test]
fn a_bare_call_at_an_input_on_the_other_branch_is_infected_but_not_exposed() -> Result<(), String> {
    let finding = condition_finding(
        "if x > 0 { 1 } else { 2 }",
        "if true { 1 } else { 2 }",
        "",
        "let _ = f(-1);",
    )?;
    assert_eq!(finding.ripr.infect.state, StageState::Yes, "{finding:?}");
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// `f(5)` already took the forced branch on the base side, so the two
/// versions agree there and the pin cannot tell them apart.
#[test]
fn a_pin_at_an_input_already_on_the_forced_branch_is_not_exposed() -> Result<(), String> {
    let finding = condition_finding(
        "if x > 0 { 1 } else { 2 }",
        "if true { 1 } else { 2 }",
        "",
        "assert_eq!(f(5), 1);",
    )?;
    assert_eq!(finding.ripr.infect.state, StageState::Weak, "{finding:?}");
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// An argument the fold does not read (`let v = -1`) leaves the change's
/// infection unknown rather than crediting or ruling it out.
#[test]
fn an_unread_argument_keeps_infection_unknown() -> Result<(), String> {
    let finding = condition_finding(
        "if x > 0 { 1 } else { 2 }",
        "if true { 1 } else { 2 }",
        "",
        "let v = -1;\n        assert_eq!(f(v), 1);",
    )?;
    assert_eq!(
        finding.probe.expression.trim(),
        "true",
        "premise: {finding:?}"
    );
    assert_eq!(
        finding.ripr.infect.state,
        StageState::Unknown,
        "{finding:?}"
    );
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Shapes the reading refuses, so none is credited: a line changed beyond
/// its condition, a second function a test's `f(..)` may name, an `if let`
/// condition (its pattern binds and tests, which a literal cannot stand in
/// for), and a condition that spans lines on the base side.
#[test]
fn a_wider_edit_a_twin_owner_an_if_let_or_a_multi_line_condition_is_refused() -> Result<(), String>
{
    let forced = owner_lines("if true { 1 } else { 2 }");
    let if_let_old = vec![
        "pub fn f(x: Option<i32>) -> i32 {".to_string(),
        "    if let Some(_) = x { 1 } else { 2 }".to_string(),
        "}".to_string(),
    ];
    let if_let_new = vec![
        "pub fn f(x: Option<i32>) -> i32 {".to_string(),
        "    if true { 1 } else { 2 }".to_string(),
        "}".to_string(),
    ];
    let multi_line_old = vec![
        "pub fn f(x: i32) -> i32 {".to_string(),
        "    if x".to_string(),
        "        > 0 { 1 } else { 2 }".to_string(),
        "}".to_string(),
    ];
    for (label, old, new, extra, test_body) in [
        (
            "wider edit",
            owner_lines("if x > 0 { 3 } else { 2 }"),
            forced.clone(),
            "",
            "assert_eq!(f(-1), 1);",
        ),
        (
            "twin owner",
            owner_lines("if x > 0 { 1 } else { 2 }"),
            forced.clone(),
            "pub mod other {\n    pub fn f(x: i32) -> i32 {\n        x\n    }\n}\n",
            "assert_eq!(f(-1), 1);",
        ),
        (
            "if let",
            if_let_old,
            if_let_new,
            "",
            "assert_eq!(f(None), 1);",
        ),
        (
            "multi-line condition",
            multi_line_old,
            forced.clone(),
            "",
            "assert_eq!(f(-1), 1);",
        ),
    ] {
        let finding = finding_on(&old, &new, 2, extra, "", test_body)?;
        assert_eq!(
            finding.probe.expression.trim(),
            "true",
            "{label} premise: {finding:?}"
        );
        assert_ne!(
            finding.ripr.infect.state,
            StageState::Yes,
            "{label}: {finding:?}"
        );
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {finding:?}"
        );
    }
    Ok(())
}

/// An `else if` condition is an `if` condition, so forcing it reads the same
/// way: at `-1` the base side falls through to `else` (2) and the forced
/// version returns 1, so the pin reaches the change; at `-9` the first arm
/// answers on both sides, so the pin reaches nothing.
#[test]
fn a_forced_else_if_condition_is_read_like_an_if() -> Result<(), String> {
    let lines = |condition: &str| {
        vec![
            "pub fn f(x: i32) -> i32 {".to_string(),
            "    if x < -5 {".to_string(),
            "        0".to_string(),
            format!("    }} else if {condition} {{"),
            "        1".to_string(),
            "    } else {".to_string(),
            "        2".to_string(),
            "    }".to_string(),
            "}".to_string(),
        ]
    };
    for (test_body, expected) in [
        ("assert_eq!(f(-1), 1);", StageState::Yes),
        ("assert_eq!(f(-9), 0);", StageState::Weak),
    ] {
        let finding = finding_on(&lines("x > 0"), &lines("true"), 4, "", "", test_body)?;
        assert_eq!(
            finding.probe.expression.trim(),
            "true",
            "premise: {finding:?}"
        );
        assert_eq!(
            finding.ripr.infect.state, expected,
            "{test_body}: {finding:?}"
        );
    }
    Ok(())
}

/// An ignored test never runs and a `should_panic` test passes on either
/// version by panicking, so neither supplies a call that reaches the change.
#[test]
fn an_ignored_or_should_panic_test_supplies_no_reaching_call() -> Result<(), String> {
    for attribute in ["#[ignore]", "#[should_panic]"] {
        for test_body in ["let _ = f(-1);", "assert_eq!(f(-1), 1);"] {
            let finding = finding_on(
                &owner_lines("if x > 0 { 1 } else { 2 }"),
                &owner_lines("if true { 1 } else { 2 }"),
                2,
                "",
                attribute,
                test_body,
            )?;
            assert_eq!(
                finding.probe.expression.trim(),
                "true",
                "premise: {finding:?}"
            );
            assert_ne!(
                finding.ripr.infect.state,
                StageState::Yes,
                "{attribute} {test_body}: {finding:?}"
            );
        }
    }
    // Control: the same call in a plain test reaches the change.
    let finding = condition_finding(
        "if x > 0 { 1 } else { 2 }",
        "if true { 1 } else { 2 }",
        "",
        "let _ = f(-1);",
    )?;
    assert_eq!(finding.ripr.infect.state, StageState::Yes, "{finding:?}");
    Ok(())
}
