//! Rule 7 must establish the called function before treating an operand as primitive.
//! The custom Add leaves the numeric total unchanged, but a third field observes
//! its side effect. Dropping that operand therefore does not pass the fixture test.

use ripr::domain::{RelationReason, StageState};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SHADOW: &str = include_str!("fixtures/operand_callee_identity/let_shadow.rs");
const METHOD: &str = include_str!("fixtures/operand_callee_identity/method_confusion.rs");
const PRIMITIVE: &str = include_str!("fixtures/operand_callee_identity/primitive_control.rs");
const INITIALIZER: &str = "total_cents: subtotal + shipping";
const OPERAND_PIN: &str = "field_pinned_equal_to_operand";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str) -> Result<(Self, usize), String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-operand-callee-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self { root };
        std::fs::create_dir_all(repo.root.join("src"))
            .map_err(|error| format!("create source directory: {error}"))?;
        std::fs::write(
            repo.root.join("Cargo.toml"),
            "[package]\nname = \"operand-callee\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write manifest: {error}"))?;
        std::fs::write(repo.root.join("src/lib.rs"), source)
            .map_err(|error| format!("write source: {error}"))?;
        let matches = source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.trim() == format!("{INITIALIZER},"))
            .collect::<Vec<_>>();
        let [(offset, added)] = matches.as_slice() else {
            return Err("fixture must contain exactly one changed initializer".to_string());
        };
        let line = offset + 1;
        let removed = added.replace("subtotal + shipping", "subtotal");
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{removed}\n+{added}\n"
        );
        std::fs::write(repo.root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff: {error}"))?;
        Ok((repo, line))
    }

    fn field_finding(&self, line: usize) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut selected = output.findings.iter().filter(|finding| {
            finding.probe.location.line == line
                && finding.probe.family == ProbeFamily::FieldConstruction
                && finding.probe.expression.trim().trim_end_matches(',') == INITIALIZER
        });
        match (selected.next(), selected.next()) {
            (Some(finding), None) => Ok(finding.clone()),
            _ => Err(format!(
                "premise: exactly one field initializer finding at {line}: {:?}",
                output.findings
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_operator_credit(source: &str) -> Result<(), String> {
    let (repo, line) = TempRepo::create(source)?;
    let finding = repo.field_finding(line)?;
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| { test.relation_reason == Some(RelationReason::DirectOwnerCall) }),
        "premise: a direct owner test: {finding:?}"
    );
    assert_eq!(
        finding.ripr.reveal.discriminate.state,
        StageState::Yes,
        "{finding:?}"
    );
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains(OPERAND_PIN),
        "{finding:?}"
    );
    Ok(())
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert_eq!(source.matches(from).count(), 1, "fixture edit: {from}");
    source.replacen(from, to, 1)
}

#[test]
fn a_shadowed_callee_cannot_erase_an_observed_operator_effect() -> Result<(), String> {
    assert_operator_credit(SHADOW)?;
    for binding in [
        "use crate::flat_rate as shipping_cents;",
        "use crate::local_rates::shipping_cents;",
        "use crate::local_rates::*;",
        "declare!(fn shipping_cents(_: u64) -> Shipping { Shipping });",
    ] {
        let source = format!(
            "mod local_rates {{ pub use crate::flat_rate as shipping_cents; }}\n\
             macro_rules! declare {{ ($item:item) => {{ $item }}; }}\n{}",
            replace_once(SHADOW, "let shipping_cents = flat_rate;", binding)
        );
        assert_operator_credit(&source)?;
    }
    // An irrelevant same-name definition must not change the verdict.
    assert_operator_credit(&replace_once(
        SHADOW,
        "pub fn shipping_cents(_: u64) -> u64 {\n    0\n}\n",
        "",
    ))
}

#[test]
fn an_import_alias_cannot_borrow_a_method_return_type() -> Result<(), String> {
    assert_operator_credit(METHOD)?;
    assert_operator_credit(&replace_once(
        METHOD,
        "impl Rates {\n    pub fn shipping_cents(&self, _: u64) -> u64 {\n        0\n    }\n}\n",
        "",
    ))?;
    // Associated functions without a receiver are not free calls either.
    assert_operator_credit(&replace_once(METHOD, "&self, _: u64", "_: u64"))
}

#[test]
fn a_genuine_primitive_call_retains_the_operand_pin_downgrade() -> Result<(), String> {
    assert_primitive_pin(PRIMITIVE)?;
    for replacement in [
        "let shipping = shipping_cents(subtotal);\n    let shipping_cents = 7u64;",
        "let shipping = shipping_cents(subtotal)   ;",
        "let shipping_cents = shipping_cents(subtotal);\n    let shipping = shipping_cents;",
    ] {
        assert_primitive_pin(&replace_once(
            PRIMITIVE,
            "let shipping = shipping_cents(subtotal);",
            replacement,
        ))?;
    }
    Ok(())
}

fn assert_primitive_pin(source: &str) -> Result<(), String> {
    let (repo, line) = TempRepo::create(source)?;
    let finding = repo.field_finding(line)?;
    assert_eq!(
        finding.ripr.reveal.discriminate.state,
        StageState::Weak,
        "{finding:?}"
    );
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains(OPERAND_PIN),
        "{finding:?}"
    );
    // This intentionally small projection is fed by the real public producer.
    // The separate honesty manifest forbids blessing it as exposed; it is
    // an expected contract, not a captured execution or complete check report.
    let projected = serde_json::json!({
        "kind": "operand_callee_expected_test_projection",
        "summary": { "findings": 1 },
        "findings": [{
            "classification": finding.class.as_str(),
            "probe": { "family": finding.probe.family.as_str() },
            "discriminate": finding.ripr.reveal.discriminate.state.as_str(),
            "operand_pin": finding.ripr.reveal.discriminate.summary.contains(OPERAND_PIN),
        }],
    });
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/evidence-promotion-honesty-corpus/reports/operand-callee-later-binding.json"
    ))
    .map_err(|error| error.to_string())?;
    assert_eq!(
        projected, expected,
        "current API must satisfy the registered contract"
    );
    Ok(())
}
