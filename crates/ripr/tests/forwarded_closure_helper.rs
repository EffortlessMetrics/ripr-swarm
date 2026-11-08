//! Public-API controls for a same-file helper that forwards the test's
//! closure to another call before asserting (#9981 forwarding residual).
//!
//! The direct-invocation helper (`let got = f(); assert_eq!(got, want);`) is
//! the reference: forwarding to a same-file function that eagerly invokes the
//! closure must read exactly like it. Forwarding to a call ripr cannot see
//! (tracing's `with_default(subscriber, fun)`) earns no credit, but the row
//! must not claim the test "has no assertion": it names the helper and callee
//! as an analyzer limit and keeps the uncredited class.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn greet(n: &str) -> String {\n    format!(\"hello {n}\")\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn greet(n: &str) -> String {
-    format!(\"hi {n}\")
+    format!(\"hello {n}\")
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(helper: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-forwarded-closure-helper-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"forwarded-closure-helper\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let source = format!(
            "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{helper}\n    #[test]\n    fn a() {{\n        run_test(\"hello bob\", || greet(\"bob\"));\n    }}\n}}\n"
        );
        std::fs::write(this.root.join("src/lib.rs"), source)
            .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

    /// The one finding on the changed line 2.
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
        let mut on_line = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == 2);
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Test `a`'s row: its oracle text, oracle strength and miss token.
struct Row {
    oracle: Option<String>,
    oracle_strength: String,
    miss: Option<&'static str>,
}

fn row(finding: &Finding) -> Result<Row, String> {
    finding
        .related_tests
        .iter()
        .find(|test| test.name == "a")
        .map(|test| Row {
            oracle: test.oracle.clone(),
            oracle_strength: format!("{:?}", test.oracle_strength),
            miss: test.miss.map(|miss| miss.as_str()),
        })
        .ok_or_else(|| format!("premise: test `a` is related: {finding:?}"))
}

const DIRECT: &str = "    fn run_test(expected: &str, f: impl FnOnce() -> String) {\n        let got = f();\n        assert_eq!(got, expected);\n    }\n";

const SAME_FILE_FORWARD: &str = "    fn with_default(f: impl FnOnce() -> String) -> String {\n        f()\n    }\n    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let got = with_default(fun);\n        assert_eq!(got, expected);\n    }\n";

const NEVER_INVOKED: &str = "    fn run_test(expected: &str, _fun: impl FnOnce() -> String) {\n        assert_eq!(\"hello bob\", expected);\n    }\n";

#[test]
fn forwarding_to_a_same_file_invoker_reads_like_direct_invocation() -> Result<(), String> {
    let direct = TempRepo::create(DIRECT)?.finding()?;
    let forwarded = TempRepo::create(SAME_FILE_FORWARD)?.finding()?;

    let direct_row = row(&direct)?;
    // Premise: the #9996 reference credits the helper's assertion.
    assert_eq!(
        direct_row.oracle.as_deref(),
        Some("assert_eq!(got, expected);"),
        "{direct:?}"
    );
    let forwarded_row = row(&forwarded)?;
    assert_eq!(
        forwarded_row.oracle.as_deref(),
        Some("assert_eq!(got, expected);"),
        "{forwarded:?}"
    );
    assert_eq!(forwarded.class, direct.class, "{forwarded:?}");
    assert_eq!(forwarded_row.oracle_strength, direct_row.oracle_strength);
    assert_eq!(forwarded_row.miss, direct_row.miss);
    Ok(())
}

#[test]
fn forwarding_to_an_unresolved_callee_names_the_limit_instead_of_no_assertion() -> Result<(), String>
{
    // Control: a helper that never runs the closure keeps `no_assertion`
    // (#9997); the limitation must not reach it.
    let control = TempRepo::create(NEVER_INVOKED)?.finding()?;
    let control_row = row(&control)?;
    assert_eq!(control_row.oracle, None, "{control:?}");
    assert_eq!(control_row.miss, Some("no_assertion"), "{control:?}");
    assert!(
        !control
            .evidence
            .iter()
            .any(|line| line.starts_with("closure forwarded by helper: ")),
        "{control:?}"
    );

    for (callee, helper) in [
        (
            "with_default",
            "    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let (subscriber, handle) = mock(expected);\n        with_default(subscriber, fun);\n        handle.assert_finished();\n        assert_eq!(handle.seen(), expected);\n    }\n",
        ),
        (
            "tracing::subscriber::with_default",
            "    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let got = tracing::subscriber::with_default(expected, fun);\n        assert_eq!(got, expected);\n    }\n",
        ),
    ] {
        let finding = TempRepo::create(helper)?.finding()?;
        let forwarded_row = row(&finding)?;
        // No credit: no oracle, no strength, and never a promotion.
        assert_eq!(forwarded_row.oracle, None, "{callee}: {finding:?}");
        assert_eq!(
            forwarded_row.oracle_strength, control_row.oracle_strength,
            "{callee}"
        );
        assert_ne!(finding.class, ExposureClass::Exposed, "{callee}");
        assert_eq!(finding.class, control.class, "{callee}: {finding:?}");
        // The false absence claim is replaced by the analyzer-limit miss.
        assert_eq!(
            forwarded_row.miss,
            Some("assertion_admission_unproven"),
            "{callee}: {finding:?}"
        );
        let expected = format!(
            "closure forwarded by helper: test `a` calls helper `run_test`, which asserts after forwarding the closure to `{callee}`; whether it runs the closure is not established statically"
        );
        assert!(
            finding.evidence.contains(&expected),
            "{callee}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}
