//! #4478: real UB #1440 helper refusal and public-API attribution controls.
//! The frozen source is a parser input; native UB execution is separate evidence.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!("data/uncertain-delivery/github_delivery.rs");
const RECEIPT: &str = include_str!("data/uncertain-delivery/receipt.json");
const OWNER: &str = "pub fn gate(input: u32) -> bool {\n    input >= 10\n}\n";
const OWNER_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn gate(input: u32) -> bool {\n-    input > 10\n+    input >= 10\n }\n";

struct Subject(PathBuf);

impl Subject {
    fn create(source: &str, diff: &str, delivery: bool) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let subject = Self(std::env::temp_dir().join(format!(
            "ripr-uncertain-helper-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(subject.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(subject.0.join("Cargo.toml"),
            "[package]\nname = \"uncertain-helper\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n")
            .map_err(|e| e.to_string())?;
        if delivery {
            std::fs::write(subject.0.join("src/lib.rs"), "mod github_delivery;\n")
                .map_err(|e| e.to_string())?;
        }
        std::fs::write(
            subject.0.join(if delivery {
                "src/github_delivery.rs"
            } else {
                "src/lib.rs"
            }),
            source,
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(subject.0.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        Ok(subject)
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        Ok(check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?
        .findings)
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn frozen_real_helper_keeps_its_callers_without_inventing_an_oracle() -> Result<(), String> {
    let receipt: serde_json::Value = serde_json::from_str(RECEIPT).map_err(|e| e.to_string())?;
    let diff = receipt["full_diff"]
        .as_str()
        .ok_or("frozen full diff exists")?;
    let findings = Subject::create(SOURCE, diff, true)?.findings()?;
    assert!(
        !findings.is_empty(),
        "real diff has analyzed production subjects"
    );
    let helper_start = SOURCE
        .lines()
        .position(|l| l.contains("fn uncertain_delivery_retry("))
        .ok_or("helper exists")?
        + 1;
    let helper_end = SOURCE
        .lines()
        .position(|l| l.contains("fn delivery_reservation_is_exclusive_and_contains_no_token("))
        .ok_or("next test exists")?;
    assert!(
        findings
            .iter()
            .all(|f| !(helper_start..helper_end).contains(&f.probe.location.line)),
        "cfg(test) helper is evidence, not a production probe inventory"
    );
    for name in [
        "lost_submission_response_retry_never_sends_another_review",
        "receipt_write_failure_retry_never_sends_another_review",
    ] {
        let rows: Vec<_> = findings
            .iter()
            .flat_map(|f| &f.related_tests)
            .filter(|t| t.name == name)
            .collect();
        assert!(!rows.is_empty(), "real caller remains related: {name}");
        assert!(
            rows.iter().all(|t| t.oracle.is_none()),
            "closure-containing helper is refused: {rows:?}"
        );
    }
    Ok(())
}

fn boundary(helper: &str, caller: &str) -> Result<Finding, String> {
    let source = format!(
        "{OWNER}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n{helper}\n#[test]\nfn boundary() -> Result<(), ()> {{\n{caller}\n}}\n}}\n"
    );
    let findings = Subject::create(&source, OWNER_DIFF, false)?.findings()?;
    let mut selected = findings
        .into_iter()
        .filter(|f| f.probe.family == ProbeFamily::Predicate && f.probe.location.line == 2);
    let finding = selected.next().ok_or("changed predicate exists")?;
    assert!(selected.next().is_none(), "unique changed predicate");
    Ok(finding)
}

#[test]
fn direct_and_plain_helper_oracles_are_credited_while_closure_helpers_stay_refused()
-> Result<(), String> {
    let direct = boundary("", "assert_eq!(gate(10), true);\nOk(())")?;
    assert_eq!(
        direct.class,
        ripr::ExposureClass::Exposed,
        "direct positive: {direct:?}"
    );
    let plain = boundary(
        "fn check(input: u32, want: bool) -> Result<(), ()> {\n    assert_eq!(gate(input), want);\n    Ok(())\n}",
        "check(10, true)",
    )?;
    assert!(
        plain.related_tests.iter().any(|t| t.name == "boundary"),
        "plain caller is retained"
    );
    // Since #8089 a returned plain fallible helper's panic assertion is
    // credited to the calling test (the refusal this witness once retained).
    assert_eq!(
        plain.class,
        ripr::ExposureClass::Exposed,
        "plain fallible helper assertion is now credited: {plain:?}"
    );
    assert!(
        plain
            .related_tests
            .iter()
            .filter(|t| t.name == "boundary")
            .all(|t| t.oracle.as_deref() == Some("assert_eq!(gate(input), want);")),
        "plain helper oracle is the copied helper assertion: {plain:?}"
    );
    assert!(
        plain
            .evidence
            .iter()
            .all(|e| !e.contains("could not identify the test function uniquely")),
        "plain helper no longer reaches the downstream uniqueness refusal: {plain:?}"
    );
    assert!(
        plain.related_tests.iter().any(|t| t.name == "boundary"
            && format!("{:?}", t.relation_reason) == "Some(DirectOwnerCall)"),
        "plain helper's owner call was copied: {plain:?}"
    );
    let factory = boundary(
        "fn check() -> Result<(), ()> {\n    let make_input = || 10;\n    assert_eq!(gate(make_input()), true);\n    Ok(())\n}",
        "check()",
    )?;
    assert!(
        factory.related_tests.iter().any(|t| t.name == "boundary"),
        "factory caller is retained"
    );
    assert!(
        factory
            .related_tests
            .iter()
            .filter(|t| t.name == "boundary")
            .all(|t| t.oracle.is_none()),
        "invoked closure is still outside helper admission: {factory:?}"
    );
    assert!(
        factory
            .related_tests
            .iter()
            .filter(|t| t.name == "boundary")
            .all(|t| format!("{:?}", t.relation_reason) != "Some(DirectOwnerCall)"),
        "factory helper owner call was not copied: {factory:?}"
    );
    assert!(
        factory
            .evidence
            .iter()
            .all(|e| !e.contains("assertion not credited:")),
        "factory assertion never reached downstream admission: {factory:?}"
    );
    Ok(())
}

#[test]
fn sibling_uninvoked_and_wrong_binding_do_not_gain_helper_observation() -> Result<(), String> {
    for (helper, caller) in [
        (
            "mod sibling {\n    pub fn check() -> Result<(), ()> {\n        assert_eq!(super::gate(10), true);\n        Ok(())\n    }\n}",
            "sibling::check()",
        ),
        (
            "fn check(input: u32, want: bool) -> Result<(), ()> {\n    assert_eq!(gate(input), want);\n    Ok(())\n}",
            "let _later = || check(10, true);\nOk(())",
        ),
        // Keep this assertion in the test itself: helper-line context refusal
        // must not mask wrong-binding observation.
        (
            "",
            "let actual = gate(10);\nlet other = true;\nassert_eq!(other, true);\nOk(())",
        ),
    ] {
        let finding = boundary(helper, caller)?;
        assert_ne!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "negative must not expose owner: {finding:?}"
        );
    }
    Ok(())
}
