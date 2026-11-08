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
        Self::create_with(helper, "")
    }

    /// `helper` and test `a`, plus `extra` items (more helpers and tests) in
    /// the same test module, so one finding relates several tests.
    fn create_with(helper: &str, extra: &str) -> Result<Self, String> {
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
            "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{helper}\n    #[test]\n    fn a() {{\n        run_test(\"hello bob\", || greet(\"bob\"));\n    }}\n{extra}}}\n"
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
    row_named(finding, "a")
}

fn row_named(finding: &Finding, name: &str) -> Result<Row, String> {
    finding
        .related_tests
        .iter()
        .find(|test| test.name == name)
        .map(|test| Row {
            oracle: test.oracle.clone(),
            oracle_strength: format!("{:?}", test.oracle_strength),
            miss: test.miss.map(|miss| miss.as_str()),
        })
        .ok_or_else(|| format!("premise: test `{name}` is related: {finding:?}"))
}

const DIRECT: &str = "    fn run_test(expected: &str, f: impl FnOnce() -> String) {\n        let got = f();\n        assert_eq!(got, expected);\n    }\n";

const SAME_FILE_FORWARD: &str = "    fn with_default(f: impl FnOnce() -> String) -> String {\n        f()\n    }\n    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let got = with_default(fun);\n        assert_eq!(got, expected);\n    }\n";

const ADD_ASSERTION: &str = "Add a meaningful assertion";
const NO_ASSERTION_FOUND: &str = "No assertion found on related tests";
const NO_OBSERVING_ASSERTION: &str = "Related tests were found, but no assertion appears to observe the changed value, error, field, or effect";
/// RIPR-SPEC-0221's refused-context observe summary (the existing wording
/// for an assertion ripr could not establish).
const CONTEXT_UNESTABLISHED: &str = "No statically established oracle: assertion execution or macro binding is unestablished (rust_assertion_context_unestablished)";

/// The finding-level next step, and the observe and discriminate summaries.
fn guidance(finding: &Finding) -> (String, String, String) {
    (
        finding.recommended_next_step.clone().unwrap_or_default(),
        finding.ripr.reveal.observe.summary.clone(),
        finding.ripr.reveal.discriminate.summary.clone(),
    )
}

const NEVER_INVOKED: &str = "    fn run_test(expected: &str, _fun: impl FnOnce() -> String) {\n        assert_eq!(\"hello bob\", expected);\n    }\n";

#[test]
fn forwarding_to_a_same_file_invoker_reads_like_direct_invocation() -> Result<(), String> {
    for forward in [
        SAME_FILE_FORWARD,
        // A non-closure parameter is forwarded bare as well; only the
        // parameter at the test's closure position is the forwarded closure.
        "    fn make(expected: &str) -> String {\n        expected.to_string()\n    }\n    fn with_default(want: String, f: impl FnOnce() -> String) -> String {\n        let got = f();\n        got + &want\n    }\n    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let want = make(expected);\n        let got = with_default(want, fun);\n        assert_eq!(got, expected);\n    }\n",
    ] {
        forwarding_reads_like_direct_invocation(forward)?;
    }
    Ok(())
}

#[test]
fn a_same_file_receiver_that_never_names_its_parameter_reads_like_never_invoked()
-> Result<(), String> {
    for receiver in [
        "    fn with_default(_f: impl FnOnce() -> String) -> String {\n        String::new()\n    }\n",
        "    fn with_default(_: impl FnOnce() -> String) -> String {\n        String::new()\n    }\n",
    ] {
        let finding = TempRepo::create(&format!(
            "{receiver}    fn run_test(expected: &str, fun: impl FnOnce() -> String) {{\n        let got = with_default(fun);\n        assert_eq!(got, expected);\n    }}\n"
        ))?
        .finding()?;
        let forwarded = row(&finding)?;
        assert_eq!(forwarded.oracle, None, "{finding:?}");
        assert_eq!(forwarded.miss, Some("no_assertion"), "{finding:?}");
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line.starts_with("closure forwarded by helper: ")),
            "{:?}",
            finding.evidence
        );
        let (step, _, discriminate) = guidance(&finding);
        assert!(step.starts_with(ADD_ASSERTION), "next step: {step}");
        assert_eq!(discriminate, NO_ASSERTION_FOUND);
    }
    Ok(())
}

fn forwarding_reads_like_direct_invocation(forward: &str) -> Result<(), String> {
    let direct = TempRepo::create(DIRECT)?.finding()?;
    let forwarded = TempRepo::create(forward)?.finding()?;

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
    // The control keeps the old finding-level wording: a test with no
    // assertion is still told to add one.
    let (step, observe, discriminate) = guidance(&control);
    assert!(step.starts_with(ADD_ASSERTION), "control next step: {step}");
    assert_eq!(observe, NO_OBSERVING_ASSERTION, "control observe");
    assert_eq!(discriminate, NO_ASSERTION_FOUND, "control discriminate");
    assert!(
        control
            .evidence
            .iter()
            .any(|line| line == NO_ASSERTION_FOUND)
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
        (
            // Tracing shape: the non-closure `span` is also forwarded bare,
            // inside a method chain, before the closure is forwarded.
            "with_default",
            "    fn run_test(span: &str, fun: impl FnOnce() -> String) {\n        let (subscriber, handle) = subscriber::mock().new_span(span).run_with_handle();\n        with_default(subscriber, fun);\n        assert_eq!(handle.seen(), \"hello bob\");\n    }\n",
        ),
        (
            "with_default",
            "    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let s = make(expected);\n        with_default(s, fun);\n        assert_eq!(s.seen(), \"hello bob\");\n    }\n",
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
        // The finding-level guidance defers to the refused-context wording:
        // no "add an assertion" for a test that has one.
        let (step, observe, discriminate) = guidance(&finding);
        assert!(!step.is_empty(), "{callee}: a next step is still offered");
        assert!(!step.contains(ADD_ASSERTION), "{callee} next step: {step}");
        assert_eq!(observe, CONTEXT_UNESTABLISHED, "{callee} observe");
        assert_ne!(discriminate, NO_ASSERTION_FOUND, "{callee} discriminate");
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == NO_ASSERTION_FOUND || line == NO_OBSERVING_ASSERTION),
            "{callee}: {:?}",
            finding.evidence
        );
        assert!(
            !finding
                .missing
                .iter()
                .any(|line| line.starts_with("No detected assertion")),
            "{callee}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// The tracing-shaped unresolved forward used by the mixed-finding controls.
const UNRESOLVED_FORWARD: &str = "    fn run_test(expected: &str, fun: impl FnOnce() -> String) {\n        let got = tracing::subscriber::with_default(expected, fun);\n        assert_eq!(got, expected);\n    }\n";

#[test]
fn a_genuine_no_assertion_test_beside_a_forwarded_limit_keeps_the_finding_wording()
-> Result<(), String> {
    // Test `b` hands its closure to a helper that never runs it: a real
    // `no_assertion` row. Only `a`'s row is rewritten; the finding keeps the
    // no-assertion summaries and next step because `b` really has none.
    let finding = TempRepo::create_with(
        UNRESOLVED_FORWARD,
        "    fn run_plain(expected: &str, _fun: impl FnOnce() -> String) {\n        assert_eq!(\"hello bob\", expected);\n    }\n    #[test]\n    fn b() {\n        run_plain(\"hello bob\", || greet(\"bob\"));\n    }\n",
    )?
    .finding()?;

    let forwarded = row_named(&finding, "a")?;
    let genuine = row_named(&finding, "b")?;
    assert_eq!(
        forwarded.miss,
        Some("assertion_admission_unproven"),
        "{finding:?}"
    );
    assert_eq!(genuine.miss, Some("no_assertion"), "{finding:?}");
    assert_eq!(genuine.oracle, None, "{finding:?}");
    let (step, observe, discriminate) = guidance(&finding);
    assert!(step.starts_with(ADD_ASSERTION), "next step: {step}");
    assert_eq!(observe, NO_OBSERVING_ASSERTION);
    assert_eq!(discriminate, NO_ASSERTION_FOUND);
    assert!(
        finding.evidence.iter().any(|line| {
            line.starts_with("closure forwarded by helper: test `a` calls helper `run_test`")
        }),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn an_oracle_bearing_test_beside_a_forwarded_limit_decides_the_finding_unchanged()
-> Result<(), String> {
    // Test `c` asserts on the owner directly. Its finding alone is the
    // reference: adding the forwarded-limit test `a` must not change the
    // class, the strongest oracle or the stage summaries.
    const DIRECT_TEST: &str =
        "    #[test]\n    fn c() {\n        assert_eq!(greet(\"bob\"), \"hello bob\");\n    }\n";
    // Reference: `a` calls a helper that never runs its closure, so it adds
    // no oracle either; `c` decides the finding.
    let reference = TempRepo::create_with(NEVER_INVOKED, DIRECT_TEST)?.finding()?;
    let mixed = TempRepo::create_with(UNRESOLVED_FORWARD, DIRECT_TEST)?.finding()?;

    let reference_c = row_named(&reference, "c")?;
    let mixed_c = row_named(&mixed, "c")?;
    // Premise: `c` supplies an oracle.
    assert!(reference_c.oracle.is_some(), "{reference:?}");
    assert_eq!(mixed.class, reference.class, "{mixed:?}");
    assert_eq!(mixed_c.oracle, reference_c.oracle);
    assert_eq!(mixed_c.oracle_strength, reference_c.oracle_strength);
    assert_eq!(
        format!("{:?}", mixed.ripr.reveal),
        format!("{:?}", reference.ripr.reveal),
        "no stage swap when another test has an oracle"
    );
    assert_eq!(mixed.recommended_next_step, reference.recommended_next_step);
    assert_ne!(
        mixed.ripr.reveal.observe.summary, CONTEXT_UNESTABLISHED,
        "{mixed:?}"
    );
    Ok(())
}
