//! #6645: what ripr reports today for each family of provider assertion macro
//! (`claims`, `approx`, `float-cmp`) beside the std forms it already grades.
//!
//! The table is an inventory of current behavior, not a statement of what is
//! desirable. Each row names the form, the oracle ripr reads from it and the
//! limitation it names. A later credit slice must flip a row on purpose, with
//! its own controls; a row that moves by accident fails here.
//!
//! Families:
//! - presence or variant only: `assert_some!`, `assert_ok!`, `assert_err!`;
//! - direct payload equality: `assert_some_eq!`, `assert_ok_eq!`;
//! - payload extraction followed by a std assertion (credited elsewhere,
//!   see `claims_payload_owner_pin.rs`; here only as a contrast row);
//! - bounded numeric tolerance: `approx` and `float-cmp` equality and
//!   inequality forms;
//! - debug-only checks and a local no-op macro of a provider's name.
//!
//! The provider crates are never compiled: ripr reads the manifest and the
//! source, so a dependency line is enough to make an import verifiable.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

const OPTION_OWNER: &str =
    "pub fn pick(v: i32) -> Option<i32> {\n    if v >= 0 { Some(v * 2) } else { None }\n}\n";
const OPTION_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn pick(v: i32) -> Option<i32> {\n-    if v > 0 { Some(v * 2) } else { None }\n+    if v >= 0 { Some(v * 2) } else { None }\n }\n";
const RESULT_OWNER: &str = "pub fn parse(v: i32) -> Result<i32, String> {\n    if v >= 0 { Ok(v * 2) } else { Err(\"neg\".to_string()) }\n}\n";
const RESULT_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn parse(v: i32) -> Result<i32, String> {\n-    if v > 0 { Ok(v * 2) } else { Err(\"neg\".to_string()) }\n+    if v >= 0 { Ok(v * 2) } else { Err(\"neg\".to_string()) }\n }\n";
const FLOAT_OWNER: &str =
    "pub fn scale(v: i32) -> f64 {\n    if v >= 3 { v as f64 * 1.5 } else { 0.0 }\n}\n";
const FLOAT_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn scale(v: i32) -> f64 {\n-    if v > 3 { v as f64 * 1.5 } else { 0.0 }\n+    if v >= 3 { v as f64 * 1.5 } else { 0.0 }\n }\n";

/// An owner, the diff that changes it, and the probe family of the finding
/// under test.
type Subject = (&'static str, &'static str, &'static str);
const OPTION: Subject = (OPTION_OWNER, OPTION_DIFF, "predicate");
const RESULT: Subject = (RESULT_OWNER, RESULT_DIFF, "predicate");
const FLOAT: Subject = (FLOAT_OWNER, FLOAT_DIFF, "predicate");
/// A changed `Ok(..)` tail, which the owner-return pin can credit.
const DIVISION: Subject = (DIVISION_OWNER, DIVISION_DIFF, "return_value");

const DIVISION_OWNER: &str = "pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n    if b == 0 {\n        return Err(());\n    }\n    Ok(a / b)\n}\n";
const DIVISION_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n     if b == 0 {\n         return Err(());\n     }\n-    Ok(a / b + 1)\n+    Ok(a / b)\n }\n";

const CLAIMS: &str = "claims = \"0.8\"";
const APPROX: &str = "approx = \"0.5\"";
const FLOAT_CMP: &str = "float-cmp = \"0.9\"";

const UNRESOLVED: &str = "rust_macro_wrapped_assertion_unresolved";

struct Row {
    name: &'static str,
    owner: &'static str,
    diff: &'static str,
    /// The probe family of the finding under test.
    family: &'static str,
    dependency: &'static str,
    imports: &'static str,
    body: &'static str,
    /// The oracle kind and strength ripr reads from the test.
    oracle: (&'static str, &'static str),
    /// The named static limitation, when the finding carries one.
    limit: Option<&'static str>,
}

const fn row(
    name: &'static str,
    (owner, diff, family): Subject,
    dependency: &'static str,
    imports: &'static str,
    body: &'static str,
    oracle: (&'static str, &'static str),
    limit: Option<&'static str>,
) -> Row {
    Row {
        name,
        owner,
        diff,
        family,
        dependency,
        imports,
        body,
        oracle,
        limit,
    }
}

fn rows() -> Vec<Row> {
    let none = ("unknown", "none");
    vec![
        // Presence or variant only.
        row(
            "claims_assert_some",
            OPTION,
            CLAIMS,
            "use claims::assert_some;",
            "assert_some!(pick(5));",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "claims_assert_ok",
            RESULT,
            CLAIMS,
            "use claims::assert_ok;",
            "assert_ok!(parse(5));",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "claims_assert_err",
            RESULT,
            CLAIMS,
            "use claims::assert_err;",
            "assert_err!(parse(-5));",
            none,
            Some(UNRESOLVED),
        ),
        // Direct payload equality: exact by the macro's contract, but ripr
        // does not yet read it, so it stays limited rather than credited.
        row(
            "claims_assert_some_eq",
            OPTION,
            CLAIMS,
            "use claims::assert_some_eq;",
            "assert_some_eq!(pick(5), 10);",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "claims_assert_ok_eq",
            RESULT,
            CLAIMS,
            "use claims::assert_ok_eq;",
            "assert_ok_eq!(parse(5), 10);",
            none,
            Some(UNRESOLVED),
        ),
        // Payload extraction then a std assertion: the verified-import path.
        row(
            "claims_payload_extraction_then_assert_eq",
            RESULT,
            CLAIMS,
            "use claims::assert_ok;",
            "let v = assert_ok!(parse(5));\nassert_eq!(v, 10);",
            ("exact_value", "strong"),
            None,
        ),
        row(
            "claims_payload_discarded",
            RESULT,
            CLAIMS,
            "use claims::assert_ok;",
            "let v = assert_ok!(parse(5));\nlet _ = v;\nassert_eq!(1, 1);",
            ("relational_check", "weak"),
            None,
        ),
        // Bounded numeric tolerance: equality and inequality directions.
        row(
            "approx_relative_eq",
            FLOAT,
            APPROX,
            "use approx::assert_relative_eq;",
            "assert_relative_eq!(scale(5), 7.5, epsilon = 1e-9);",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "approx_relative_ne",
            FLOAT,
            APPROX,
            "use approx::assert_relative_ne;",
            "assert_relative_ne!(scale(5), 0.0, epsilon = 1e-9);",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "approx_abs_diff_eq",
            FLOAT,
            APPROX,
            "use approx::assert_abs_diff_eq;",
            "assert_abs_diff_eq!(scale(5), 7.5, epsilon = 1e-9);",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "approx_ulps_eq",
            FLOAT,
            APPROX,
            "use approx::assert_ulps_eq;",
            "assert_ulps_eq!(scale(5), 7.5);",
            none,
            Some(UNRESOLVED),
        ),
        row(
            "float_cmp_assert_approx_eq",
            FLOAT,
            FLOAT_CMP,
            "use float_cmp::assert_approx_eq;",
            "assert_approx_eq!(f64, scale(5), 7.5);",
            none,
            Some(UNRESOLVED),
        ),
        // Std forms, for contrast with the rows above.
        row(
            "std_assert_gt_is_relational",
            FLOAT,
            "",
            "",
            "assert!(scale(5) > 1.0);",
            ("relational_check", "weak"),
            None,
        ),
        row(
            "std_is_some_is_presence_only",
            OPTION,
            "",
            "",
            "assert!(pick(5).is_some());",
            ("smoke_only", "smoke"),
            None,
        ),
        // Debug-only: the check is compiled out under `--release`, so ripr
        // reads no oracle. Where the `assert_eq!` twin would be credited, the
        // gap is withheld under a named profile limit (RIPR-SPEC-0240); where
        // the twin would not be credited, the gap stays by design.
        row(
            "std_debug_assert_eq_pin_is_withheld_under_a_profile_limit",
            DIVISION,
            "",
            "",
            "debug_assert_eq!(checked_div(17, 5), Ok(3));",
            none,
            Some("rust_debug_assertion_profile_unresolved"),
        ),
        row(
            "std_debug_assert_eq_off_the_boundary_keeps_its_gap",
            RESULT,
            "",
            "",
            "debug_assert_eq!(parse(5), Ok(10));",
            none,
            None,
        ),
        // A local macro with a provider's name is not the provider's macro.
        row(
            "local_noop_macro_named_like_a_provider",
            OPTION,
            "",
            "macro_rules! assert_some { ($e:expr) => { let _ = $e; }; }",
            "assert_some!(pick(5));",
            none,
            Some(UNRESOLVED),
        ),
    ]
}

#[test]
fn provider_assertion_macro_inventory_matches_current_behavior() -> Result<(), String> {
    let root_dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("rust_assertion_provider_macro_inventory");
    let mut mismatches = Vec::new();
    for case in rows() {
        let source = format!(
            "{}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n{}\n#[test]\nfn exercises_the_owner() {{\n{}\n}}\n}}\n",
            case.owner, case.imports, case.body
        );
        let root = root_dir.join(case.name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"inventory_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dev-dependencies]\n{}\n[workspace]\n",
                case.dependency
            ),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), case.diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        // Every subject changes one predicate. A `Result` owner also yields an
        // error-path finding for the same line, so the finding under test is
        // chosen by its probe family, and exactly one must match.
        let predicates: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.probe.family.as_str() == case.family)
            .collect();
        let [finding] = predicates.as_slice() else {
            return Err(format!(
                "{}: expected one {} finding, got {}",
                case.name,
                case.family,
                predicates.len()
            ));
        };
        let test = finding
            .related_tests
            .first()
            .ok_or_else(|| format!("{}: no related test", case.name))?;
        let oracle = (test.oracle_kind.as_str(), test.oracle_strength.as_str());
        let limit = finding.static_limit_kind.as_ref().map(|kind| kind.as_str());
        if oracle != case.oracle || limit != case.limit {
            mismatches.push(format!(
                "{}: expected oracle {:?} limit {:?}, got oracle {:?} limit {:?}",
                case.name, case.oracle, case.limit, oracle, limit
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "inventory rows moved:\n{}",
        mismatches.join("\n")
    );
    Ok(())
}
