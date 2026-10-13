//! Public consumer controls for uncertain owner reach (#7593).
//! Runtime doctest and example contrasts are independently retained with the PR.

use ripr::{CheckInput, ExposureClass, OutputFormat, ProbeFamily, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const INSPECT: &str = "The indexed related tests do not establish a call to the changed owner. Inspect existing tests, doctests and examples before changing assertions; they may already constrain the changed behavior. Add or strengthen a test only if that inspection finds a gap.";
const EXACT: &str = "Replace broad assertions with exact equality or a property that constrains the changed returned value.";

struct Fixture(PathBuf);

impl Fixture {
    fn new(fence: &str, assertion: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unindexed-guidance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname = \"doc_probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), format!(
            "/// Reads a big-endian u16.\n///\n/// ```{fence}\n/// assert_eq!(doc_probe::read_u16(&[0x12, 0x34]), 0x1234);\n/// ```\npub fn read_u16(bytes: &[u8]) -> u16 {{\n    (u16::from(bytes[0]) << 8) | u16::from(bytes[1])\n}}\n\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn current_test() {{ {assertion} }}\n}}\n"
        )).map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("change.diff"),
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -6,3 +6,3 @@\n pub fn read_u16(bytes: &[u8]) -> u16 {\n-    u16::from_be_bytes([bytes[0], bytes[1]])\n+    (u16::from(bytes[0]) << 8) | u16::from(bytes[1])\n }\n"
        ).map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn input(&self) -> CheckInput {
        CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("change.diff")),
            format: OutputFormat::Json,
            ..CheckInput::default()
        }
    }

    fn replace(&self, source: &str, diff: &str) -> Result<(), String> {
        std::fs::write(self.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        std::fs::write(self.0.join("change.diff"), diff).map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unindexed_doc_assertion_prompts_inspection_without_credit_or_gap_claim() -> Result<(), String> {
    for fence in ["", "ignore", "no_run", "compile_fail", "text"] {
        let fixture = Fixture::new(fence, "assert_eq!(2 + 2, 4);")?;
        let output = check_workspace(fixture.input())?;
        let finding = output
            .findings
            .iter()
            .find(|finding| finding.probe.family == ProbeFamily::ReturnValue)
            .ok_or("setup: expected a real return-value finding")?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{fence}: {finding:?}"
        );
        assert_eq!(finding.ripr.reach.state.as_str(), "weak", "{fence}");
        assert!(
            !finding.related_tests.is_empty(),
            "setup: indexed smoke test must exist"
        );
        assert!(
            finding
                .related_tests
                .iter()
                .all(|test| test.name == "current_test")
        );
        assert_eq!(
            finding.recommended_next_step.as_deref(),
            Some(INSPECT),
            "{fence}"
        );
        for format in [
            OutputFormat::Json,
            OutputFormat::Human,
            OutputFormat::HumanFull,
        ] {
            let rendered = render_check(&output, &format)?;
            let words = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(words.contains(INSPECT), "{fence}: {rendered}");
        }
        let explanation = ripr::explain_finding_with_input(fixture.input(), &finding.id)?;
        let words = explanation.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(words.contains(INSPECT), "{fence}: {explanation}");
    }
    Ok(())
}

#[test]
fn weak_direct_owner_assertion_keeps_exact_value_advice() -> Result<(), String> {
    let fixture = Fixture::new("ignore", "assert!(super::read_u16(&[0x12, 0x34]) > 0);")?;
    let output = check_workspace(fixture.input())?;
    let finding = output
        .findings
        .iter()
        .find(|finding| finding.probe.family == ProbeFamily::ReturnValue)
        .ok_or("setup: expected a real return-value finding")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(finding.ripr.reach.state.as_str(), "yes");
    assert!(finding.related_tests.iter().any(|test| {
        test.relation_reason
            .is_some_and(|reason| reason.as_str() == "direct_owner_call")
    }));
    assert_eq!(finding.recommended_next_step.as_deref(), Some(EXACT));
    Ok(())
}

#[test]
fn wrapper_error_retains_specific_guidance_for_proximity_reach() -> Result<(), String> {
    let fixture = Fixture::new("no_run", "assert_eq!(2 + 2, 4);")?;
    fixture.replace(
        "pub fn raw() -> Result<u16, std::io::Error> { Ok(0x1234) }\n/// ```no_run\n/// assert_eq!(doc_probe::boxed().unwrap(), 0x1234);\n/// ```\npub fn boxed() -> Result<u16, Box<dyn std::error::Error>> {\n    raw().map_err(|error| error.into())\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn current_test() { assert_eq!(2 + 2, 4); }\n}\n",
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -6,1 +6,1 @@\n-    raw().map_err(|error| Box::new(error) as Box<dyn std::error::Error>)\n+    raw().map_err(|error| error.into())\n",
    )?;
    let output = check_workspace(fixture.input())?;
    assert_eq!(output.findings.len(), 2, "setup: two wrapper probes");
    for finding in &output.findings {
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(finding.ripr.reach.state.as_str(), "weak");
        assert!(!finding.related_tests.is_empty());
        assert!(finding.related_tests.iter().all(|test| {
            test.relation_reason
                .is_some_and(|reason| reason.as_str() == "same_test_file")
        }));
        assert_eq!(
            finding.recommended_next_step.as_deref(),
            Some(
                "Typed static limitation (wrapper_error_binding_unresolved): ripr cannot statically establish that this boxed wrapper conversion carries the callee's error variant, so an existing exact downcast witness is not statically creditable. Verify via real mutation testing or deeper conversion modeling."
            )
        );
    }
    Ok(())
}

#[test]
fn assertion_helper_retains_oracle_inspection_guidance() -> Result<(), String> {
    let fixture = Fixture::new("no_run", "assert_eq!(2 + 2, 4);")?;
    fixture.replace(
        "/// ```no_run\n/// doc_probe::assert_expected(0x1234);\n/// ```\npub fn assert_expected(value: u16) {\n    if value >= 100 {\n        assert_eq!(value, 0x1234);\n    }\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn current_test() { assert_eq!(2 + 2, 4); }\n}\n",
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,1 +5,1 @@\n-    if value > 100 {\n+    if value >= 100 {\n",
    )?;
    let output = check_workspace(fixture.input())?;
    assert_eq!(
        output.findings.len(),
        1,
        "setup: one assertion-helper probe"
    );
    let finding = &output.findings[0];
    assert_eq!(finding.class, ExposureClass::PropagationUnknown);
    assert_eq!(finding.ripr.reach.state.as_str(), "weak");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("owner_shape: assertion_shaped"))
    );
    assert_eq!(
        finding.recommended_next_step.as_deref(),
        Some(
            "The changed owner is an assertion helper (its body is dominated by assert*/expect calls and nothing outside tests calls it); inspect the inputs and behavior this helper checks, including any existing boundary or negative-path assertions. Add or strengthen a test only if this inspection finds a gap. Compare the original and changed implementations with a focused run using the project's configured test runner (`cargo test` for ordinary Cargo tests). Static classification may remain unknown."
        )
    );
    Ok(())
}
