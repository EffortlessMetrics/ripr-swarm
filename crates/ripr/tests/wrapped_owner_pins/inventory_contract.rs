//! Frozen native identities from the three pre-consolidation executables.
//! Source: 2c03ae6671e375a2f16d3ee30c581f09f596ea88, default features,
//! Rust 1.95.0, Linux. Original modules have no platform/feature/ignore gates.

use std::collections::BTreeSet;
use std::process::Command;

// Each row maps ripr::<old binary>::<old test> to
// ripr::wrapped_owner_pins::<old binary>::<old test>, preserving ignored state.
const CASES: &[(&str, &str, bool)] = &[
    (
        "claims_payload_eq_owner_pin",
        "a_literal_matches_pin_neither_owner_version_returns_does_not_credit",
        false,
    ),
    (
        "claims_payload_eq_owner_pin",
        "a_pinned_literal_neither_owner_version_returns_does_not_credit",
        false,
    ),
    (
        "claims_payload_eq_owner_pin",
        "claims_payload_equality_credits_only_on_a_verified_claims_import",
        false,
    ),
    (
        "claims_payload_eq_owner_pin",
        "claims_payload_equality_does_not_credit_clone_or_err_variant_owners",
        false,
    ),
    (
        "claims_payload_eq_owner_pin",
        "claims_payload_equality_grades_only_the_return_value_family",
        false,
    ),
    (
        "claims_payload_eq_owner_pin",
        "claims_payload_equality_pins_a_whole_value_field",
        false,
    ),
    (
        "claims_payload_eq_owner_pin",
        "the_eq_macro_import_must_be_in_scope_of_the_test",
        false,
    ),
    (
        "claims_payload_owner_pin",
        "claims_payload_pins_credit_only_on_a_verified_claims_import",
        false,
    ),
    (
        "unwrapped_ok_owner_pin",
        "unwrapped_ok_pins_credit_only_when_the_value_stays_the_owners",
        false,
    ),
];

const CONTROLS: &[&str] = &[
    "inventory_contract::complete_native_inventory_preserves_every_baseline_case",
    "inventory_contract::duplicate_mapping_is_refused",
    "inventory_contract::missing_mapping_is_refused",
    "inventory_contract::missing_native_case_is_refused",
    "inventory_contract::changed_ignore_state_is_refused",
];

fn native_listing(arguments: &[&str]) -> Result<BTreeSet<String>, String> {
    let output = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(arguments)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("native inventory failed: {:?}", output.status));
    }
    let text = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let Some((name, state)) = line.rsplit_once(": ") else {
            continue;
        };
        if state != "test" {
            return Err(format!("unsupported native inventory row: {line}"));
        }
        if !names.insert(name.to_string()) {
            return Err(format!("duplicate native case: {name}"));
        }
    }
    Ok(names)
}

fn native_inventory() -> Result<BTreeSet<(String, bool)>, String> {
    let all = native_listing(&["--list", "--format=terse"])?;
    // An ordinary libtest listing also includes ignored tests without marking
    // them. The separate ignored-only listing is necessary to conserve state.
    let ignored = native_listing(&["--list", "--ignored", "--format=terse"])?;
    if !ignored.is_subset(&all) {
        return Err("ignored native cases absent from the complete inventory".into());
    }
    Ok(all
        .into_iter()
        .map(|name| {
            let is_ignored = ignored.contains(&name);
            (name, is_ignored)
        })
        .collect())
}

fn validate(cases: &[(&str, &str, bool)], actual: &BTreeSet<(String, bool)>) -> Result<(), String> {
    let mut old = BTreeSet::new();
    let mut expected = BTreeSet::new();
    for &(binary, name, ignored) in cases {
        if !old.insert((binary, name)) {
            return Err("duplicate baseline case mapping".into());
        }
        if !expected.insert((format!("wrapped_owner_pins::{binary}::{name}"), ignored)) {
            return Err("duplicate candidate case mapping".into());
        }
    }
    expected.extend(CONTROLS.iter().map(|name| ((*name).to_string(), false)));
    if &expected != actual {
        return Err(format!(
            "native inventory mismatch: missing {:?}; extra {:?}",
            expected.difference(actual).collect::<Vec<_>>(),
            actual.difference(&expected).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

#[test]
fn complete_native_inventory_preserves_every_baseline_case() -> Result<(), String> {
    validate(CASES, &native_inventory()?)
}

#[test]
fn duplicate_mapping_is_refused() -> Result<(), String> {
    let mut cases = CASES.to_vec();
    cases.push(*cases.first().ok_or("empty baseline")?);
    assert!(validate(&cases, &native_inventory()?).is_err());
    Ok(())
}

#[test]
fn missing_mapping_is_refused() -> Result<(), String> {
    let mut cases = CASES.to_vec();
    cases.pop().ok_or("empty baseline")?;
    assert!(validate(&cases, &native_inventory()?).is_err());
    Ok(())
}

#[test]
fn missing_native_case_is_refused() -> Result<(), String> {
    let &(binary, name, ignored) = CASES.first().ok_or("empty baseline")?;
    let mut actual = native_inventory()?;
    assert!(actual.remove(&(format!("wrapped_owner_pins::{binary}::{name}"), ignored)));
    assert!(validate(CASES, &actual).is_err());
    Ok(())
}

#[test]
fn changed_ignore_state_is_refused() -> Result<(), String> {
    let &(binary, name, ignored) = CASES.first().ok_or("empty baseline")?;
    let mut actual = native_inventory()?;
    let name = format!("wrapped_owner_pins::{binary}::{name}");
    assert!(actual.remove(&(name.clone(), ignored)));
    actual.insert((name, !ignored));
    assert!(validate(CASES, &actual).is_err());
    Ok(())
}
