//! Frozen native baseline identities prevent silent omission during consolidation.
//! #14500 explicitly replaces the temporary main-writer hold with six current
//! cache-lifecycle controls; the eight historical identities remain accounted for.

use serde::Deserialize;
use std::collections::BTreeSet;
use std::process::Command;

#[derive(Clone, Deserialize)]
struct Case {
    old_binary: String,
    old_test: String,
    new_binary: String,
    new_test: String,
    ignored: bool,
    linux_only: bool,
}

#[derive(Deserialize)]
struct Mapping {
    baseline_source: String,
    cases: Vec<Case>,
}

const CONTROLS: &[&str] = &[
    "workflow_contracts::inventory_contract::complete_native_inventory_preserves_every_baseline_case",
    "workflow_contracts::inventory_contract::duplicate_mapping_is_refused",
    "workflow_contracts::inventory_contract::missing_mapping_is_refused",
    "workflow_contracts::inventory_contract::missing_native_case_is_refused",
    "workflow_contracts::inventory_contract::changed_ignore_state_is_refused",
    "workflow_contracts::inventory_contract::cache_migration_requires_every_active_control",
];

const RETIRED_CACHE: &[&str] = &[
    "admission_scope_cannot_remove_the_main_push_cache_writer",
    "broadening_saves_is_not_a_trusted_default_scope_writer",
    "current_source_preserves_a_main_scope_writer_opportunity",
    "disabled_hosted_job_or_filtered_push_is_not_writer_continuity",
    "disabled_producer_or_additional_cache_is_not_writer_continuity",
    "missing_caller_dependencies_cannot_supply_guard_outputs",
    "missing_or_rebound_producer_is_not_a_writer_opportunity",
    "retiring_main_push_is_not_cache_continuity",
];
const CURRENT_CACHE: &[&str] = &[
    "current_source_restores_opportunistically_and_recomputes_required_proof",
    "ordinary_warmer_cannot_return",
    "neither_gate_nor_shard_can_broaden_write_authority",
    "missing_changed_or_duplicate_cache_domains_refuse",
    "hit_or_miss_never_substitutes_for_compilation_or_tests",
    "absent_proof_or_rebound_caller_is_not_cache_continuity",
];

fn mapping() -> Result<Mapping, String> {
    serde_json::from_str(include_str!("fixtures/inventory-map.json")).map_err(|e| e.to_string())
}

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
    // Libtest's ordinary terse listing does not label ignored tests. Query
    // the ignored subset separately instead of treating every listed case as active.
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

fn validate(cases: &[Case], actual: &BTreeSet<(String, bool)>) -> Result<(), String> {
    let mut old = BTreeSet::new();
    let mut expected = BTreeSet::new();
    let mut retired = BTreeSet::new();
    for case in cases {
        if !old.insert((&case.old_binary, &case.old_test)) {
            return Err("duplicate baseline case mapping".into());
        }
        if case.new_binary != "workflow_contracts"
            || case.new_test
                != format!("workflow_contracts::{}::{}", case.old_binary, case.old_test)
        {
            return Err("unexpected binary or logical-name mapping".into());
        }
        if case.old_binary == "hosted_cache_continuity" {
            if case.ignored || case.linux_only || !RETIRED_CACHE.contains(&case.old_test.as_str()) {
                return Err("unreviewed historical cache migration".into());
            }
            retired.insert(case.old_test.as_str());
            continue;
        }
        if (!case.linux_only || cfg!(target_os = "linux"))
            && !expected.insert((case.new_test.clone(), case.ignored))
        {
            return Err("duplicate candidate case mapping".into());
        }
    }
    if retired != RETIRED_CACHE.iter().copied().collect() {
        return Err("missing historical cache migration identity".into());
    }
    expected.extend(CURRENT_CACHE.iter().map(|name| {
        (
            format!("workflow_contracts::hosted_cache_continuity::{name}"),
            false,
        )
    }));
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
    let mapping = mapping()?;
    if mapping.baseline_source != "46b62971d1ea2e06d916c09e35212c10fe4c7835" {
        return Err("unexpected baseline source".into());
    }
    validate(&mapping.cases, &native_inventory()?)
}

#[test]
fn duplicate_mapping_is_refused() -> Result<(), String> {
    let mut cases = mapping()?.cases;
    let duplicate = cases.first().ok_or("empty baseline")?.clone();
    cases.push(duplicate);
    assert!(validate(&cases, &native_inventory()?).is_err());
    Ok(())
}

#[test]
fn missing_mapping_is_refused() -> Result<(), String> {
    let mut cases = mapping()?.cases;
    let index = cases
        .iter()
        .position(|case| !case.linux_only)
        .ok_or("no portable baseline case")?;
    cases.remove(index);
    assert!(validate(&cases, &native_inventory()?).is_err());
    Ok(())
}

#[test]
fn missing_native_case_is_refused() -> Result<(), String> {
    let cases = mapping()?.cases;
    let first = cases
        .iter()
        .find(|case| !case.linux_only && case.old_binary != "hosted_cache_continuity")
        .ok_or("no portable baseline case")?;
    let mut actual = native_inventory()?;
    assert!(actual.remove(&(first.new_test.clone(), first.ignored)));
    assert!(validate(&cases, &actual).is_err());
    Ok(())
}

#[test]
fn changed_ignore_state_is_refused() -> Result<(), String> {
    let cases = mapping()?.cases;
    let first = cases
        .iter()
        .find(|case| !case.linux_only && case.old_binary != "hosted_cache_continuity")
        .ok_or("no portable baseline case")?;
    let mut actual = native_inventory()?;
    assert!(actual.remove(&(first.new_test.clone(), first.ignored)));
    actual.insert((first.new_test.clone(), !first.ignored));
    assert!(validate(&cases, &actual).is_err());
    Ok(())
}

#[test]
fn cache_migration_requires_every_active_control() -> Result<(), String> {
    let cases = mapping()?.cases;
    let actual = native_inventory()?;
    for name in CURRENT_CACHE {
        let name = format!("workflow_contracts::hosted_cache_continuity::{name}");
        let mut changed = actual.clone();
        assert!(changed.remove(&(name.clone(), false)));
        assert!(validate(&cases, &changed).is_err());
        changed.insert((name, true));
        assert!(validate(&cases, &changed).is_err());
    }
    Ok(())
}
