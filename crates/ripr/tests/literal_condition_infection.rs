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
/// `test_body`.
fn source(owner: &str, extra: &str, test_body: &str) -> String {
    format!(
        "{owner}
{extra}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn checks_owner() {{
        {test_body}
    }}
}}
"
    )
}

/// The owner `fn f(x: i32) -> i32 { LINE }` on lines 1-3.
fn owner(line: &str) -> String {
    format!("pub fn f(x: i32) -> i32 {{\n    {line}\n}}")
}

/// A diff replacing line 2 of [`owner`].
fn line_diff(removed: &str, added: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn f(x: i32) -> i32 {{
-    {removed}
+    {added}
 }}
"
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

/// The one predicate finding on the changed line 2 when the owner's line
/// changes from `removed` to `added`, with `extra` items beside the owner.
fn condition_finding(
    removed: &str,
    added: &str,
    extra: &str,
    test_body: &str,
) -> Result<Finding, String> {
    let findings = TempRepo::create(
        &source(&owner(added), extra, test_body),
        &line_diff(removed, added),
    )?
    .findings()?;
    let mut matching = findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate)
        .filter(|finding| finding.probe.location.line == 2);
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one predicate finding on line 2, got {other:?} in {findings:?}"
        )),
    }
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

/// Controls that keep the ordinary `infection_unknown` reading: an argument
/// the fold does not read, a line that changed more than its condition, and
/// a second function a test's `f(..)` may name.
#[test]
fn an_unread_argument_a_wider_edit_or_a_twin_owner_keeps_infection_unknown() -> Result<(), String> {
    for (removed, extra, test_body) in [
        (
            "if x > 0 { 1 } else { 2 }",
            "",
            "let v = -1;\n        assert_eq!(f(v), 1);",
        ),
        ("if x > 0 { 3 } else { 2 }", "", "assert_eq!(f(-1), 1);"),
        (
            "if x > 0 { 1 } else { 2 }",
            "pub mod other {\n    pub fn f(x: i32) -> i32 {\n        x\n    }\n}\n",
            "assert_eq!(f(-1), 1);",
        ),
    ] {
        let finding = condition_finding(removed, "if true { 1 } else { 2 }", extra, test_body)?;
        assert_ne!(
            finding.ripr.infect.state,
            StageState::Yes,
            "{removed} / {test_body}: {finding:?}"
        );
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{removed} / {test_body}: {finding:?}"
        );
    }
    Ok(())
}
