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

fn returned_ledger_source(outer_body: &str, observed: &str, expected: usize) -> String {
    format!(
        "struct Error {{ detail: String }}\nfn record(errors: &mut Vec<Error>) {{\n    errors.push(Error {{\n        detail: format!(\"diagnostic\"),\n    }});\n}}\nfn collect() -> Vec<Error> {{ let mut errors = Vec::new(); record(&mut errors); errors }}\nfn outer() -> Vec<Error> {{ {outer_body} }}\n#[test]\nfn observes() {{ assert_eq!({observed}().len(), {expected}); }}\n"
    )
}

const RETURNED_LEDGER_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,3 +3,3 @@\n     errors.push(Error {\n-        detail: format!(\"old diagnostic\"),\n+        detail: format!(\"diagnostic\"),\n     });\n";

#[test]
fn returned_ledger_higher_observed_routes_keep_forwarding_limitation() -> Result<(), String> {
    // Neither a dropped result nor a syntactically forwarded higher result is
    // covered by the first-hop ledger proof. Do not invent an upper identity.
    for (outer, expected) in [("let _ = collect(); Vec::new()", 0), ("collect()", 1)] {
        let finding = family_finding(
            &returned_ledger_source(outer, "outer", expected),
            RETURNED_LEDGER_DIFF,
            "field_construction",
        )?;
        assert_eq!(
            finding["classification"], "propagation_unknown",
            "{finding}"
        );
        let summary = finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default();
        assert!(summary.contains("helper_result_not_forwarded"), "{finding}");
        assert!(
            !summary.contains("returned_mutable_ledger_observer_unresolved"),
            "{finding}"
        );
    }
    Ok(())
}

#[test]
fn returned_ledger_direct_observation_keeps_partial_identity() -> Result<(), String> {
    for outer in ["collect()", "let _ = collect(); Vec::new()"] {
        let finding = family_finding(
            &returned_ledger_source(outer, "collect", 1),
            RETURNED_LEDGER_DIFF,
            "field_construction",
        )?;
        assert_eq!(
            finding["classification"], "propagation_unknown",
            "{finding}"
        );
        let summary = finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default();
        assert!(
            summary.contains("returned_mutable_ledger_observer_unresolved"),
            "{finding}"
        );
        assert!(
            !summary.contains("helper_result_not_forwarded"),
            "{finding}"
        );
    }
    Ok(())
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
fn return_value_behind_a_dropping_or_transforming_wrapper_is_not_credited() -> Result<(), String> {
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
        &unit_source(
            "    unit_cents(qty)",
            "        assert_eq!(order_cents(2), 8);",
        ),
        UNIT_DIFF,
        "return_value",
    )?;
    assert_eq!(finding["probe"]["line"], 2, "{finding}");
    assert_eq!(finding["probe"]["expression"], "qty * 4", "{finding}");
    assert_eq!(
        relation_of(&finding, "two_units_cost_eight_cents"),
        Some("helper_owner_call"),
        "{finding}"
    );
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

// #6780 review: a test that calls the forwarding intermediate hop
// (`middle(10)`) observes that hop's result, so an outer wrapper that drops
// `middle`'s result does not make propagation unknown; a test that calls
// only the dropping outer wrapper still abstains. End to end, a test
// calling `middle` also makes `middle` multi-caller, so the chain stops
// there; the hop-bound check itself is discriminated by the
// `chain_forwards_owner_result_checks_every_hop` unit test.
fn layered_source(tests: &str) -> String {
    format!(
        "fn is_bulk(qty: u32) -> bool {{\n    10 <= qty\n}}\n\npub fn middle(qty: u32) -> bool {{\n    is_bulk(qty)\n}}\n\npub fn entry(qty: u32) -> u32 {{\n    let _ = middle(qty);\n    5\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn bulk_threshold() {{\n{tests}\n    }}\n}}\n"
    )
}

#[test]
fn intermediate_hop_test_is_not_stopped_by_a_dropping_outer_wrapper() -> Result<(), String> {
    let finding = predicate_finding(
        &layered_source("        assert!(middle(10));\n        assert!(!middle(9));"),
        BULK_DIFF,
    )?;
    assert_eq!(
        relation_of(&finding, "bulk_threshold"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert!(
        !finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("helper_result_not_forwarded"),
        "{finding}"
    );
    let outer = predicate_finding(
        &layered_source("        assert_eq!(entry(10), 5);"),
        BULK_DIFF,
    )?;
    assert_not_forwarded(&outer);
    Ok(())
}

// #6780 review round 2: activation binds the first scalar buried in a
// compound wrapper argument, so `order_discount(std::cmp::max(10, 50))` or
// `order_discount(10 * 2)` must not pair the `10 <= qty` boundary. Where the
// refusal lands differs after the #6796 unresolved-boundary authority:
// `max(10, 50)` keeps infection yes and refuses at pairing, while the
// computed `10 * 2` argument leaves the boundary unresolved, so the finding
// abstains as `infection_unknown` before pairing is consulted.
#[test]
fn buried_scalar_in_a_wrapper_argument_does_not_pair() -> Result<(), String> {
    for (pin, refuses_at_infection) in [
        (
            "        assert_eq!(order_discount(std::cmp::max(10, 50)), 5);",
            false,
        ),
        ("        assert_eq!(order_discount(10 * 2), 5);", true),
    ] {
        let tests = format!("{pin}\n        assert_eq!(order_discount(3), 0);");
        let finding = predicate_finding(&bulk_source(FORWARDING, &tests), BULK_DIFF)?;
        assert_eq!(
            relation_of(&finding, "ten_items_earn_the_bulk_discount"),
            Some("helper_owner_call"),
            "{pin}: {finding}"
        );
        assert_ne!(finding["classification"], "exposed", "{pin}: {finding}");
        if refuses_at_infection {
            assert_eq!(
                finding["classification"], "infection_unknown",
                "{pin}: {finding}"
            );
            assert!(
                finding["ripr"]["infect"]["summary"]
                    .as_str()
                    .is_some_and(|summary| summary.contains("computed argument for `qty`")),
                "{pin}: {finding}"
            );
        } else {
            assert!(
                discriminate_summary(&finding).contains("same_test_pairing_missing"),
                "{pin}: {finding}"
            );
        }
    }
    Ok(())
}

// #6780 review round 2: a direct owner call with a weak oracle plus a
// non-forwarding wrapper with an exact oracle. The direct call lifts the
// helper-only abstention, so the wrapper's exact oracle must still not be
// credited for the owner's return-value change. End to end the direct test
// is itself a caller of the helper, so the chain stops at the multi-caller
// edge and the wrapper test keeps its file-proximity relation.
#[test]
fn mixed_direct_weak_and_wrapper_exact_oracles_do_not_credit_the_wrapper() -> Result<(), String> {
    let source = "fn unit_cents(qty: u32) -> u32 {\n    qty * 4\n}\n\npub fn order_cents(qty: u32) -> u32 {\n    unit_cents(qty) + 1\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn unit_cents_is_positive() {\n        assert!(unit_cents(2) > 0);\n    }\n\n    #[test]\n    fn two_units_cost_nine_cents() {\n        assert_eq!(order_cents(2), 9);\n    }\n}\n";
    let finding = family_finding(source, UNIT_DIFF, "return_value")?;
    assert_eq!(finding["probe"]["expression"], "qty * 4", "{finding}");
    assert_eq!(
        relation_of(&finding, "unit_cents_is_positive"),
        Some("direct_owner_call"),
        "{finding}"
    );
    assert_eq!(
        relation_of(&finding, "two_units_cost_nine_cents"),
        Some("same_test_file"),
        "{finding}"
    );
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    assert_ne!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    Ok(())
}

// #6780 Devin review: a test-local binding that shadows the wrapper name
// never reaches the helper, so its call must not relate through the chain
// or pair the wrapper's boundary.
#[test]
fn shadowed_wrapper_call_does_not_relate_or_pair() -> Result<(), String> {
    for shadow in [
        "        let order_discount = |_: u32| 5;",
        "        fn order_discount(_: u32) -> u32 { 5 }",
    ] {
        let tests = format!("{shadow}\n        assert_eq!(order_discount(10), 5);");
        let finding = predicate_finding(&bulk_source(FORWARDING, &tests), BULK_DIFF)?;
        assert_ne!(
            relation_of(&finding, "ten_items_earn_the_bulk_discount"),
            Some("helper_owner_call"),
            "{shadow}: {finding}"
        );
        assert_ne!(finding["classification"], "exposed", "{shadow}: {finding}");
        assert!(
            !has_boundary_row(&finding, "qty == 10"),
            "{shadow}: {finding}"
        );
    }
    // Control: a shadow binding after the call, or under another name,
    // leaves the wrapper call relating and pairing.
    for unshadowed in [
        "        assert_eq!(order_discount(10), 5);
        let order_discount = |_: u32| 5;
        assert_eq!(order_discount(9), 5);",
        "        let discount = |_: u32| 5;
        assert_eq!(order_discount(10), 5);
        assert_eq!(discount(9), 5);",
    ] {
        let finding = predicate_finding(&bulk_source(FORWARDING, unshadowed), BULK_DIFF)?;
        assert_eq!(
            relation_of(&finding, "ten_items_earn_the_bulk_discount"),
            Some("helper_owner_call"),
            "{unshadowed}: {finding}"
        );
        assert!(
            has_boundary_row(&finding, "qty == 10"),
            "{unshadowed}: {finding}"
        );
    }
    Ok(())
}

const RECORD_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn record(out: &mut Vec<u32>) {\n-    out.push(9);\n+    out.push(10);\n }\n";

// #6780 Devin review: a helper's side effect on caller-owned state is
// observable without forwarding the helper's (unit) result, so a wrapper
// that discards it must not make the effect's propagation unknown.
#[test]
fn helper_side_effect_behind_a_discarding_wrapper_keeps_its_propagation() -> Result<(), String> {
    let source = "fn record(out: &mut Vec<u32>) {\n    out.push(10);\n}\n\npub fn wrapper(out: &mut Vec<u32>) {\n    record(out);\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn wrapper_records_ten() {\n        let mut out = Vec::new();\n        wrapper(&mut out);\n        assert_eq!(out, vec![10]);\n    }\n}\n";
    let report = all_findings(source, RECORD_DIFF)?;
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    let mut families = Vec::new();
    for finding in findings {
        assert_eq!(finding["probe"]["expression"], "out.push(10)", "{finding}");
        assert_eq!(
            relation_of(finding, "wrapper_records_ten"),
            Some("helper_owner_call"),
            "{finding}"
        );
        assert!(
            !finding["ripr"]["propagate"]["summary"]
                .as_str()
                .unwrap_or_default()
                .contains("helper_result_not_forwarded"),
            "{finding}"
        );
        assert_ne!(
            finding["classification"], "propagation_unknown",
            "{finding}"
        );
        families.push(
            finding["probe"]["family"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
    }
    families.sort();
    assert_eq!(families, ["call_deletion", "side_effect"], "{report}");
    Ok(())
}

fn all_findings(source: &str, diff: &str) -> Result<Value, String> {
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
    serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())
}

// #6780 round 5: a helper effect on a target the caller's test can never
// see (a fresh temporary, or a wrapper-local) must not keep the effect's
// propagation through a wrapper that drops it: it abstains.
#[test]
fn helper_effect_on_a_wrapper_owned_target_abstains() -> Result<(), String> {
    for wrapper in [
        "pub fn wrapper() -> u32 {\n    record(&mut Vec::new());\n    7\n}",
        "pub fn wrapper() -> u32 {\n    let mut v = Vec::new();\n    record(&mut v);\n    7\n}",
    ] {
        let source = format!(
            "fn record(out: &mut Vec<u32>) {{\n    out.push(10);\n}}\n\n{wrapper}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn wrapper_returns_seven() {{\n        let out = vec![10];\n        assert_eq!(wrapper(), 7);\n        assert_eq!(out, vec![10]);\n    }}\n}}\n"
        );
        let report = all_findings(&source, RECORD_DIFF)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert!(!findings.is_empty(), "{wrapper}: {report}");
        for finding in findings {
            assert_eq!(finding["probe"]["expression"], "out.push(10)", "{finding}");
            assert_eq!(
                relation_of(finding, "wrapper_returns_seven"),
                Some("helper_owner_call"),
                "{wrapper}: {finding}"
            );
            assert_not_forwarded(finding);
        }
    }
    Ok(())
}

/// The bulk crate with extra items in `mod tests` before the test.
fn bulk_with_test_items(items: &str, tests: &str) -> String {
    format!(
        "fn is_bulk(qty: u32) -> bool {{\n    10 <= qty\n}}\n\npub fn order_discount(qty: u32) -> u32 {{\n{FORWARDING}\n}}\n\npub mod other {{\n    pub fn fake_discount(q: u32) -> u32 {{\n        if q > 10 {{ 5 }} else {{ 0 }}\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{items}\n    #[test]\n    fn ten_items_earn_the_bulk_discount() {{\n{tests}\n    }}\n}}\n"
    )
}

// #6780 round 5: an import alias or a same-named fn item in the test module
// rebinds the wrapper's name, so the call earns neither the chain relation
// nor the wrapper boundary pairing.
#[test]
fn aliased_or_same_named_wrapper_does_not_relate_or_pair() -> Result<(), String> {
    for (items, tests) in [
        (
            "",
            format!("        use crate::other::fake_discount as order_discount;\n{BOUNDARY_TESTS}"),
        ),
        (
            "    use crate::other::fake_discount as order_discount;\n",
            BOUNDARY_TESTS.to_string(),
        ),
        (
            "    fn order_discount(q: u32) -> u32 {\n        if q > 10 { 5 } else { 0 }\n    }\n",
            BOUNDARY_TESTS.to_string(),
        ),
    ] {
        let finding = predicate_finding(&bulk_with_test_items(items, &tests), BULK_DIFF)?;
        assert_ne!(
            relation_of(&finding, "ten_items_earn_the_bulk_discount"),
            Some("helper_owner_call"),
            "{items}{tests}: {finding}"
        );
        assert_ne!(
            finding["classification"], "exposed",
            "{items}{tests}: {finding}"
        );
        assert!(
            !has_boundary_row(&finding, "qty == 10"),
            "{items}{tests}: {finding}"
        );
    }
    // Control: the same crate with no alias still relates and pairs.
    let finding = predicate_finding(&bulk_with_test_items("", BOUNDARY_TESTS), BULK_DIFF)?;
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

/// A wrapper that hands the changed owner to a local `macro_rules!` macro
/// as an identifier token (#6614). The changed `base_rate` body is always
/// on line 8.
fn macro_rate_source(macro_rule: &str, wrapper_body: &str, tests: &str) -> String {
    format!(
        "macro_rules! twice {{\n    {macro_rule}\n}}\n\npub fn other_rate() -> u32 {{ 5 }}\n\npub fn base_rate() -> u32 {{\n    3 + 4\n}}\n\npub fn doubled_rate() -> u32 {{\n    {wrapper_body}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn doubled_rate_is_pinned() {{\n        {tests}\n    }}\n}}\n"
    )
}

const IDENT_RULE: &str = "($f:ident) => { $f() * 2 };";
const MACRO_RATE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -7,3 +7,3 @@\n pub fn base_rate() -> u32 {\n-    7\n+    3 + 4\n }\n";

fn macro_rate_finding(rule: &str, wrapper: &str, tests: &str) -> Result<Value, String> {
    let finding = family_finding(
        &macro_rate_source(rule, wrapper, tests),
        MACRO_RATE_DIFF,
        "return_value",
    )?;
    assert_eq!(finding["probe"]["line"], 8, "{finding}");
    Ok(finding)
}

#[test]
fn exact_pin_through_a_local_macro_argument_abstains_as_not_forwarded() -> Result<(), String> {
    for wrapper in [
        "twice!(base_rate)",
        "twice![base_rate]",
        "twice! { base_rate }",
    ] {
        let finding = macro_rate_finding(IDENT_RULE, wrapper, "assert_eq!(doubled_rate(), 14);")?;
        assert_eq!(
            finding["classification"], "propagation_unknown",
            "{wrapper}: {finding}"
        );
        assert_eq!(
            relation_of(&finding, "doubled_rate_is_pinned"),
            Some("helper_owner_call"),
            "{wrapper}: {finding}"
        );
        assert_not_forwarded(&finding);
    }
    Ok(())
}

#[test]
fn any_exact_oracle_on_a_macro_argument_caller_abstains() -> Result<(), String> {
    // ripr does not expand `twice!`, so an exact oracle in any operand shape
    // can neither credit nor fault the owner.
    for tests in [
        "let rate = doubled_rate();\n        assert_eq!(rate, 14);",
        "assert_eq!(doubled_rate().to_string(), \"14\");",
    ] {
        let finding = macro_rate_finding(IDENT_RULE, "twice!(base_rate)", tests)?;
        assert_eq!(
            finding["classification"], "propagation_unknown",
            "{tests}: {finding}"
        );
        assert_not_forwarded(&finding);
    }
    Ok(())
}

#[test]
fn macro_argument_without_the_owner_or_an_exact_pin_keeps_the_gap() -> Result<(), String> {
    // The macro names a different function: no edge reaches the owner.
    let other = macro_rate_finding(
        IDENT_RULE,
        "twice!(other_rate)",
        "assert_eq!(doubled_rate(), 10);",
    )?;
    // The owner name is only string-literal content of the macro argument.
    let literal = macro_rate_finding(
        "($f:literal) => { $f.len() as u32 * 2 };",
        "twice!(\"base_rate\")",
        "assert_eq!(doubled_rate(), 18);",
    )?;
    // The edge exists, but the oracle is weak on its own terms.
    let compared = macro_rate_finding(
        IDENT_RULE,
        "twice!(base_rate)",
        "assert!(doubled_rate() > 0);",
    )?;
    for finding in [&other, &literal, &compared] {
        assert_ne!(
            finding["classification"], "propagation_unknown",
            "{finding}"
        );
        let propagate = finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default();
        assert!(
            !propagate.contains("helper_result_not_forwarded"),
            "{finding}"
        );
    }
    assert_eq!(compared["classification"], "weakly_exposed", "{compared}");
    assert_eq!(literal["classification"], "weakly_exposed", "{literal}");
    Ok(())
}

#[test]
fn owner_named_only_in_a_wrapper_string_literal_does_not_abstain() -> Result<(), String> {
    // `banner` names `title` only inside its format string; the test calls
    // `title()` itself and pins `banner()`. No macro edge, no abstention.
    let source = "fn title() -> &'static str {\n    concat!(\"ri\", \"pr\")\n}\n\npub fn banner() -> String {\n    format!(\"title\")\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn banner_reads_title() {\n        let _ = title();\n        assert_eq!(banner(), \"title\");\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn title() -> &'static str {\n-    \"ripr\"\n+    concat!(\"ri\", \"pr\")\n }\n";
    let finding = family_finding(source, diff, "return_value")?;
    assert_eq!(finding["probe"]["line"], 2, "{finding}");
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    assert_ne!(
        relation_of(&finding, "banner_reads_title"),
        Some("helper_owner_call"),
        "{finding}"
    );
    Ok(())
}

const TITLE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn title() -> &'static str {\n-    \"ripr\"\n+    concat!(\"ri\", \"pr\")\n }\n";

fn banner_finding(wrapper: &str, test: &str) -> Result<Value, String> {
    let source = format!(
        "fn title() -> &'static str {{\n    concat!(\"ri\", \"pr\")\n}}\n\npub fn banner() -> String {{\n    {wrapper}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn banner_check() {{\n        {test}\n    }}\n}}\n"
    );
    let finding = family_finding(&source, TITLE_DIFF, "return_value")?;
    assert_eq!(
        relation_of(&finding, "banner_check"),
        Some("helper_owner_call"),
        "{wrapper} | {test}: {finding}"
    );
    Ok(finding)
}

#[test]
fn weak_oracle_behind_a_non_forwarding_hop_keeps_its_gap() -> Result<(), String> {
    // #7549: the oracle cannot discriminate the caller's result exactly, so
    // it cannot discriminate the owner's either; no pairing is needed.
    for (wrapper, test) in [
        ("format!(\"[{}]\", title())", "assert!(banner().len() > 0);"),
        (
            "format!(\"[{}]\", title())",
            "assert!(!banner().is_empty());",
        ),
        (
            "title().to_string() + \"!\"",
            "assert!(!banner().is_empty());",
        ),
    ] {
        let finding = banner_finding(wrapper, test)?;
        assert_eq!(
            finding["classification"], "weakly_exposed",
            "{wrapper} | {test}: {finding}"
        );
        let propagate = finding["ripr"]["propagate"]["summary"]
            .as_str()
            .unwrap_or_default();
        assert!(
            !propagate.contains("helper_result_not_forwarded"),
            "{wrapper} | {test}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn exact_or_mixed_oracle_behind_a_non_forwarding_hop_abstains() -> Result<(), String> {
    // An exact oracle on the caller (even on a projection such as `.len()`)
    // might pin the owner through the transform: neither credit nor gap.
    for (wrapper, test) in [
        (
            "format!(\"[{}]\", title())",
            "assert_eq!(banner().len(), 6);",
        ),
        (
            "title().to_string() + \"!\"",
            "assert_eq!(banner(), \"ripr!\");",
        ),
        (
            "title().to_string() + \"!\"",
            "assert!(!banner().is_empty());\n        assert_eq!(banner(), \"ripr!\");",
        ),
        // A bare call of a caller pins its whole result.
        (
            "title().to_string() + \"!\"",
            "assert!(banner().len() == 5);",
        ),
        (
            "title().to_string() + \"!\"",
            "assert!(!banner().is_empty() != true);",
        ),
        (
            "title().to_string() + \"!\"",
            "assert!(banner().contains(\"pr\"));",
        ),
    ] {
        let finding = banner_finding(wrapper, test)?;
        assert_eq!(
            finding["classification"], "propagation_unknown",
            "{wrapper} | {test}: {finding}"
        );
        assert_not_forwarded(&finding);
    }
    Ok(())
}

fn title_item_finding(items: &str, test: &str) -> Result<Value, String> {
    let source = format!(
        "fn title() -> &'static str {{\n    concat!(\"ri\", \"pr\")\n}}\n\n{items}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn banner_check() {{\n        {test}\n    }}\n}}\n"
    );
    family_finding(&source, TITLE_DIFF, "return_value")
}

fn assert_abstains_through_hop(finding: &Value, label: &str) {
    assert_eq!(
        relation_of(finding, "banner_check"),
        Some("helper_owner_call"),
        "{label}: {finding}"
    );
    assert_eq!(
        finding["classification"], "propagation_unknown",
        "{label}: {finding}"
    );
    assert_not_forwarded(finding);
}

#[test]
fn bool_caller_pinned_by_a_plain_assert_abstains() -> Result<(), String> {
    // `assert!(caller(..))` is `assert_eq!(caller(..), true)`, a cast keeps
    // that pin, and a `bool` caller decides rather than carries data.
    let titled = "pub fn titled() -> bool {\n    title() == \"ripr\"\n}";
    for test in [
        "assert!(titled());",
        "assert!(!titled());",
        "assert!(titled() as u8 > 0);",
    ] {
        assert_abstains_through_hop(&title_item_finding(titled, test)?, test);
    }
    Ok(())
}

#[test]
fn caller_that_branches_on_the_owner_abstains_under_a_loose_check() -> Result<(), String> {
    // #7549 review B1: the caller turns the owner's exact value into the
    // very shape the loose check reads, so the check pins the owner.
    for (item, test) in [
        (
            "pub fn parse() -> Result<(), ()> {\n    if title() == \"ripr\" { Ok(()) } else { Err(()) }\n}",
            "assert!(parse().is_ok());",
        ),
        (
            "pub fn gated(len: usize) -> usize {\n    if title() == \"ripr\" { len } else { 0 }\n}",
            "assert!(gated(4) > 0);",
        ),
        (
            "pub fn role() -> Option<u8> {\n    (title() == \"ripr\").then_some(1)\n}",
            "assert!(role().is_some());",
        ),
        (
            "pub fn role() -> Option<u8> {\n    (title() == \"ripr\").then_some(1)\n}",
            "assert!(role() != None);",
        ),
        (
            "pub fn kind() -> u8 {\n    match title() { \"ripr\" => 1, _ => 0 }\n}",
            "assert!(kind() > 0);",
        ),
        (
            "pub fn banner() -> String {\n    format!(\"[{}]\", title())\n}",
            "assert!(banner().parse::<u8>().is_err());\n        let _ = banner().chars().next().unwrap();",
        ),
        // Review B2: the decision moves into something the caller calls.
        (
            "fn check(t: &str) -> Result<(), ()> {\n    if t == \"ripr\" { Ok(()) } else { Err(()) }\n}\n\npub fn parse() -> Result<(), ()> {\n    check(title())\n}",
            "assert!(parse().is_ok());",
        ),
        (
            "pub fn port() -> Result<u16, std::num::ParseIntError> {\n    title().parse()\n}",
            "assert!(port().is_err());",
        ),
        (
            "const TABLE: [u8; 2] = [1, 2];\n\npub fn entry() -> Option<&'static u8> {\n    TABLE.get(title().len())\n}",
            "assert!(entry().is_none());",
        ),
        (
            "pub fn share() -> usize {\n    100 / title().len()\n}",
            "assert!(share() > 0);",
        ),
        (
            "pub fn banner() -> String {\n    format!(\"{}\", title().len() > 3)\n}",
            "assert!(!banner().is_empty());",
        ),
        (
            "pub fn mode() -> String {\n    title().into()\n}",
            "assert!(!mode().is_empty());",
        ),
        // Devin review: a discarded call, and an exact decision under a
        // loose projection or comparison.
        (
            "pub fn banner() -> String {\n    let _ignored = title();\n    String::from(\"fixed\")\n}",
            "assert!(!banner().is_empty());",
        ),
        (
            "pub fn banner() -> String {\n    title().to_string() + \"!\"\n}",
            "assert!(banner().strip_prefix(\"ripr!\").is_some());",
        ),
        (
            "pub fn banner() -> String {\n    title().to_string() + \"!\"\n}",
            "assert!(banner().find(\"ripr\").unwrap_or(9) < 1);",
        ),
        (
            "pub fn banner() -> String {\n    title().to_string() + \"!\"\n}",
            "let shown = banner();\n        assert!(shown.len() > 0);",
        ),
    ] {
        assert_abstains_through_hop(&title_item_finding(item, test)?, test);
    }
    Ok(())
}

#[test]
fn test_local_helper_hop_is_part_of_the_oracle() -> Result<(), String> {
    let source = "fn title() -> &'static str {\n    concat!(\"ri\", \"pr\")\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    fn titled(suffix: &str) -> String {\n        format!(\"{}{}\", title(), suffix)\n    }\n\n    #[test]\n    fn banner_check() {\n        assert!(titled(\"!\").len() > 0);\n    }\n}\n";
    let finding = family_finding(source, TITLE_DIFF, "return_value")?;
    assert_abstains_through_hop(&finding, "test-local helper");
    // The same data-carrying caller in production keeps the weak gap.
    let production = title_item_finding(
        "pub fn titled(suffix: &str) -> String {\n    format!(\"{}{}\", title(), suffix)\n}",
        "assert!(titled(\"!\").len() > 0);",
    )?;
    assert_eq!(
        production["classification"], "weakly_exposed",
        "{production}"
    );
    Ok(())
}

#[test]
fn weak_oracle_does_not_lift_the_effect_target_rule() -> Result<(), String> {
    // Effect families keep their target-passing rule: a fresh temporary is
    // invisible to the test whatever its oracle.
    let source = "fn record(out: &mut Vec<u32>) {\n    out.push(10);\n}\n\npub fn wrapper() -> u32 {\n    record(&mut Vec::new());\n    7\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn wrapper_returns_seven() {\n        assert!(wrapper() > 0);\n    }\n}\n";
    let report = all_findings(source, RECORD_DIFF)?;
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    assert!(!findings.is_empty(), "{report}");
    for finding in findings {
        assert_eq!(finding["probe"]["expression"], "out.push(10)", "{finding}");
        assert_eq!(
            relation_of(finding, "wrapper_returns_seven"),
            Some("helper_owner_call"),
            "{finding}"
        );
        assert_not_forwarded(finding);
    }
    Ok(())
}

/// An effect owner behind a local-macro hop, called by `entry`. `wrapper`
/// is either the macro caller itself or a plain fn hop above it.
fn macro_record_source(entry: &str, test: &str) -> String {
    format!(
        "fn record(out: &mut Vec<u32>) {{\n    out.push(10);\n}}\n\nmacro_rules! fresh {{\n    ($f:ident) => {{{{ $f(&mut Vec::new()); 7 }}}};\n}}\n\npub fn inner() -> u32 {{\n    fresh!(record)\n}}\n\n{entry}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn wrapper_returns_seven() {{\n        {test}\n    }}\n}}\n"
    )
}

#[test]
fn effect_macro_exception_needs_every_hop_through_the_macro() -> Result<(), String> {
    // #7622 review: the local-macro exception covers the macro hop only. A
    // plain fn hop above it keeps the effect target-passing rule.
    let direct = all_findings(
        &macro_record_source("", "assert!(inner() > 0);"),
        RECORD_DIFF,
    )?;
    let direct = direct["findings"].as_array().ok_or("missing findings")?;
    assert!(!direct.is_empty());
    for finding in direct {
        assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    }
    let mixed = all_findings(
        &macro_record_source(
            "pub fn outer() -> u32 {\n    inner()\n}",
            "assert!(outer() > 0);",
        ),
        RECORD_DIFF,
    )?;
    let mixed = mixed["findings"].as_array().ok_or("missing findings")?;
    assert!(!mixed.is_empty());
    for finding in mixed {
        assert_eq!(
            relation_of(finding, "wrapper_returns_seven"),
            Some("helper_owner_call"),
            "{finding}"
        );
        assert_not_forwarded(finding);
    }
    Ok(())
}

#[test]
fn user_type_between_owner_and_caller_abstains() -> Result<(), String> {
    // #7549 review B3: `format!`, `to_string` and `clone` run the owner's
    // type's own impls, which may decide on the value.
    let source = "#[derive(Clone, PartialEq, Debug)]\npub struct Mode(u8);\n\nimpl std::fmt::Display for Mode {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        if self.0 == 1 { write!(f, \"fast\") } else { write!(f, \"\") }\n    }\n}\n\nfn mode() -> Mode {\n    Mode(2 - 1)\n}\n\npub fn banner() -> String {\n    format!(\"{}\", mode())\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn banner_check() {\n        assert!(!banner().is_empty());\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,3 +10,3 @@ impl std::fmt::Display for Mode {\n fn mode() -> Mode {\n-    Mode(1)\n+    Mode(2 - 1)\n }\n";
    let finding = family_finding(source, diff, "return_value")?;
    assert_abstains_through_hop(&finding, "user Display");
    Ok(())
}

// #6926: a helper called inside an `Arc<Mutex<T>>::lock().await` closure is
// not a forwarding tail, so the wrapper's pin must not credit the owner.
#[test]
fn helper_inside_async_lock_closure_abstains() -> Result<(), String> {
    let wrapper = "    let locked = std::sync::Arc::new(std::sync::Mutex::new(qty));\n    let guard = locked.lock().await;\n    (|| {\n        if is_bulk(*guard) {\n            5\n        } else {\n            0\n        }\n    })()";
    let finding = predicate_finding(&bulk_source(wrapper, BOUNDARY_TESTS), BULK_DIFF)?;
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_not_forwarded(&finding);
    Ok(())
}

// #6926: OnceCell / OnceLock / lazy_static cache the helper, so a later
// wrapper pin does not observe a forwarded hop result.
#[test]
fn helper_cached_behind_once_cell_once_lock_or_lazy_static_abstains() -> Result<(), String> {
    for wrapper in [
        "    let cache = std::cell::OnceCell::new();\n    *cache.get_or_init(|| if is_bulk(qty) { 5 } else { 0 })",
        "    static CACHE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();\n    *CACHE.get_or_init(|| if is_bulk(qty) { 5 } else { 0 })",
        "    lazy_static::lazy_static! {\n        static ref CACHE: u32 = if is_bulk(qty) { 5 } else { 0 };\n    }\n    *CACHE",
    ] {
        let finding = predicate_finding(&bulk_source(wrapper, BOUNDARY_TESTS), BULK_DIFF)?;
        assert_eq!(
            relation_of(&finding, "ten_items_earn_the_bulk_discount"),
            Some("helper_owner_call"),
            "{wrapper}: {finding}"
        );
        assert_not_forwarded(&finding);
    }
    Ok(())
}

// #6926: `Option<&T>` wrapping or slicing the helper's own result, not an
// index into a separate table (`TABLE.get(title().len())` above).
#[test]
fn option_ref_derived_from_helper_result_abstains() -> Result<(), String> {
    for (item, test) in [
        (
            "pub fn entry() -> Option<&'static str> {\n    Some(title())\n}",
            "assert_eq!(entry(), Some(\"ripr\"));",
        ),
        (
            "pub fn entry() -> Option<&'static str> {\n    title().get(..)\n}",
            "assert_eq!(entry(), Some(\"ripr\"));",
        ),
    ] {
        assert_abstains_through_hop(&title_item_finding(item, test)?, test);
    }
    Ok(())
}

// #6926: a same-named `fn` in the test module that macro expansion would
// define is absent from the workspace index, so uniqueness cannot see it.
// `test_call_is_shadowed`'s fail-closed path (lexical / macro) must still
// refuse the helper-owner relation.
#[test]
fn unindexed_same_named_test_module_fn_is_caught_by_shadow_fallback() -> Result<(), String> {
    let items = "    macro_rules! local_discount {\n        () => {\n            fn order_discount(_: u32) -> u32 { 5 }\n        };\n    }\n    local_discount!();\n";
    let finding = predicate_finding(&bulk_with_test_items(items, BOUNDARY_TESTS), BULK_DIFF)?;
    assert_ne!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    assert!(!has_boundary_row(&finding, "qty == 10"), "{finding}");
    Ok(())
}
