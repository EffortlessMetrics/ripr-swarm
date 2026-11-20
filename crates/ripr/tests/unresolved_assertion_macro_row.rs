//! Public-API controls for a related test whose only check is a local
//! assertion-like macro ripr does not classify (#15237), such as strsim's
//! test-module `assert_delta!`.
//!
//! The macro's hand-rolled body earns no oracle, but the row must not claim
//! the test "has no assertion": it reads `assertion_admission_unproven` and
//! the finding names the test and macro. The class, oracle and finding-level
//! next step stay as they are. Every negative below keeps the absence claim.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn bump(x: f64) -> f64 {\n    x + 2.0\n}\n";

/// The strsim shape: a hand-rolled `if .. { panic!(..) }` check.
const ASSERT_DELTA: &str = "    macro_rules! assert_delta {\n        ($x:expr, $y:expr, $d:expr) => {\n            if ($x - $y).abs() > $d {\n                panic!(\"not within delta\");\n            }\n        };\n    }\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn bump(x: f64) -> f64 {
-    x + 1.0
+    x + 2.0
 }
";

const LIMIT_LINE: &str =
    "assertion-like macro not classified: test `holds` invokes `assert_delta!`";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_body: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unresolved-assertion-macro-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"unresolved-assertion-macro\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let source = format!(
            "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{ASSERT_DELTA}\n    #[test]\n    fn holds() {{\n{test_body}\n    }}\n}}\n"
        );
        std::fs::write(this.root.join("src/lib.rs"), source)
            .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

    /// The one return-value finding on the changed line 2.
    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut on_line = output.findings.into_iter().filter(|finding| {
            finding.probe.location.line == 2 && finding.probe.family == ProbeFamily::ReturnValue
        });
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one return-value finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The `holds` row's miss token, whether the finding names the macro limit,
/// the finding's class and its next step.
type Reading = (Option<&'static str>, bool, &'static str, Option<String>);

fn read(test_body: &str) -> Result<Reading, String> {
    let finding = TempRepo::create(test_body)?.finding()?;
    // Fixture construction: the intended test is related to the owner.
    let row = finding
        .related_tests
        .iter()
        .find(|test| test.name == "holds")
        .ok_or_else(|| format!("premise: `holds` is related\n{test_body}"))?;
    let named = finding
        .evidence
        .iter()
        .any(|line| line.starts_with(LIMIT_LINE));
    Ok((
        row.miss.map(|miss| miss.as_str()),
        named,
        finding.class.as_str(),
        finding.recommended_next_step.clone(),
    ))
}

/// A body that relates `holds` and has no check of any kind.
const PLAIN: &str = "        let _ = bump(1.0);";

#[test]
fn an_unclassified_assertion_macro_is_not_read_as_missing_an_assertion() -> Result<(), String> {
    let (control_miss, control_named, control_class, _) = read(PLAIN)?;
    assert_eq!(control_miss, Some("no_assertion"), "premise: control row");
    assert!(!control_named, "premise: control names nothing");
    let body = "        assert_delta!(3.0, bump(1.0), 0.001);";
    let finding = TempRepo::create(body)?.finding()?;
    let row = finding
        .related_tests
        .iter()
        .find(|test| test.name == "holds")
        .ok_or("premise: `holds` is related")?;
    assert_eq!(row.oracle, None, "no oracle may be granted: {row:?}");
    let (miss, named, class, _) = read(body)?;
    assert_eq!(miss, Some("assertion_admission_unproven"));
    assert!(named, "the finding names the test and macro");
    assert_eq!(class, control_class, "the class does not move");
    Ok(())
}

#[test]
fn other_bodies_keep_the_absence_claim() -> Result<(), String> {
    for body in [
        PLAIN,
        "        println!(\"{}\", bump(1.0));",
        "        let _values = vec![bump(1.0)];",
        "        // assert_delta!(3.0, bump(1.0), 0.001);\n        let _ = bump(1.0);",
        "        let _text = \"assert_delta!(3.0, bump(1.0), 0.001)\";\n        let _ = bump(1.0);",
    ] {
        let (miss, named, _, _) = read(body)?;
        if named || miss != Some("no_assertion") {
            return Err(format!(
                "this body must keep its absence claim: miss={miss:?} named={named}\n{body}"
            ));
        }
    }
    Ok(())
}

/// The #7089 shape: the directly related `rate_triples` checks the owner only
/// through an unclassified local macro, and a proximity-only test holds a
/// recognized `assert!` oracle on another function, so the finding is
/// `weakly_exposed`.
const RATE_SOURCE: &str = "pub fn inner_rate(x: i64) -> i64 {
    3 * x
}
pub fn other(x: i64) -> i64 {
    x + 1
}
#[cfg(test)]
mod tests {
    use super::*;
    macro_rules! assert_rate {
        ($actual:expr, $expected:expr) => {
            if $actual != $expected {
                panic!(\"rate mismatch\");
            }
        };
    }
    #[test]
    fn rate_triples() {
        assert_rate!(inner_rate(4), 12);
    }
    #[test]
    fn other_is_positive() {
        assert!(other(1) > 0);
    }
}
";

const RATE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn inner_rate(x: i64) -> i64 {
-    x * 3
+    3 * x
 }
";

/// #15237 review: rewriting `rate_triples`'s row must not hide the #7089
/// macro limit. The row reads `assertion_admission_unproven`, and the finding
/// still carries `rust_macro_wrapped_assertion_unresolved` naming the macro,
/// so a proximity oracle cannot turn it into an actionable gap.
#[test]
fn the_rewritten_row_keeps_the_macro_limit_over_a_proximity_oracle() -> Result<(), String> {
    let repo = TempRepo::create(PLAIN)?;
    std::fs::write(repo.root.join("src/lib.rs"), RATE_SOURCE)
        .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
    std::fs::write(repo.root.join("diff.patch"), RATE_DIFF)
        .map_err(|error| format!("write diff failed: {error}"))?;
    let finding = repo.finding()?;
    // Fixture construction: the direct test has no oracle, and the only
    // recognized one comes from the proximity-only test.
    let direct = finding
        .related_tests
        .iter()
        .find(|test| test.name == "rate_triples")
        .ok_or_else(|| format!("premise: `rate_triples` is related {finding:?}"))?;
    assert_eq!(direct.oracle, None, "{direct:?}");
    let proximity = finding
        .related_tests
        .iter()
        .find(|test| test.name == "other_is_positive")
        .ok_or_else(|| format!("premise: `other_is_positive` is related {finding:?}"))?;
    assert!(proximity.oracle.is_some(), "{proximity:?}");
    assert_eq!(finding.class.as_str(), "weakly_exposed");
    // The #15237 rewrite applies to the direct row.
    assert_eq!(
        direct.miss.map(|miss| miss.as_str()),
        Some("assertion_admission_unproven")
    );
    // The #7089 limit still fires.
    assert_eq!(
        finding.static_limit_kind,
        Some(ripr::domain::StaticLimitKind::RustMacroWrappedAssertionUnresolved),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("`rate_triples`") && line.contains("`assert_rate!`")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_std_assertion_is_an_oracle_and_never_names_the_limit() -> Result<(), String> {
    let (miss, named, _, _) = read("        assert_eq!(bump(1.0), 3.0);")?;
    if named || miss == Some("assertion_admission_unproven") || miss == Some("no_assertion") {
        return Err(format!(
            "a std assertion is read as an oracle, not this limit: miss={miss:?} named={named}"
        ));
    }
    Ok(())
}
