//! Check a diff, then check it again after the tests are strengthened.
//!
//! The sample's tests reach both changed functions but assert only
//! `total > 0` and `is_err()`, so the first check reports each changed
//! behavior as `weakly_exposed`. The second check runs after the tests pin
//! the discount threshold and the exact error variant. Both results are
//! static evidence about the tests' assertions; neither runs a mutant.

use ripr::{CheckOutput, check_workspace};
use std::fmt::Write as _;
use std::path::Path;

#[path = "support/sample_workspace.rs"]
mod sample_workspace;

/// `tests/pricing.rs` once it pins the changed behavior: the equality
/// boundary of the discount threshold, and the exact error and success values
/// of the currency check.
const STRENGTHENED_TESTS: &str = r#"use ripr_sample::{InvoiceError, Quote};

#[test]
fn premium_customer_gets_discount() {
    let quote = ripr_sample::price(10_000, 100);
    assert!(quote.total > 0);
}

#[test]
fn discount_starts_at_the_threshold() {
    let quote = ripr_sample::price(100, 100);
    assert_eq!(quote, Quote { total: 0, discount_applied: true });
}

#[test]
fn rejects_bad_currency() {
    let result = ripr_sample::validate_currency("XYZ");
    assert_eq!(result, Err(InvoiceError::InvalidCurrency));
}

#[test]
fn accepts_usd() {
    assert_eq!(ripr_sample::validate_currency("USD"), Ok(()));
}
"#;

fn describe(report: &mut String, heading: &str, root: &Path, output: &CheckOutput) {
    let _ = writeln!(report, "{heading}: {} findings", output.findings.len());
    for finding in &output.findings {
        let location = &finding.probe.location;
        let file = location.file.strip_prefix(root).unwrap_or(&location.file);
        let _ = writeln!(
            report,
            "- {}:{} {}: {}",
            file.display(),
            location.line,
            finding.probe.family.as_str(),
            finding.class.as_str()
        );
        if let Some(missing) = finding.missing.first() {
            let _ = writeln!(report, "    missing: {missing}");
        }
    }
}

pub fn report() -> Result<String, String> {
    let workspace = sample_workspace::SampleWorkspace::create("check_diff")?;
    let mut report = String::new();

    let before = check_workspace(workspace.input())?;
    let _ = writeln!(report, "schema: {}", before.schema_version);
    describe(&mut report, "tests as shipped", &workspace.root, &before);

    let tests = workspace.root.join("tests/pricing.rs");
    std::fs::write(&tests, STRENGTHENED_TESTS)
        .map_err(|error| format!("write {} failed: {error}", tests.display()))?;
    let after = check_workspace(workspace.input())?;
    describe(&mut report, "tests strengthened", &workspace.root, &after);
    let _ = writeln!(report, "sample workspace: {}", workspace.root.display());

    Ok(report)
}

pub fn main() -> Result<(), String> {
    print!("{}", report()?);
    Ok(())
}
