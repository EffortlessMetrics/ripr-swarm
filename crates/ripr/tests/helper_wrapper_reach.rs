//! Public-API controls for a private helper reached only through a tested
//! public wrapper (#6694, #6672; RIPR-SPEC-0159 relation, RIPR-SPEC-0186
//! pairing). Each case writes a small crate, changes the helper's boundary
//! line, and reads the predicate finding through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-helper-wrapper-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"wrapper_reach\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed helper is always on line 2: `fn is_bulk(qty: u32) -> bool {`
/// on line 1 and the edited `10 <= qty` on line 2.
fn bulk_source(wrapper_body: &str, tests: &str) -> String {
    format!(
        "fn is_bulk(qty: u32) -> bool {{\n    10 <= qty\n}}\n\npub fn order_discount(qty: u32) -> u32 {{\n{wrapper_body}\n}}\n\npub fn within_quota(used: u32) -> bool {{\n    used <= 200\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn quota_is_two_hundred() {{\n        assert_eq!(within_quota(200), true);\n    }}\n\n    #[test]\n    fn ten_items_earn_the_bulk_discount() {{\n{tests}\n    }}\n}}\n"
    )
}

const BULK_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn is_bulk(qty: u32) -> bool {\n-    qty >= 10\n+    10 <= qty\n }\n";

const FORWARDING: &str = "    if is_bulk(qty) {\n        5\n    } else {\n        0\n    }";
const BOUNDARY_TESTS: &str =
    "        assert_eq!(order_discount(10), 5);\n        assert_eq!(order_discount(9), 0);";

fn predicate_finding(source: &str, diff: &str) -> Result<Value, String> {
    family_finding(source, diff, "predicate")
}

fn family_finding(source: &str, diff: &str, family: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == family)
        .cloned()
        .ok_or_else(|| format!("no {family} finding: {json}"))
}

fn relation_of<'a>(finding: &'a Value, test: &str) -> Option<&'a str> {
    finding["related_tests"]
        .as_array()?
        .iter()
        .find(|related| related["name"] == test)
        .and_then(|related| related["relation_reason"].as_str())
}

fn discriminate_summary(finding: &Value) -> &str {
    finding["ripr"]["discriminate"]["summary"]
        .as_str()
        .unwrap_or_default()
}

#[test]
fn forwarding_wrapper_relates_and_pairs_its_boundary_pin() -> Result<(), String> {
    let finding = predicate_finding(&bulk_source(FORWARDING, BOUNDARY_TESTS), BULK_DIFF)?;
    assert_eq!(finding["probe"]["expression"], "10 <= qty", "{finding}");
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    // A same-file test that never calls the wrapper keeps file proximity.
    assert_eq!(
        relation_of(&finding, "quota_is_two_hundred"),
        Some("same_test_file"),
        "{finding}"
    );
    assert_eq!(finding["ripr"]["reach"]["state"], "yes", "{finding}");
    assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn wrapper_tests_that_miss_the_boundary_still_report_the_gap() -> Result<(), String> {
    let tests =
        "        assert_eq!(order_discount(12), 5);\n        assert_eq!(order_discount(3), 0);";
    let finding = predicate_finding(&bulk_source(FORWARDING, tests), BULK_DIFF)?;
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    let missing = finding["missing_discriminators"]
        .as_array()
        .ok_or("missing_discriminators")?;
    assert!(
        missing
            .iter()
            .any(|fact| fact["value"].as_str() == Some("qty == 10")),
        "{finding}"
    );
    Ok(())
}

#[test]
fn wrapper_that_drops_the_helper_result_keeps_pairing_missing() -> Result<(), String> {
    let dropping = "    let _ = is_bulk(qty);\n    if qty > 0 { 5 } else { 0 }";
    let finding = predicate_finding(&bulk_source(dropping, BOUNDARY_TESTS), BULK_DIFF)?;
    // Reach and the bound boundary row are both present, so the dropped
    // result is the only reason the pin does not count.
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert!(has_boundary_row(&finding, "qty == 10"), "{finding}");
    assert_ne!(finding["classification"], "exposed", "{finding}");
    assert!(
        discriminate_summary(&finding).contains("same_test_pairing_missing"),
        "{finding}"
    );
    assert_not_forwarded(&finding);
    Ok(())
}

fn has_boundary_row(finding: &Value, value: &str) -> bool {
    finding["activation"]["observed_values"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["value"].as_str() == Some(value)))
}

/// Abstains (never credit, never an actionable gap) and names the stop.
fn assert_not_forwarded(finding: &Value) {
    assert_eq!(
        finding["classification"], "propagation_unknown",
        "{finding}"
    );
    assert!(
        finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("helper_result_not_forwarded"),
        "{finding}"
    );
}

// #6780 review B1: a wrapper that rebinds the forwarded parameter
// (`let qty = qty * 2;`) must not bind the test's `10` to the helper.
#[test]
fn wrapper_that_rebinds_the_parameter_does_not_credit_the_boundary() -> Result<(), String> {
    let rebinding =
        "    let qty = qty * 2;\n    if is_bulk(qty) {\n        5\n    } else {\n        0\n    }";
    let tests =
        "        assert_eq!(order_discount(10), 5);\n        assert_eq!(order_discount(3), 0);";
    let finding = predicate_finding(&bulk_source(rebinding, tests), BULK_DIFF)?;
    assert!(!has_boundary_row(&finding, "qty == 10"), "{finding}");
    assert_not_forwarded(&finding);
    Ok(())
}

// #6780 review B3: a computed branch (`qty / 2`) equals the other branch at
// the boundary, so the wrapper's pin cannot discriminate the helper.
#[test]
fn wrapper_with_a_computed_branch_does_not_pair() -> Result<(), String> {
    let computed_branch = "    if is_bulk(qty) {\n        qty / 2\n    } else {\n        5\n    }";
    let tests =
        "        assert_eq!(order_discount(10), 5);\n        assert_eq!(order_discount(3), 5);";
    let finding = predicate_finding(&bulk_source(computed_branch, tests), BULK_DIFF)?;
    assert!(has_boundary_row(&finding, "qty == 10"), "{finding}");
    assert_not_forwarded(&finding);
    Ok(())
}

const TIER_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -8,3 +8,3 @@\n     match t {\n-        Tier::Gold => 10,\n+        Tier::Gold => 15,\n         _ => 0,\n";

fn tier_source(wrapper_body: &str, tests: &str) -> String {
    format!(
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum Tier {{\n    Standard,\n    Gold,\n}}\n\nfn discount_percent(t: Tier) -> u64 {{\n    match t {{\n        Tier::Gold => 15,\n        _ => 0,\n    }}\n}}\n\npub fn discounted_cents(p: u64, t: Tier) -> u64 {{\n{wrapper_body}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn gold_discount_applies() {{\n{tests}\n    }}\n}}\n"
    )
}

// #6780 review B2: a match arm in a helper whose wrapper drops the helper's
// result must not be credited through the wrapper's exact pin.
#[test]
fn match_arm_behind_a_dropping_wrapper_is_not_credited() -> Result<(), String> {
    let dropping = "    let _ = discount_percent(t);\n    p";
    let tests = "        assert_eq!(discounted_cents(10_000, Tier::Gold), 10_000);";
    let finding = family_finding(&tier_source(dropping, tests), TIER_DIFF, "match_arm")?;
    assert_eq!(finding["probe"]["line"], 9, "{finding}");
    assert_eq!(
        relation_of(&finding, "gold_discount_applies"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_not_forwarded(&finding);
    Ok(())
}

// #6780 review B2: a wrapper that transforms the helper's result before
// returning it is not a forwarding hop either, so the match arm abstains.
#[test]
fn match_arm_behind_a_transforming_wrapper_is_not_credited() -> Result<(), String> {
    let transforming = "    p * (100 - discount_percent(t)) / 100";
    let tests = "        assert_eq!(discounted_cents(10_000, Tier::Gold), 8_500);";
    let finding = family_finding(&tier_source(transforming, tests), TIER_DIFF, "match_arm")?;
    assert_eq!(
        relation_of(&finding, "gold_discount_applies"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_not_forwarded(&finding);
    Ok(())
}

const UNIT_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn unit_cents(qty: u32) -> u32 {\n-    qty * 3\n+    qty * 4\n }\n";

fn unit_source(wrapper_body: &str, tests: &str) -> String {
    format!(
        "fn unit_cents(qty: u32) -> u32 {{\n    qty * 4\n}}\n\npub fn order_cents(qty: u32) -> u32 {{\n{wrapper_body}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn two_units_cost_eight_cents() {{\n{tests}\n    }}\n}}\n"
    )
}

// #6780 review B2: a return-value change in a helper whose wrapper drops
// or transforms the helper's result is not credited through the wrapper pin.
#[test]
fn return_value_behind_a_dropping_or_transforming_wrapper_is_not_credited() -> Result<(), String>
{
    for (wrapper, tests) in [
        (
            "    let _ = unit_cents(qty);\n    qty",
            "        assert_eq!(order_cents(2), 2);",
        ),
        (
            "    unit_cents(qty) + 1",
            "        assert_eq!(order_cents(2), 9);",
        ),
    ] {
        let finding = family_finding(&unit_source(wrapper, tests), UNIT_DIFF, "return_value")?;
        assert_eq!(
            relation_of(&finding, "two_units_cost_eight_cents"),
            Some("helper_owner_call"),
            "{wrapper}: {finding}"
        );
        assert_not_forwarded(&finding);
    }
    Ok(())
}

// Control: the same return-value change behind a forwarding wrapper is not
// stopped at the hop.
#[test]
fn return_value_behind_a_forwarding_wrapper_is_not_stopped_at_the_hop() -> Result<(), String> {
    let finding = family_finding(
        &unit_source("    unit_cents(qty)", "        assert_eq!(order_cents(2), 8);"),
        UNIT_DIFF,
        "return_value",
    )?;
    assert!(
        !finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("helper_result_not_forwarded"),
        "{finding}"
    );
    Ok(())
}

// Control for the dropping wrapper: the same arm behind a wrapper that
// returns the helper's result keeps its ordinary verdict path (not the
// hop stop).
#[test]
fn match_arm_behind_a_forwarding_wrapper_keeps_its_verdict_path() -> Result<(), String> {
    let forwarding = "    let _ = p;\n    discount_percent(t)";
    let tests = "        assert_eq!(discounted_cents(10_000, Tier::Gold), 15);";
    let finding = family_finding(&tier_source(forwarding, tests), TIER_DIFF, "match_arm")?;
    assert!(
        !finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("helper_result_not_forwarded"),
        "{finding}"
    );
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn wrapper_that_transforms_the_argument_transfers_no_boundary_value() -> Result<(), String> {
    let computed = "    if is_bulk(qty + 1) {\n        5\n    } else {\n        0\n    }";
    let finding = predicate_finding(&bulk_source(computed, BOUNDARY_TESTS), BULK_DIFF)?;
    // Reach still holds through the call; the computed hop argument stops
    // the exact-value transfer, so the wrapper's `10` is not a boundary row.
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    assert_ne!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    Ok(())
}

#[test]
fn generic_helper_behind_a_constant_argument_wrapper_pairs() -> Result<(), String> {
    let source = "fn capped<T: PartialOrd + Copy>(value: T, cap: T) -> T {\n    if cap < value {\n        cap\n    } else {\n        value\n    }\n}\n\npub fn capped_score(value: u32) -> u32 {\n    capped(value, 10)\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn scores_above_the_cap_are_capped() {\n        assert_eq!(capped_score(11), 10);\n        assert_eq!(capped_score(10), 10);\n        assert_eq!(capped_score(3), 3);\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn capped<T: PartialOrd + Copy>(value: T, cap: T) -> T {\n-    if value > cap {\n+    if cap < value {\n         cap\n";
    let finding = predicate_finding(source, diff)?;
    assert_eq!(
        relation_of(&finding, "scores_above_the_cap_are_capped"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    // The wrapper's exact pin at `capped_score(10)` pairs with `cap ==
    // value`; propagation through `if .. { cap } else { value }` remains a
    // separate syntax-first limit, so the class is not asserted here.
    assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    assert!(
        !discriminate_summary(&finding).contains("same_test_pairing_missing"),
        "{finding}"
    );
    Ok(())
}
