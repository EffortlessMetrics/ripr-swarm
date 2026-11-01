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

// #9849: the private bool's whole return is the same observed boundary as
// its predicate twin; the wrapper's distinct literal branches preserve it.
#[test]
fn boolean_return_twin_is_pinned_by_a_forwarding_wrapper() -> Result<(), String> {
    for tests in [
        BOUNDARY_TESTS,
        "        assert_eq!(5, order_discount(10));\n        assert_eq!(0, order_discount(9));",
    ] {
        let finding = family_finding(&bulk_source(FORWARDING, tests), BULK_DIFF, "return_value")?;
        assert_eq!(finding["probe"]["expression"], "10 <= qty", "{finding}");
        assert_eq!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
        assert_eq!(finding["classification"], "exposed", "{finding}");
    }
    Ok(())
}

#[test]
fn boolean_return_twin_keeps_unobserved_wrapper_results_unconfirmed() -> Result<(), String> {
    for (wrapper, tests) in [
        (FORWARDING, "        assert_eq!(order_discount(10) * 0, 0);"),
        (FORWARDING, "        assert_eq!(order_discount(10) % 5, 0);"),
        (
            FORWARDING,
            "        let expected = order_discount(10);\n        assert_eq!(order_discount(10), expected);",
        ),
        (FORWARDING, "        assert_ne!(order_discount(10), 99);"),
        (
            FORWARDING,
            "        assert_eq!(order_discount(12), 5);\n        assert_eq!(order_discount(3), 0);",
        ),
        (
            FORWARDING,
            "        let _ = order_discount(10);\n        assert_eq!(order_discount(12), 5);",
        ),
        (
            FORWARDING,
            "        if false { assert_eq!(order_discount(10), 5); }",
        ),
        (
            FORWARDING,
            "        let _ = order_discount(10);\n        let order_discount = |_: u32| 5;\n        assert_eq!(order_discount(10), 5);",
        ),
        (
            "    let _ = is_bulk(qty);\n    5",
            "        assert_eq!(order_discount(10), 5);",
        ),
        (
            "    let _ = is_bulk(qty);\n    if qty >= 10 { 5 } else { 0 }",
            BOUNDARY_TESTS,
        ),
        // #10284: a wrapper-local binding shadows the helper, and a macro
        // statement may return before the forwarded call.
        (
            "    let is_bulk = |_q: u32| true;\n    if is_bulk(qty) { 5 } else { 0 }",
            BOUNDARY_TESTS,
        ),
        (
            "    early!(qty);\n    if is_bulk(qty) { 5 } else { 0 }",
            BOUNDARY_TESTS,
        ),
        (
            "    if is_bulk(qty) { qty / 2 } else { 5 }",
            "        assert_eq!(order_discount(10), 5);\n        assert_eq!(order_discount(9), 5);",
        ),
    ] {
        let finding = family_finding(&bulk_source(wrapper, tests), BULK_DIFF, "return_value")?;
        // The refusal must come from the guard, not from a lost relation.
        assert_eq!(
            relation_of(&finding, "ten_items_earn_the_bulk_discount"),
            Some("helper_owner_call"),
            "{wrapper}\n{tests}\n{finding}"
        );
        assert_ne!(
            finding["classification"], "exposed",
            "{wrapper}\n{tests}\n{finding}"
        );
        assert_ne!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    }
    Ok(())
}

// #10284: neither twin borrows the wrapper's pins when the wrapper binds
// the helper's name or runs a macro that may return first. Only a bare
// standard assertion/format macro with no rebinding stays transparent;
// logging and path-qualified macros withhold credit from both twins.
const EPRINTLN: &str = "macro_rules! éprintln { ($q:expr) => { if $q == 10 { return 5; } }; }\n";
const DEP_PRINTLN: &str = "use crate as dep;\n#[macro_export]\nmacro_rules! println { ($q:expr) => { if $q == 10 { return 5; } }; }";

#[test]
fn shadowed_or_macro_guarded_wrappers_confirm_neither_twin() -> Result<(), String> {
    for wrapper in [
        "    let is_bulk = |_q: u32| true;\n    if is_bulk(qty) { 5 } else { 0 }",
        "    bail_at_ten!(qty);\n    if is_bulk(qty) { 5 } else { 0 }",
        // #10351 review: U+200E is Rust whitespace, so the bang still invokes.
        "    bail_at_ten!\u{200E}(qty);\n    if is_bulk(qty) { 5 } else { 0 }",
    ] {
        let source = bulk_source(wrapper, BOUNDARY_TESTS);
        for finding in [
            predicate_finding(&source, BULK_DIFF)?,
            family_finding(&source, BULK_DIFF, "return_value")?,
        ] {
            assert_ne!(finding["classification"], "exposed", "{wrapper}\n{finding}");
        }
    }
    // #10351 review: a crate-local macro may share a logging name and return
    // before the helper runs, so `qty > 10` would still pass both pins.
    for name in ["warn", "error", "debug", "info"] {
        let source = format!(
            "macro_rules! {name} {{ ($q:expr) => {{ if $q == 10 {{ return 5; }} }}; }}\n\n{}",
            bulk_source(
                &format!("    {name}!(qty);\n    if is_bulk(qty) {{ 5 }} else {{ 0 }}"),
                BOUNDARY_TESTS,
            )
        );
        let diff = BULK_DIFF.replace("@@ -1,3 +1,3 @@", "@@ -3,3 +3,3 @@");
        for finding in [
            predicate_finding(&source, &diff)?,
            family_finding(&source, &diff, "return_value")?,
        ] {
            assert_eq!(finding["probe"]["line"], 4, "{name}\n{finding}");
            assert_ne!(finding["classification"], "exposed", "{name}\n{finding}");
        }
    }
    // #10351 review: a crate alias routes a qualified logging path to an
    // exported user macro, and a local `println!` shadows the standard one.
    for (prelude, call) in [
        (
            "use crate as tracing;\n#[macro_export]\nmacro_rules! info { ($q:expr) => { if $q == 10 { return 5; } }; }",
            "tracing::info!(qty);",
        ),
        (
            "macro_rules! println { ($q:expr) => { if $q == 10 { return 5; } }; }\n",
            "println!(qty);",
        ),
        // Equivalent spellings of one qualified path to an exported macro.
        (DEP_PRINTLN, "dep::println!(qty);"),
        (DEP_PRINTLN, "dep :: println!(qty);"),
        (DEP_PRINTLN, "dep /* routed */ :: r#println ! (qty);"),
        // A distinct user macro whose name only ends in `println`; it binds
        // no standard name, so only the tokenizer can refuse it.
        (EPRINTLN, "éprintln!(qty);"),
        (EPRINTLN, "r#éprintln!(qty);"),
    ] {
        let source = format!(
            "{prelude}\n{}",
            bulk_source(
                &format!("    {call}\n    if is_bulk(qty) {{ 5 }} else {{ 0 }}"),
                BOUNDARY_TESTS,
            )
        );
        let first = source
            .lines()
            .position(|line| line.contains("10 <= qty"))
            .ok_or("probe")?;
        let diff = BULK_DIFF.replace("@@ -1,3 +1,3 @@", &format!("@@ -{0},3 +{0},3 @@", first));
        for finding in [
            predicate_finding(&source, &diff)?,
            family_finding(&source, &diff, "return_value")?,
        ] {
            assert_eq!(finding["probe"]["line"], first + 1, "{call}\n{finding}");
            assert_ne!(finding["classification"], "exposed", "{call}\n{finding}");
        }
    }
    // Positive controls: the standard `println!`, unshadowed and in its raw
    // spelling, stays transparent.
    for call in ["println!(\"{qty}\");", "r#println!(\"{qty}\");"] {
        let logged = bulk_source(
            &format!("    {call}\n    if is_bulk(qty) {{ 5 }} else {{ 0 }}"),
            BOUNDARY_TESTS,
        );
        assert_eq!(
            predicate_finding(&logged, BULK_DIFF)?["classification"],
            "exposed",
            "{call}"
        );
        assert_eq!(
            family_finding(&logged, BULK_DIFF, "return_value")?["classification"],
            "exposed",
            "{call}"
        );
    }
    Ok(())
}

#[test]
fn boolean_return_twin_does_not_borrow_other_calls_or_short_circuit_gates() -> Result<(), String> {
    let split = bulk_source(FORWARDING, "        let _ = order_discount(10);");
    let split = split.replace(
        "fn quota_is_two_hundred() {",
        "fn quota_is_two_hundred() { assert_eq!(order_discount(12), 5);",
    );
    let finding = family_finding(&split, BULK_DIFF, "return_value")?;
    assert_ne!(finding["classification"], "exposed", "{finding}");

    let source = bulk_source(FORWARDING, "        assert_eq!(order_discount(10), 5);")
        .replace("10 <= qty", "(10 <= qty) || true");
    let diff = BULK_DIFF
        .replace("qty >= 10", "(qty >= 10) || true")
        .replace("10 <= qty", "(10 <= qty) || true");
    let finding = family_finding(&source, &diff, "return_value")?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
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
        // `+ 1` over integers is the one admitted shift (#9638,
        // `free_call_return_pin`); a product is still a transform.
        (
            "    unit_cents(qty) * 2",
            "        assert_eq!(order_cents(2), 16);",
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
fn wrapper_that_transforms_the_argument_binds_the_folded_boundary_value() -> Result<(), String> {
    // RIPR-SPEC-0159 computed chain-edge argument: `qty + 1` over the
    // wrapper's own `u32` parameter folds exactly, so `order_discount(9)`
    // hands the owner 10 (the boundary) and `order_discount(8)` hands it 9.
    let computed = "    if is_bulk(qty + 1) {\n        5\n    } else {\n        0\n    }";
    let tests =
        "        assert_eq!(order_discount(9), 5);\n        assert_eq!(order_discount(8), 0);";
    let finding = predicate_finding(&bulk_source(computed, tests), BULK_DIFF)?;
    assert_eq!(
        relation_of(&finding, "ten_items_earn_the_bulk_discount"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    Ok(())
}

#[test]
fn wrapper_that_transforms_the_argument_by_an_unreadable_value_transfers_no_boundary_value()
-> Result<(), String> {
    // Each stimulus is consistent with its wrapper's runtime value (`qty + 1`
    // is the boundary at 9, `qty * 1` at 10), so a retained abstention is the
    // unreadable argument stopping the transfer and not the stimulus
    // contradicting a folded reading.
    let plus_one_tests =
        "        assert_eq!(order_discount(9), 5);\n        assert_eq!(order_discount(8), 0);";
    for (computed, tests) in [
        (
            "    if is_bulk(qty + step()) {\n        5\n    } else {\n        0\n    }",
            plus_one_tests,
        ),
        (
            "    if is_bulk((qty + 1) as u32) {\n        5\n    } else {\n        0\n    }",
            plus_one_tests,
        ),
        (
            "    if is_bulk(qty * LIMIT) {\n        5\n    } else {\n        0\n    }",
            BOUNDARY_TESTS,
        ),
    ] {
        let source = bulk_source(computed, tests).replace(
            "fn is_bulk",
            "const LIMIT: u32 = 1;\nfn step() -> u32 {\n    1\n}\n\nfn is_bulk",
        );
        let finding = predicate_finding(&source, BULK_DIFF)?;
        // Reach still holds through the call; the unreadable hop argument
        // stops the exact-value transfer.
        assert_ne!(
            finding["ripr"]["infect"]["state"], "yes",
            "{computed}: {finding}"
        );
    }
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
            // #10484: the call-built `max(10, 50)` is no longer read as the
            // literal `10`, so the boundary stays a missing discriminator
            // (weak infection) instead of being paired and then refused.
            assert_eq!(
                finding["classification"], "weakly_exposed",
                "{pin}: {finding}"
            );
            assert!(
                finding["ripr"]["infect"]["summary"].as_str().is_some_and(
                    |summary| summary.contains("equality-boundary discriminator is missing")
                ),
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

#[test]
fn qualified_helper_effect_keeps_reach_without_propagation() -> Result<(), String> {
    for (entry, imports, call) in [
        (
            "pub fn wrapper(out: &mut Vec<u32>) { crate::record(out); }",
            "use super::wrapper;",
            "wrapper(&mut out);",
        ),
        (
            "pub struct Recorder {}\nimpl Recorder {\n    pub fn record_all(&self, out: &mut Vec<u32>) {\n        crate::record(out);\n    }\n}",
            "use super::Recorder;",
            "let recorder = Recorder {};\n        recorder.record_all(&mut out);",
        ),
    ] {
        let source = format!(
            "fn record(out: &mut Vec<u32>) {{\n    out.push(10);\n}}\n\n{entry}\n\n#[cfg(test)]\nmod tests {{\n    {imports}\n    #[test]\n    fn observes_record() {{\n        let mut out = Vec::new();\n        {call}\n        assert_eq!(out, vec![10]);\n    }}\n}}\n"
        );
        let report = all_findings(&source, RECORD_DIFF)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        let mut families = Vec::new();
        for finding in findings {
            assert_eq!(finding["probe"]["expression"], "out.push(10)", "{finding}");
            assert_eq!(
                relation_of(finding, "observes_record"),
                Some("helper_owner_call"),
                "{entry}: {finding}"
            );
            assert_not_forwarded(finding);
            families.push(
                finding["probe"]["family"]
                    .as_str()
                    .ok_or("missing family")?,
            );
        }
        families.sort();
        assert_eq!(families, ["call_deletion", "side_effect"], "{report}");
    }
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
    let source = "fn title() -> &'static str {\n    stringify!(ripr)\n}\n\npub fn banner() -> String {\n    format!(\"title\")\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn banner_reads_title() {\n        let _ = title();\n        assert_eq!(banner(), \"title\");\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn title() -> &'static str {\n-    \"ripr\"\n+    stringify!(ripr)\n }\n";
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

const TITLE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn title() -> &'static str {\n-    \"ripr\"\n+    stringify!(ripr)\n }\n";

fn banner_finding(wrapper: &str, test: &str) -> Result<Value, String> {
    let source = format!(
        "fn title() -> &'static str {{\n    stringify!(ripr)\n}}\n\npub fn banner() -> String {{\n    {wrapper}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn banner_check() {{\n        {test}\n    }}\n}}\n"
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
        "fn title() -> &'static str {{\n    stringify!(ripr)\n}}\n\n{items}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn banner_check() {{\n        {test}\n    }}\n}}\n"
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
    let source = "fn title() -> &'static str {\n    stringify!(ripr)\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    fn titled(suffix: &str) -> String {\n        format!(\"{}{}\", title(), suffix)\n    }\n\n    #[test]\n    fn banner_check() {\n        assert!(titled(\"!\").len() > 0);\n    }\n}\n";
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

// #9329: the bytesize shape. The owner has two direct callers (the method a
// test helper reaches and a `Display::fmt` no test calls), the helper asserts
// the method's result unchanged, and the boundary row arrives as a
// const-constructor receiver.
fn bytesize_source(helper_body: &str) -> String {
    format!(
        "pub const KB: u64 = 1_000;\npub const KIB: u64 = 1_024;\n\n#[derive(Clone, Copy)]\npub struct ByteSize(pub u64);\n\nimpl ByteSize {{\n    pub const fn kib(size: u64) -> Self {{\n        ByteSize(size * KIB)\n    }}\n\n    pub fn to_string_as(&self, si_unit: bool) -> String {{\n        to_string(self.0, si_unit)\n    }}\n}}\n\nimpl std::fmt::Display for ByteSize {{\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        f.pad(&to_string(self.0, false))\n    }}\n}}\n\npub fn to_string(bytes: u64, si_prefix: bool) -> String {{\n    let unit = if si_prefix {{ KIB }} else {{ KB }};\n    if bytes <= unit {{\n        format!(\"{{}} B\", bytes)\n    }} else {{\n        format!(\"{{}} big\", bytes / unit)\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn assert_to_string(expected: &str, b: ByteSize, si: bool) {{\n        {helper_body}\n    }}\n\n    #[test]\n    fn test_to_string() {{\n        assert_to_string(\"215 B\", ByteSize(215), true);\n        assert_to_string(\"1 big\", ByteSize::kib(1), true);\n        assert_to_string(\"1 big\", ByteSize::kib(1), false);\n    }}\n}}\n"
    )
}

const BYTESIZE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -24,3 +24,3 @@ pub fn to_string(bytes: u64, si_prefix: bool) -> String {\n     let unit = if si_prefix { KIB } else { KB };\n-    if bytes < unit {\n+    if bytes <= unit {\n         format!(\"{} B\", bytes)\n";

#[test]
fn bytesize_shape_asserting_the_method_result_is_exposed() -> Result<(), String> {
    let source = bytesize_source("assert_eq!(expected.to_string(), b.to_string_as(si));");
    let finding = predicate_finding(&source, BYTESIZE_DIFF)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert_eq!(
        relation_of(&finding, "test_to_string"),
        Some("helper_owner_call"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn bytesize_shape_not_observing_the_method_result_is_not_exposed() -> Result<(), String> {
    for helper in [
        "b.to_string_as(si);\n        assert!(!expected.is_empty());",
        "let shown = b.to_string_as(si);\n        assert_eq!(expected.to_string(), shown);",
    ] {
        let finding = predicate_finding(&bytesize_source(helper), BYTESIZE_DIFF)?;
        assert_ne!(finding["classification"], "exposed", "{helper}: {finding}");
    }
    Ok(())
}
const WIDE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn wide_x(input: &str) -> bool {\n-    input.len() > 3\n+    3 < input.len()\n }\n";

/// RIPR-SPEC-0159 example 9 (#9889): the private helper `wide_x` is reached
/// through `classify_trimmed`, whose call hands it `argument`.
fn wide_source(argument: &str) -> String {
    format!(
        "fn wide_x(input: &str) -> bool {{\n    3 < input.len()\n}}\n\npub fn classify_trimmed(input: &str) -> &'static str {{\n    if wide_x({argument}) {{\n        \"wide\"\n    }} else {{\n        \"narrow\"\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn rows_straddle_four() {{\n        assert_eq!(classify_trimmed(\" abcd \"), \"wide\");\n        assert_eq!(classify_trimmed(\" abc \"), \"narrow\");\n    }}\n}}\n"
    )
}

// #9889: a computed argument on the chain edge (`input.trim()`) stops row
// transfer, so observation through the chain can be neither confirmed nor
// refuted. The return_value probe abstains like its predicate sibling and
// never claims a gap.
#[test]
fn return_value_behind_a_computed_chain_argument_abstains() -> Result<(), String> {
    // `&input` is no strict literal and not the caller parameter text, so
    // `bind_helper_argument` stops the row there too (gemini review, #9947).
    for argument in [
        "input.trim()",
        "input.trim_start()",
        "&input[1..]",
        "&input",
    ] {
        let source = wide_source(argument);
        let predicate = predicate_finding(&source, WIDE_DIFF)?;
        assert_eq!(
            relation_of(&predicate, "rows_straddle_four"),
            Some("helper_owner_call"),
            "{argument}: {predicate}"
        );
        assert_eq!(
            predicate["classification"], "infection_unknown",
            "{argument}: {predicate}"
        );
        let finding = family_finding(&source, WIDE_DIFF, "return_value")?;
        assert_eq!(
            relation_of(&finding, "rows_straddle_four"),
            Some("helper_owner_call"),
            "{argument}: {finding}"
        );
        assert_eq!(
            finding["classification"], "infection_unknown",
            "{argument}: {finding}"
        );
    }
    Ok(())
}

// Control for #9889: where the chain transfers the rows (the entry hands the
// helper its own parameter), the return_value probe keeps its ordinary
// reading, not the computed-argument abstention.
#[test]
fn return_value_behind_a_row_transferring_chain_keeps_its_reading() -> Result<(), String> {
    let finding = family_finding(&wide_source("input"), WIDE_DIFF, "return_value")?;
    assert_eq!(
        relation_of(&finding, "rows_straddle_four"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert!(
        !finding["ripr"]["infect"]["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("computed argument"),
        "{finding}"
    );
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    Ok(())
}

// #9889: the row-binding authority binds only a strict literal or the
// caller's own parameter. A local or a named constant handed to the helper
// stops the transfer just like a method call, so the return_value probe
// abstains; a literal argument binds, so the probe keeps its reading.
#[test]
fn return_value_behind_a_local_or_constant_chain_argument_abstains() -> Result<(), String> {
    for (prelude, argument) in [
        ("    let trimmed = input.trim();\n", "trimmed"),
        ("", "PADDED"),
    ] {
        let source = wide_source(argument)
            .replace("    if wide_x(", &format!("{prelude}    if wide_x("))
            .replace("fn wide_x", "const PADDED: &str = \" abcd \";\n\nfn wide_x");
        let diff = WIDE_DIFF.replace("@@ -1,3 +1,3 @@", "@@ -3,3 +3,3 @@");
        let finding = family_finding(&source, &diff, "return_value")?;
        assert_eq!(
            relation_of(&finding, "rows_straddle_four"),
            Some("helper_owner_call"),
            "{argument}: {finding}"
        );
        assert_eq!(
            finding["classification"], "infection_unknown",
            "{argument}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn return_value_behind_a_literal_chain_argument_keeps_its_reading() -> Result<(), String> {
    let finding = family_finding(&wide_source("\" abcd \""), WIDE_DIFF, "return_value")?;
    assert_eq!(
        relation_of(&finding, "rows_straddle_four"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    assert_ne!(finding["classification"], "infection_unknown", "{finding}");
    Ok(())
}

// #9889 review: `bind_helper_argument` binds `self.<tuple index>` only, so
// any other receiver projection stops the row and abstains; the tuple index
// keeps its reading.
fn receiver_wide_source(argument: &str, field_decl: &str, construct: &str) -> String {
    format!(
        "fn wide_x(input: &str) -> bool {{\n    3 < input.len()\n}}\n\npub struct W{field_decl}\n\nimpl W {{\n    pub fn run(&self, input: &str) -> &'static str {{\n        if wide_x({argument}) {{\n            \"wide\"\n        }} else {{\n            \"narrow\"\n        }}\n    }}\n}}\n\npub fn run_w(input: &str) -> &'static str {{\n    {construct}.run(input)\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn rows_straddle_four() {{\n        assert_eq!(run_w(\" abcd \"), \"wide\");\n        assert_eq!(run_w(\" abc \"), \"narrow\");\n    }}\n}}\n"
    )
}

#[test]
fn return_value_behind_a_receiver_field_chain_argument_abstains() -> Result<(), String> {
    for argument in ["self.pad.as_str()", "&self.pad", "self.pad.as_str().trim()"] {
        let source = receiver_wide_source(
            argument,
            " {\n    pad: String,\n}",
            "W { pad: String::new() }",
        );
        let finding = family_finding(&source, WIDE_DIFF, "return_value")?;
        assert_eq!(
            relation_of(&finding, "rows_straddle_four"),
            Some("helper_owner_call"),
            "{argument}: {finding}"
        );
        assert_eq!(
            finding["classification"], "infection_unknown",
            "{argument}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn return_value_behind_a_receiver_tuple_index_keeps_its_reading() -> Result<(), String> {
    let source = receiver_wide_source("self.0", "(&'static str);", "W(\"\")");
    let finding = family_finding(&source, WIDE_DIFF, "return_value")?;
    assert_ne!(finding["classification"], "infection_unknown", "{finding}");
    Ok(())
}

// #9638: a test reaches an inherent owner method through a same-impl
// forwarder method it calls with a receiver (`pool.grow(4)` for
// `fn grow(&mut self, ..) { self.try_grow(..) }`).
//
// The owner `try_grow` is line 8 of the source (the edited `wanted >= 4`).
const FORWARDER_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,7 +5,7 @@\n }\n impl Pool {\n     pub fn try_grow(&mut self, wanted: usize) -> Result<bool, Overflow> {\n-        if wanted > 4 {\n+        if wanted >= 4 {\n             self.spilled = true;\n             return Ok(true);\n         }\n";

fn forwarder_source(extra_items: &str, forwarder: &str, tests: &str) -> String {
    format!(
        "#[derive(Debug, PartialEq)]\npub struct Overflow;\npub struct Pool {{\n    spilled: bool,\n}}\nimpl Pool {{\n    pub fn try_grow(&mut self, wanted: usize) -> Result<bool, Overflow> {{\n        if wanted >= 4 {{\n            self.spilled = true;\n            return Ok(true);\n        }}\n        Ok(false)\n    }}\n\n{forwarder}\n}}\n{extra_items}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn grow_pins_the_boundary() {{\n        let mut pool = Pool {{ spilled: false }};\n{tests}\n    }}\n}}\n"
    )
}

const FORWARDING_METHOD: &str = "    pub fn grow(&mut self, wanted: usize) -> Result<bool, Overflow> {\n        self.try_grow(wanted)\n    }";
const GROW_TESTS: &str =
    "        assert_eq!(pool.grow(3), Ok(false));\n        assert_eq!(pool.grow(4), Ok(true));";

fn forwarder_finding(extra_items: &str, forwarder: &str, tests: &str) -> Result<Value, String> {
    let finding = predicate_finding(
        &forwarder_source(extra_items, forwarder, tests),
        FORWARDER_DIFF,
    )?;
    assert_eq!(finding["probe"]["expression"], "wanted >= 4", "{finding}");
    Ok(finding)
}

#[test]
fn forwarder_method_relates_and_pairs_its_boundary_pin() -> Result<(), String> {
    let finding = forwarder_finding("", FORWARDING_METHOD, GROW_TESTS)?;
    assert_eq!(
        relation_of(&finding, "grow_pins_the_boundary"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["ripr"]["reach"]["state"], "yes", "{finding}");
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn qualified_hop_above_a_tested_method_keeps_the_lower_forwarder() -> Result<(), String> {
    let finding = forwarder_finding(
        "pub fn bridge(wanted: usize) -> Result<bool, Overflow> { Pool { spilled: false }.grow(wanted) }\npub fn above(wanted: usize) -> Result<bool, Overflow> { crate::bridge(wanted) }",
        FORWARDING_METHOD,
        GROW_TESTS,
    )?;
    assert_eq!(
        relation_of(&finding, "grow_pins_the_boundary"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn forwarder_method_that_drops_or_transforms_the_result_is_not_credited() -> Result<(), String> {
    for forwarder in [
        "    pub fn grow(&mut self, wanted: usize) -> bool {\n        let _ = self.try_grow(wanted);\n        true\n    }",
        "    pub fn grow(&mut self, wanted: usize) -> bool {\n        self.try_grow(wanted).is_ok()\n    }",
        "    pub fn grow(&mut self, wanted: usize) -> bool {\n        self.try_grow(wanted).unwrap_or(false) || wanted > 100\n    }",
    ] {
        let tests = "        assert!(pool.grow(3) || !pool.grow(4));";
        let finding = forwarder_finding("", forwarder, tests)?;
        assert_ne!(
            finding["classification"], "exposed",
            "{forwarder}: {finding}"
        );
        assert_ne!(
            finding["classification"], "weakly_exposed",
            "{forwarder}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn forwarder_method_name_shared_with_another_type_is_not_related() -> Result<(), String> {
    let other = "pub struct Fin;\nimpl Fin {\n    pub fn grow(&mut self, wanted: usize) -> Result<bool, Overflow> {\n        Ok(wanted > 1)\n    }\n}";
    let finding = forwarder_finding(other, FORWARDING_METHOD, GROW_TESTS)?;
    assert_ne!(
        relation_of(&finding, "grow_pins_the_boundary"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn forwarder_method_to_an_owner_name_with_two_candidates_is_not_related() -> Result<(), String> {
    let other = "pub struct Fin;\nimpl Fin {\n    pub fn try_grow(&mut self, wanted: usize) -> Result<bool, Overflow> {\n        Ok(wanted > 1)\n    }\n}";
    let finding = forwarder_finding(other, FORWARDING_METHOD, GROW_TESTS)?;
    assert_ne!(
        relation_of(&finding, "grow_pins_the_boundary"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn forwarder_method_shadowed_in_the_test_is_not_related() -> Result<(), String> {
    let tests = "        fn grow(_pool: &mut Pool, wanted: usize) -> Result<bool, Overflow> {\n            Ok(wanted >= 4)\n        }\n        assert_eq!(grow(&mut pool, 3), Ok(false));\n        assert_eq!(pool.grow(4), Ok(true));";
    let finding = forwarder_finding("", FORWARDING_METHOD, tests)?;
    assert_ne!(
        relation_of(&finding, "grow_pins_the_boundary"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn forwarder_method_called_by_path_or_value_is_not_related() -> Result<(), String> {
    for tests in [
        "        assert_eq!(Pool::grow(&mut pool, 4), Ok(true));",
        "        let call = Pool::grow;\n        assert_eq!(call(&mut pool, 4), Ok(true));",
    ] {
        let finding = forwarder_finding("", FORWARDING_METHOD, tests)?;
        assert_ne!(
            relation_of(&finding, "grow_pins_the_boundary"),
            Some("helper_owner_call"),
            "{tests}: {finding}"
        );
        assert_ne!(finding["classification"], "exposed", "{tests}: {finding}");
    }
    Ok(())
}

fn forwarder_named(name: &str) -> String {
    format!(
        "    pub fn {name}(&mut self, wanted: usize) -> Result<bool, Overflow> {{\n        self.try_grow(wanted)\n    }}"
    )
}

fn assert_forwarder_not_related(finding: &Value, label: &str) {
    assert_ne!(
        relation_of(finding, "grow_pins_the_boundary"),
        Some("helper_owner_call"),
        "{label}: {finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{label}: {finding}");
}

// #9638 review F1: a method name is not type identity. A forwarder named like
// a std method is entered only by a receiver the test built as the owner's own
// type, never by `Vec::len`, `HashSet::insert`, `String::is_empty` or an
// iterator's `next`.
#[test]
fn forwarder_method_named_like_a_std_method_is_not_entered_by_a_std_receiver() -> Result<(), String>
{
    for (name, tests) in [
        (
            "len",
            "        let v = vec![1, 2];\n        assert_eq!(v.len(), 2);",
        ),
        (
            "insert",
            "        let mut set = std::collections::HashSet::new();\n        assert!(set.insert(4));",
        ),
        ("is_empty", "        assert!(String::new().is_empty());"),
        (
            "next",
            "        let mut it = vec![1].into_iter();\n        assert_eq!(it.next(), Some(1));",
        ),
        ("len", "        assert_eq!(vec![1, 2].len(), 2);"),
    ] {
        let finding = forwarder_finding("", &forwarder_named(name), tests)?;
        assert_forwarder_not_related(&finding, &format!("{name}: {tests}"));
    }
    Ok(())
}

#[test]
fn forwarder_method_named_like_a_std_method_is_entered_by_the_real_owner_type() -> Result<(), String>
{
    for constructor in [
        "        let mut pool = Pool { spilled: false };",
        "        let mut pool: Pool = Pool { spilled: false };",
    ] {
        let tests = format!(
            "{constructor}\n        assert_eq!(pool.len(3), Ok(false));\n        assert_eq!(pool.len(4), Ok(true));"
        );
        let finding = forwarder_finding("", &forwarder_named("len"), &tests)?;
        assert_eq!(
            relation_of(&finding, "grow_pins_the_boundary"),
            Some("helper_owner_call"),
            "{finding}"
        );
        assert_eq!(finding["classification"], "exposed", "{finding}");
    }
    Ok(())
}

#[test]
fn forwarder_method_receiver_of_an_unestablished_type_is_not_related() -> Result<(), String> {
    for tests in [
        // A parameter-typed or rebound receiver is not built as a `Pool` here.
        "        let mut pool = std::collections::HashSet::<usize>::new();\n        assert!(pool.grow(4).is_ok());",
        "        let mut pool = make_pool();\n        assert_eq!(pool.grow(4), Ok(true));",
        "        let mut pool = Pool { spilled: false };\n        let mut pool = vec![1];\n        assert_eq!(pool.grow(4), Ok(true));",
    ] {
        let finding = forwarder_finding(
            "fn make_pool() -> Pool { Pool { spilled: false } }",
            FORWARDING_METHOD,
            tests,
        )?;
        assert_forwarder_not_related(&finding, tests);
    }
    Ok(())
}

// Review: pin each production-side guard of the method entry.
#[test]
fn forwarder_method_entry_guards_each_refuse_their_shape() -> Result<(), String> {
    let cfg_test_impl = "#[cfg(test)]\nimpl Pool {\n    pub fn grow(&mut self, wanted: usize) -> Result<bool, Overflow> {\n        self.try_grow(wanted)\n    }\n}";
    let cfg_test_module = "#[cfg(test)]\nmod helpers {\n    use super::*;\n    impl Pool {\n        pub fn grow(&mut self, wanted: usize) -> Result<bool, Overflow> {\n            self.try_grow(wanted)\n        }\n    }\n}";
    for (label, extra, forwarder) in [
        ("cfg(test) impl", cfg_test_impl, ""),
        ("cfg(test) module", cfg_test_module, ""),
        (
            "associated fn without self",
            "",
            "    pub fn grow(wanted: usize) -> Result<bool, Overflow> {\n        Pool { spilled: false }.try_grow(wanted)\n    }",
        ),
        (
            "self by value",
            "",
            "    pub fn grow(mut self, wanted: usize) -> Result<bool, Overflow> {\n        self.try_grow(wanted)\n    }",
        ),
        (
            "typed self",
            "",
            "    pub fn grow(self: &mut Self, wanted: usize) -> Result<bool, Overflow> {\n        self.try_grow(wanted)\n    }",
        ),
    ] {
        let finding = forwarder_finding(extra, forwarder, GROW_TESTS)?;
        assert_forwarder_not_related(&finding, label);
    }
    Ok(())
}

const MACRO_HOP_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -7,3 +7,3 @@\n pub fn base_rate() -> u32 {\n-    7\n+    3 + 4\n }\n";

#[test]
fn forwarder_method_above_a_macro_argument_hop_is_not_related() -> Result<(), String> {
    let source = "macro_rules! twice {\n    ($f:ident) => { $f() * 2 };\n}\n\npub fn other_rate() -> u32 { 5 }\n\npub fn base_rate() -> u32 {\n    3 + 4\n}\n\npub fn doubled_rate() -> u32 {\n    twice!(base_rate)\n}\n\npub struct Meter {\n    n: u32,\n}\nimpl Meter {\n    pub fn read(&self) -> u32 {\n        doubled_rate()\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn read_is_pinned() {\n        let meter = Meter { n: 0 };\n        assert_eq!(meter.read(), 14);\n    }\n}\n";
    let finding = family_finding(source, MACRO_HOP_DIFF, "return_value")?;
    assert_ne!(
        relation_of(&finding, "read_is_pinned"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// A test in another package cannot use the receiver-call entry: the method
// entry never bypasses the package guard.
#[test]
fn forwarder_method_called_from_another_package_is_not_related() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    let write = |path: &str, text: &str| -> Result<(), String> {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(full, text).map_err(|error| error.to_string())
    };
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"owner\", \"user\"]\nresolver = \"2\"\n",
    )?;
    write(
        "owner/Cargo.toml",
        "[package]\nname = \"owner\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
    )?;
    write(
        "user/Cargo.toml",
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[dependencies]\nowner = { path = \"../owner\" }\n",
    )?;
    let lib = forwarder_source("", FORWARDING_METHOD, "")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .to_string();
    write("owner/src/lib.rs", &lib)?;
    write(
        "user/src/lib.rs",
        "#[cfg(test)]\nmod tests {\n    use owner::{Overflow, Pool};\n\n    #[test]\n    fn grow_from_elsewhere() {\n        let mut pool = Pool { spilled: false };\n        assert_eq!(pool.grow(4), Ok(true));\n        let _: Option<Overflow> = None;\n    }\n}\n",
    )?;
    let diff = FORWARDER_DIFF
        .replace("a/src/lib.rs", "a/owner/src/lib.rs")
        .replace("b/src/lib.rs", "b/owner/src/lib.rs");
    let diff_file = root.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))?;
    assert_ne!(
        relation_of(&finding, "grow_from_elsewhere"),
        Some("helper_owner_call"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn forwarder_method_of_an_external_trait_impl_is_not_related() -> Result<(), String> {
    let trait_impl = "impl std::ops::AddAssign<usize> for Pool {\n    fn add_assign(&mut self, wanted: usize) {\n        let _ = self.try_grow(wanted);\n    }\n}";
    let tests =
        "        pool.add_assign(3);\n        pool.add_assign(4);\n        assert!(pool.spilled);";
    let finding = forwarder_finding(trait_impl, "", tests)?;
    assert_forwarder_not_related(&finding, "external trait impl");
    Ok(())
}

// #9638 re-review F2: identity is the declaration, not the bare name. A
// sibling-module type of the same name, imported into the test, is not the
// forwarder's `Pool`, even when it has no forwarder of its own (it derefs to
// `Vec`, whose `reserve` is the call).
const ALT_POOL: &str = "pub mod alt {\n    pub struct Pool {\n        pub v: Vec<usize>,\n    }\n    impl std::ops::Deref for Pool {\n        type Target = Vec<usize>;\n        fn deref(&self) -> &Vec<usize> {\n            &self.v\n        }\n    }\n    impl std::ops::DerefMut for Pool {\n        fn deref_mut(&mut self) -> &mut Vec<usize> {\n            &mut self.v\n        }\n    }\n}";

#[test]
fn forwarder_method_receiver_of_a_same_named_sibling_type_is_not_related() -> Result<(), String> {
    let reserve = forwarder_named("reserve");
    for tests in [
        "        use super::alt::Pool;\n        let mut pool = Pool { v: vec![1] };\n        pool.reserve(4);\n        assert!(pool.capacity() >= 4);",
        "        use super::alt::Pool;\n        assert_eq!(Pool { v: vec![1] }.len(), 1);\n        let mut pool = Pool { v: vec![1] };\n        pool.reserve(4);",
    ] {
        let finding = forwarder_finding(ALT_POOL, &reserve, tests)?;
        assert_forwarder_not_related(&finding, tests);
    }
    Ok(())
}

#[test]
fn forwarder_method_receiver_of_a_same_named_type_in_an_integration_test_is_not_related()
-> Result<(), String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(root.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    let lib = forwarder_source(ALT_POOL, &forwarder_named("reserve"), "")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .to_string();
    std::fs::write(root.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("tests/alt.rs"),
        "use wrapper_reach::alt::Pool;\n\n#[test]\nfn reserve_on_the_vec() {\n    let mut pool = Pool { v: vec![1] };\n    pool.reserve(4);\n    assert!(pool.capacity() >= 4);\n}\n",
    )
    .map_err(|error| error.to_string())?;
    let diff_file = root.join("change.diff");
    std::fs::write(&diff_file, FORWARDER_DIFF).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))?;
    assert_ne!(
        relation_of(&finding, "reserve_on_the_vec"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// A root-package owner has no package prefix, so the receiver-call entry must
// not admit a workspace member's test by default.
#[test]
fn forwarder_method_called_from_a_member_of_a_root_package_is_not_related() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    let write = |path: &str, text: &str| -> Result<(), String> {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(full, text).map_err(|error| error.to_string())
    };
    write(
        "Cargo.toml",
        &format!("{MANIFEST}\n[workspace]\nmembers = [\"user\"]\n\n[dependencies]\n"),
    )?;
    write(
        "user/Cargo.toml",
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[dependencies]\nwrapper_reach = { path = \"..\" }\n",
    )?;
    let lib = forwarder_source("", FORWARDING_METHOD, "")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .to_string();
    write("src/lib.rs", &lib)?;
    write(
        "user/tests/main.rs",
        "use wrapper_reach::Pool;\n\n#[test]\nfn grow_from_a_member() {\n    let mut pool = Pool { spilled: false };\n    assert_eq!(pool.grow(4), Ok(true));\n}\n",
    )?;
    write(
        "tests/root.rs",
        "use wrapper_reach::Pool;\n\n#[test]\nfn grow_from_the_root_package() {\n    let mut pool = Pool { spilled: false };\n    assert_eq!(pool.grow(4), Ok(true));\n}\n",
    )?;
    let diff_file = root.join("change.diff");
    std::fs::write(&diff_file, FORWARDER_DIFF).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))?;
    assert_ne!(
        relation_of(&finding, "grow_from_a_member"),
        Some("helper_owner_call"),
        "{finding}"
    );
    assert_eq!(
        relation_of(&finding, "grow_from_the_root_package"),
        Some("helper_owner_call"),
        "{finding}"
    );
    Ok(())
}

// #9638 (tail slice): a forwarder that unwraps the owner's `Result` with a
// handler that only diverges (`.unwrap_or_else(die)`) hands the owner's `Ok`
// value through unchanged. Every other tail stays a transform and abstains.
const DIVERGING_DIE: &str = "fn die<T>(_: Overflow) -> T {\n    panic!(\"overflow\")\n}\n";
const UNWRAP_TESTS: &str =
    "        assert_eq!(pool.grow(3), false);\n        assert_eq!(pool.grow(4), true);";

fn unwrapping_forwarder(tail: &str) -> String {
    format!("    pub fn grow(&mut self, wanted: usize) -> bool {{\n        {tail}\n    }}")
}

fn assert_unwrap_tail_not_credited(extra: &str, tail: &str) -> Result<(), String> {
    let finding = forwarder_finding(extra, &unwrapping_forwarder(tail), UNWRAP_TESTS)?;
    assert_ne!(finding["classification"], "exposed", "{tail}: {finding}");
    // When the chain does resolve to the forwarder, the tail is the reason it
    // abstains; an unresolved caller keeps its file-proximity reading.
    if relation_of(&finding, "grow_pins_the_boundary") == Some("helper_owner_call") {
        assert_eq!(
            finding["classification"], "propagation_unknown",
            "{tail}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn unwrap_tail_with_a_value_substituting_handler_is_not_credited() -> Result<(), String> {
    for tail in [
        "self.try_grow(wanted).unwrap_or_else(|_| false)",
        "self.try_grow(wanted).unwrap_or(false)",
        "self.try_grow(wanted).unwrap_or_default()",
        "self.try_grow(wanted).ok().unwrap_or(false)",
        "self.try_grow(wanted).is_ok()",
        "self.try_grow(wanted).unwrap()",
        "self.try_grow(wanted).expect(\"grow\")",
        "!self.try_grow(wanted).unwrap_or_else(die)",
        "self.try_grow(wanted).map(|spilled| !spilled).unwrap_or_else(die)",
        "self.try_grow(wanted).unwrap_or_else(die) && wanted > 100",
        "let _ = self.try_grow(wanted).unwrap_or_else(die);\n        true",
    ] {
        assert_unwrap_tail_not_credited(DIVERGING_DIE, tail)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_with_a_non_diverging_or_conditional_handler_is_not_credited() -> Result<(), String> {
    for (extra, tail) in [
        (
            "fn soft<T: Default>(_: Overflow) -> T {\n    T::default()\n}\n",
            "self.try_grow(wanted).unwrap_or_else(soft)",
        ),
        (
            "fn maybe<T: Default>(_: Overflow) -> T {\n    if std::env::var(\"STRICT\").is_ok() {\n        panic!(\"overflow\");\n    }\n    T::default()\n}\n",
            "self.try_grow(wanted).unwrap_or_else(maybe)",
        ),
        (
            "fn early<T: Default>(_: Overflow) -> T {\n    return T::default();\n    #[allow(unreachable_code)]\n    panic!(\"overflow\")\n}\n",
            "self.try_grow(wanted).unwrap_or_else(early)",
        ),
        (
            "fn guarded<T: Default>(_: Overflow) -> T {\n    let strict = false;\n    if strict {\n        panic!(\"overflow\");\n    } else {\n        T::default()\n    }\n}\n",
            "self.try_grow(wanted).unwrap_or_else(guarded)",
        ),
    ] {
        assert_unwrap_tail_not_credited(extra, tail)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_with_an_ambiguous_or_external_handler_is_not_credited() -> Result<(), String> {
    let twin = "mod elsewhere {\n    pub fn die<T: Default>(_: super::Overflow) -> T {\n        T::default()\n    }\n}\n";
    for (extra, tail) in [
        // A second workspace function of the name.
        (
            format!("{DIVERGING_DIE}{twin}"),
            "self.try_grow(wanted).unwrap_or_else(die)",
        ),
        // A module-qualified or external path.
        (
            format!("{DIVERGING_DIE}{twin}"),
            "self.try_grow(wanted).unwrap_or_else(elsewhere::die)",
        ),
        (
            DIVERGING_DIE.to_string(),
            "self.try_grow(wanted).unwrap_or_else(std::process::abort)",
        ),
        (
            DIVERGING_DIE.to_string(),
            "self.try_grow(wanted).unwrap_or_else(Default::default)",
        ),
        // A unique module-qualified path, a free fn named through a type, a
        // renamed import and a handler that exists only in test code.
        (
            "mod util {\n    pub fn die<T>(_: super::Overflow) -> T {\n        panic!(\"overflow\")\n    }\n}\n".to_string(),
            "self.try_grow(wanted).unwrap_or_else(util::die)",
        ),
        (
            DIVERGING_DIE.to_string(),
            "self.try_grow(wanted).unwrap_or_else(Overflow::die)",
        ),
        (
            format!("{DIVERGING_DIE}use std::mem::drop as die;\n"),
            "self.try_grow(wanted).unwrap_or_else(die)",
        ),
        (
            "#[cfg(test)]\nmod helpers {\n    pub fn die<T>(_: super::Overflow) -> T {\n        panic!(\"overflow\")\n    }\n}\n".to_string(),
            "self.try_grow(wanted).unwrap_or_else(die)",
        ),
        // A user macro that replaces `panic!` makes the body non-diverging.
        (
            format!("macro_rules! panic {{\n    ($($t:tt)*) => {{ Default::default() }};\n}}\n{DIVERGING_DIE}"),
            "self.try_grow(wanted).unwrap_or_else(die)",
        ),
        // A local binding that shadows the handler name.
        (
            DIVERGING_DIE.to_string(),
            "let die = |_: Overflow| false;\n        self.try_grow(wanted).unwrap_or_else(die)",
        ),
    ] {
        assert_unwrap_tail_not_credited(&extra, tail)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_with_a_closure_handler_is_not_credited() -> Result<(), String> {
    for tail in [
        "self.try_grow(wanted).unwrap_or_else(|_| panic!(\"overflow\"))",
        "self.try_grow(wanted).unwrap_or_else(|error| { drop(error); false })",
        "self.try_grow(wanted).unwrap_or_else(|_| { let _quiet = 1; false })",
        "self.try_grow(wanted).unwrap_or_else(|error| die(error))",
    ] {
        assert_unwrap_tail_not_credited(DIVERGING_DIE, tail)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_with_a_diverging_workspace_handler_pairs_the_boundary_pin() -> Result<(), String> {
    for (extra, tail) in [
        (DIVERGING_DIE, "self.try_grow(wanted).unwrap_or_else(die)"),
        (
            "impl Overflow {\n    pub fn handle<T>(self) -> T {\n        panic!(\"overflow\")\n    }\n}\n",
            "self.try_grow(wanted).unwrap_or_else(Overflow::handle)",
        ),
        (
            "fn die<T>(_: Overflow) -> T {\n    unreachable!(\"overflow\");\n}\n",
            "self.try_grow(wanted).unwrap_or_else(die)",
        ),
        (
            "fn die<T>(_: Overflow) -> T {\n    std::process::abort()\n}\n",
            "self.try_grow(wanted).unwrap_or_else(die)",
        ),
    ] {
        let finding = forwarder_finding(extra, &unwrapping_forwarder(tail), UNWRAP_TESTS)?;
        assert_eq!(
            relation_of(&finding, "grow_pins_the_boundary"),
            Some("helper_owner_call"),
            "{tail}: {finding}"
        );
        assert_eq!(finding["classification"], "exposed", "{tail}: {finding}");
    }
    Ok(())
}

// #9638 review of the diverging tail: the handler name must really denote the
// unique workspace function at the call, not a parameter, a nested item, a
// glob-imported item, or a method of a custom return type.
fn assert_shadowed_handler_not_credited(
    extra: &str,
    forwarder: &str,
    tests: &str,
) -> Result<(), String> {
    let finding = forwarder_finding(extra, forwarder, tests)?;
    assert_ne!(
        finding["classification"], "exposed",
        "{forwarder}: {finding}"
    );
    Ok(())
}

#[test]
fn unwrap_tail_handler_name_bound_by_the_caller_is_not_credited() -> Result<(), String> {
    let tests = "        assert_eq!(pool.grow(3, |_| false), false);\n        assert_eq!(pool.grow(4, |_| false), true);";
    for signature in [
        "wanted: usize, die: fn(Overflow) -> bool",
        "wanted: usize, die: impl FnOnce(Overflow) -> bool",
        "wanted: usize, die: F",
    ] {
        let generics = if signature.ends_with(": F") {
            "<F: FnOnce(Overflow) -> bool>"
        } else {
            ""
        };
        let forwarder = format!(
            "    pub fn grow{generics}(&mut self, {signature}) -> bool {{\n        self.try_grow(wanted).unwrap_or_else(die)\n    }}"
        );
        assert_shadowed_handler_not_credited(DIVERGING_DIE, &forwarder, tests)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_handler_name_declared_inside_the_caller_is_not_credited() -> Result<(), String> {
    for local in [
        "fn die(_: Overflow) -> bool {\n            false\n        }",
        "const die: fn(Overflow) -> bool = |_| false;",
        "static die: fn(Overflow) -> bool = |_| false;",
        "let die = |_: Overflow| false;",
        "use external_crate::*;",
    ] {
        let forwarder = format!(
            "    pub fn grow(&mut self, wanted: usize) -> bool {{\n        {local}\n        self.try_grow(wanted).unwrap_or_else(die)\n    }}"
        );
        assert_shadowed_handler_not_credited(DIVERGING_DIE, &forwarder, UNWRAP_TESTS)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_handler_in_another_module_or_behind_a_foreign_glob_is_not_credited()
-> Result<(), String> {
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(die)");
    for extra in [
        // The unique `die` lives in a module the caller does not import; a
        // foreign glob may supply the `die` the caller actually names.
        "use external_crate::*;\nmod hidden {\n    pub fn die<T>(_: super::Overflow) -> T {\n        panic!(\"overflow\")\n    }\n}\n",
        // The same without a glob: the module is simply not in scope.
        "mod hidden {\n    pub fn die<T>(_: super::Overflow) -> T {\n        panic!(\"overflow\")\n    }\n}\n",
    ] {
        assert_shadowed_handler_not_credited(extra, &forwarder, UNWRAP_TESTS)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_on_a_custom_type_with_its_own_unwrap_or_else_is_not_credited() -> Result<(), String>
{
    let custom = "pub struct Answer(bool);\nimpl Answer {\n    pub fn unwrap_or_else<F: FnOnce(Overflow) -> bool>(self, _handler: F) -> bool {\n        !self.0\n    }\n}\n";
    let pool_extra = format!("{DIVERGING_DIE}{custom}");
    // `try_grow` itself must return the custom type for the hop to matter, so
    // the owner is replaced by a wrapper-free variant of the fixture.
    let source = forwarder_source(
        &pool_extra,
        &unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(die)"),
        UNWRAP_TESTS,
    )
    .replace(
        "pub fn try_grow(&mut self, wanted: usize) -> Result<bool, Overflow> {",
        "pub fn try_grow(&mut self, wanted: usize) -> Answer {",
    )
    .replace("return Ok(true);", "return Answer(true);")
    .replace(
        "        Ok(false)\n    }\n\n",
        "        Answer(false)\n    }\n\n",
    );
    let finding = predicate_finding(&source, FORWARDER_DIFF)?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn unwrap_tail_with_a_workspace_result_alias_is_not_credited() -> Result<(), String> {
    // A workspace type named `Result` may carry its own `unwrap_or_else`.
    let extra = format!("{DIVERGING_DIE}pub type Result<T> = core::result::Result<T, Overflow>;\n");
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(die)");
    assert_shadowed_handler_not_credited(&extra, &forwarder, UNWRAP_TESTS)
}

#[test]
fn unwrap_tail_with_a_handler_in_another_file_is_not_credited() -> Result<(), String> {
    let scratch = Scratch::create()?;
    let root = &scratch.0;
    std::fs::write(root.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(die)");
    let source = forwarder_source("mod util;\nuse util::die;\n", &forwarder, UNWRAP_TESTS);
    std::fs::write(root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/util.rs"),
        "use super::Overflow;\n\npub fn die<T>(_: Overflow) -> T {\n    panic!(\"overflow\")\n}\n",
    )
    .map_err(|error| error.to_string())?;
    let diff_file = root.join("change.diff");
    std::fs::write(&diff_file, FORWARDER_DIFF).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// #9638 re-review: the qualifier of `Type::die` must really be the workspace
// type, not a generic parameter, a local item or an import of the same name.
const POOL_DIE: &str =
    "impl Pool {\n    pub fn die<T>(_: Overflow) -> T {\n        panic!(\"overflow\")\n    }\n}\n";

#[test]
fn unwrap_tail_with_the_real_qualifier_type_is_credited() -> Result<(), String> {
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(Pool::die)");
    let finding = forwarder_finding(POOL_DIE, &forwarder, UNWRAP_TESTS)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn unwrap_tail_qualifier_that_is_not_the_workspace_type_is_not_credited() -> Result<(), String> {
    // `ExtTr` is an external trait: declaring it here would add a second `die`.
    let ext_trait = "";
    for (extra, forwarder) in [
        (
            POOL_DIE,
            "    pub fn grow<Pool: ExtTr>(&mut self, wanted: usize) -> bool {\n        self.try_grow(wanted).unwrap_or_else(Pool::die)\n    }",
        ),
        (
            POOL_DIE,
            "    pub fn grow<Pool>(&mut self, wanted: usize) -> bool\n    where\n        Pool: ExtTr,\n    {\n        self.try_grow(wanted).unwrap_or_else(Pool::die)\n    }",
        ),
        (
            POOL_DIE,
            "    pub fn grow(&mut self, wanted: usize) -> bool {\n        type Pool = ExtTy;\n        self.try_grow(wanted).unwrap_or_else(Pool::die)\n    }",
        ),
        (
            POOL_DIE,
            "    pub fn grow(&mut self, wanted: usize) -> bool {\n        struct Pool;\n        impl Pool {\n            fn die<T>(_: Overflow) -> T {\n                loop {}\n            }\n        }\n        self.try_grow(wanted).unwrap_or_else(Pool::die)\n    }",
        ),
        (
            POOL_DIE,
            "    pub fn grow(&mut self, wanted: usize) -> bool {\n        use ext::Pool;\n        self.try_grow(wanted).unwrap_or_else(Pool::die)\n    }",
        ),
    ] {
        let extra = format!("{ext_trait}{extra}");
        assert_shadowed_handler_not_credited(&extra, forwarder, UNWRAP_TESTS)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_qualifier_generic_over_the_enclosing_impl_is_not_credited() -> Result<(), String> {
    // `ExtTr` is an external trait: declaring it here would add a second `die`.
    let ext_trait = "";
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(Pool::die)");
    for header in [
        "impl<Pool: ExtTr> Pool {",
        "impl<Pool> Pool where Pool: ExtTr {",
    ] {
        let source = forwarder_source(&format!("{ext_trait}{POOL_DIE}"), &forwarder, UNWRAP_TESTS)
            .replacen(
                "impl Pool {\n    pub fn try_grow",
                &format!("{header}\n    pub fn try_grow"),
                1,
            );
        let finding = predicate_finding(&source, FORWARDER_DIFF)?;
        assert_ne!(finding["classification"], "exposed", "{header}: {finding}");
    }
    Ok(())
}

#[test]
fn unwrap_tail_caller_nested_in_a_function_with_a_local_handler_is_not_credited()
-> Result<(), String> {
    // The nested impl sees the enclosing function's own `die`, not the
    // module-level one.
    let extra = format!(
        "{DIVERGING_DIE}fn outer() {{\n    const die: fn(Overflow) -> bool = |_| false;\n    impl Pool {{\n        pub fn grow2(&mut self, wanted: usize) -> bool {{\n            self.try_grow(wanted).unwrap_or_else(die)\n        }}\n    }}\n}}\n"
    );
    let tests =
        "        assert_eq!(pool.grow2(3), false);\n        assert_eq!(pool.grow2(4), true);";
    assert_shadowed_handler_not_credited(&extra, "", tests)
}

// #9638 re-review: an enclosing function keeps its scope however its
// signature or name spells `impl`, and a handler in another (one-line)
// module is not the caller's.
fn nested_grow2_extra(outer_head: &str) -> String {
    format!(
        "{DIVERGING_DIE}{outer_head} {{\n    const die: fn(Overflow) -> bool = |_| false;\n    impl Pool {{\n        pub fn grow2(&mut self, wanted: usize) -> bool {{\n            self.try_grow(wanted).unwrap_or_else(die)\n        }}\n    }}\n}}\n"
    )
}

const GROW2_TESTS: &str =
    "        assert_eq!(pool.grow2(3), false);\n        assert_eq!(pool.grow2(4), true);";

#[test]
fn unwrap_tail_caller_in_a_function_whose_spelling_contains_impl_is_not_credited()
-> Result<(), String> {
    for head in [
        "fn outer(_a: impl Copy)",
        "fn outer() -> impl Copy",
        "fn implode()",
        "fn simple_impl()",
        "pub unsafe fn outer()",
    ] {
        assert_shadowed_handler_not_credited(&nested_grow2_extra(head), "", GROW2_TESTS)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_handler_in_the_callers_own_module_stays_credited() -> Result<(), String> {
    let extra = format!(
        "mod m {{\n    use super::*;\n{}\n    impl Pool {{\n        pub fn grow2(&mut self, wanted: usize) -> bool {{\n            self.try_grow(wanted).unwrap_or_else(die)\n        }}\n    }}\n}}\n",
        DIVERGING_DIE.replace("fn die", "pub fn die")
    );
    let finding = forwarder_finding(&extra, "", GROW2_TESTS)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn unwrap_tail_handler_in_a_one_line_sibling_module_is_not_credited() -> Result<(), String> {
    for extra in [
        "mod a { pub fn die<T>(_: super::Overflow) -> T { panic!(\"overflow\") } }\nimpl Pool {\n    pub fn grow2(&mut self, wanted: usize) -> bool {\n        self.try_grow(wanted).unwrap_or_else(die)\n    }\n}\n",
        "mod a { pub fn die<T>(_: super::Overflow) -> T { panic!(\"overflow\") } } impl Pool { pub fn grow2(&mut self, wanted: usize) -> bool { self.try_grow(wanted).unwrap_or_else(die) } }\n",
    ] {
        assert_shadowed_handler_not_credited(extra, "", GROW2_TESTS)?;
    }
    Ok(())
}

// #9638 review lead: a qualifier or `std` path must really denote the
// workspace type / the standard library.
#[test]
fn unwrap_tail_type_qualified_handler_on_a_different_or_ambiguous_type_is_not_credited()
-> Result<(), String> {
    // `Self::die` from an impl of a different type than the one defining it.
    let other = "pub struct Other;\nimpl Other {\n    pub fn die<T>(_: Overflow) -> T {\n        panic!(\"overflow\")\n    }\n}\n";
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(Self::die)");
    assert_shadowed_handler_not_credited(other, &forwarder, UNWRAP_TESTS)?;
    // `Pool::die` when two workspace types are named `Pool` (the second in
    // another module, with the inherent `die`).
    let twin = "mod alt {\n    pub struct Pool;\n    impl Pool {\n        pub fn die<T>(_: super::Overflow) -> T {\n            panic!(\"overflow\")\n        }\n    }\n}\n";
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(Pool::die)");
    assert_shadowed_handler_not_credited(twin, &forwarder, UNWRAP_TESTS)?;
    Ok(())
}

#[test]
fn unwrap_tail_process_exit_handler_with_a_rebound_std_is_not_credited() -> Result<(), String> {
    let abort_die = "fn die<T>(_: Overflow) -> T {\n    std::process::abort()\n}\n";
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(die)");
    for rebinding in [
        "mod std {\n    pub mod process {\n        pub fn abort() {}\n    }\n}\n",
        "use other_crate as std;\n",
        "extern crate other_crate as std;\n",
    ] {
        let extra = format!("{abort_die}{rebinding}");
        assert_shadowed_handler_not_credited(&extra, &forwarder, UNWRAP_TESTS)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_process_exit_handler_with_a_std_bound_by_a_use_is_not_credited() -> Result<(), String>
{
    let abort_die = "fn die<T>(_: Overflow) -> T {\n    std::process::abort()\n}\n";
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(die)");
    for rebinding in [
        "use ext_dep::std;\n",
        "use ext_dep::{other, std};\n",
        "pub use ext_dep::inner::std;\n",
        "use ext_dep::{inner::{std, more}, other};\n",
        "use crate::std;\n",
        "use ::ext_dep::std;\n",
    ] {
        let extra = format!("{abort_die}{rebinding}");
        assert_shadowed_handler_not_credited(&extra, &forwarder, UNWRAP_TESTS)?;
    }
    Ok(())
}

#[test]
fn unwrap_tail_self_qualified_handler_on_a_generic_or_repeated_type_is_not_credited()
-> Result<(), String> {
    let pool_die = POOL_DIE;
    let forwarder = unwrapping_forwarder("self.try_grow(wanted).unwrap_or_else(Self::die)");
    // The control: `Self::die` on the one real `Pool` is credited.
    let finding = forwarder_finding(pool_die, &forwarder, UNWRAP_TESTS)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    // A second workspace type named `Pool`.
    let twin = format!("{pool_die}mod alt {{\n    pub struct Pool;\n}}\n");
    assert_shadowed_handler_not_credited(&twin, &forwarder, UNWRAP_TESTS)?;
    Ok(())
}

#[test]
fn unwrap_tail_on_a_result_or_option_the_file_imports_is_not_credited() -> Result<(), String> {
    let tail = "self.try_grow(wanted).unwrap_or_else(die)";
    // The control: no import rebinds the name, so the std method is credited.
    let finding = forwarder_finding(DIVERGING_DIE, &unwrapping_forwarder(tail), UNWRAP_TESTS)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    for import in [
        "use dep::Result;\n",
        "use dep::{Overflow2, Result};\n",
        "use dep::Res as Result;\n",
        "use dep::*;\n",
        "use dep::prelude::*;\n",
        "use std::fmt::Result;\n",
        "use std::io::*;\n",
        "pub use dep::Result;\n",
    ] {
        assert_unwrap_tail_not_credited(&format!("{DIVERGING_DIE}{import}"), tail)?;
    }
    // The std spellings of the import keep the credit.
    for import in ["use std::result::Result;\n", "use core::result::Result;\n"] {
        let finding = forwarder_finding(
            &format!("{DIVERGING_DIE}{import}"),
            &unwrapping_forwarder(tail),
            UNWRAP_TESTS,
        )?;
        assert_eq!(finding["classification"], "exposed", "{import}: {finding}");
    }
    Ok(())
}

#[test]
fn unwrap_tail_handler_reached_through_a_local_alias_is_not_credited() -> Result<(), String> {
    // `halt` is not a workspace function, so it is not a known handler even
    // though it is bound to the unique diverging `die`.
    for (extra, tail) in [
        (
            format!("{DIVERGING_DIE}use crate::die as halt;\n"),
            "self.try_grow(wanted).unwrap_or_else(halt)",
        ),
        (
            DIVERGING_DIE.to_string(),
            "let halt = die;\n        self.try_grow(wanted).unwrap_or_else(halt)",
        ),
        (
            DIVERGING_DIE.to_string(),
            "const halt: fn(Overflow) -> bool = die;\n        self.try_grow(wanted).unwrap_or_else(halt)",
        ),
    ] {
        assert_unwrap_tail_not_credited(&extra, tail)?;
    }
    Ok(())
}
