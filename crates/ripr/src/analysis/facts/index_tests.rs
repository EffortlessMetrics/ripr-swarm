use super::*;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};

fn large_file() -> Result<FileFacts, String> {
    let body = (0..256)
        .map(|n| {
            format!(
                "let value_{n} = value(\"payload_{n:04}_{}\"); assert_eq!(value_{n}, {n});\n",
                "x".repeat(128)
            )
        })
        .collect::<String>();
    RaRustSyntaxAdapter.summarize_file(
        Path::new("tests/large.rs"),
        &format!(
            "fn value(input: &str) -> usize {{ input.len() }}\n#[test]\nfn large() {{\n{body}}}\n"
        ),
    )
}

#[test]
fn moved_large_payload_has_one_backing_through_both_ordered_views() -> Result<(), String> {
    let facts = large_file()?;
    assert!(facts.tests[0].body.len() > 32_768);
    assert!(facts.tests[0].calls.len() >= 256);
    assert!(facts.tests[0].assertions.len() >= 256);
    assert!(facts.tests[0].let_bindings.len() >= 256);
    let body = facts.tests[0].body.as_ptr();
    let calls = facts.tests[0].calls.as_ptr();
    let assertions = facts.tests[0].assertions.as_ptr();
    let mut index = RustIndex::default();
    let path = PathBuf::from("tests/large.rs");
    index.insert_file(path.clone(), facts, true);
    index.finalize()?;
    let file = index.files().get(&path).ok_or("missing large file")?;
    assert_eq!(index.function_facts.len(), index.functions().len());
    assert_eq!(index.test_facts.len(), index.tests().len());
    for (flat, local) in index.functions().iter().zip(file.functions.iter()) {
        assert!(std::ptr::eq(flat, local));
        assert_eq!(flat.body.as_ptr(), local.body.as_ptr());
        assert_eq!(flat.calls.as_ptr(), local.calls.as_ptr());
    }
    assert!(std::ptr::eq(index.tests().at(0), &file.tests[0]));
    assert_eq!(index.tests()[0].body.as_ptr(), body);
    assert_eq!(index.tests()[0].calls.as_ptr(), calls);
    assert_eq!(index.tests()[0].assertions.as_ptr(), assertions);
    Ok(())
}

#[test]
fn union_mutation_visits_shared_flat_only_and_file_only_once() -> Result<(), String> {
    let facts = large_file()?;
    let mut index = RustIndex::default();
    index.push_test(facts.tests[0].clone());
    index.insert_file(PathBuf::from("file-only.rs"), facts.clone(), false);
    index.insert_file(PathBuf::from("shared.rs"), facts, true);
    let count = index.test_facts.len();
    assert_eq!(count, 3);
    let mut visits = 0;
    index.for_each_test_mut(|test| {
        visits += 1;
        test.body = format!("{}// once", test.body.as_str()).into();
    });
    assert_eq!(visits, count);
    for test in index.test_facts.iter() {
        assert_eq!(test.body.matches("// once").count(), 1);
    }
    let shared = index
        .files()
        .get(Path::new("shared.rs"))
        .ok_or("missing shared file")?;
    assert!(std::ptr::eq(index.tests().at(1), &shared.tests[0]));
    Ok(())
}

#[test]
fn remove_file_purges_flat_and_file_memberships() -> Result<(), String> {
    let facts = large_file()?;
    let mut index = RustIndex::default();
    index.insert_file(PathBuf::from("shared.rs"), facts.clone(), true);
    index.insert_file(PathBuf::from("local.rs"), facts.clone(), false);
    index.non_utf8_sources.insert(PathBuf::from("shared.rs"));
    assert!(index.remove_file(Path::new("shared.rs")));
    assert!(!index.remove_file(Path::new("shared.rs")));
    assert_eq!(index.tests().len(), 0);
    assert_eq!(index.functions().len(), 0);
    assert!(!index.non_utf8_sources.contains(Path::new("shared.rs")));
    index.finalize()?;
    assert_eq!(index.tests().len(), 0);
    assert_eq!(index.files().len(), 1);
    Ok(())
}

#[test]
fn compaction_keeps_independent_membership_and_duplicate_occurrences() -> Result<(), String> {
    let facts = large_file()?;
    let mut index = RustIndex::default();
    // One flat-only occurrence plus two file-only occurrences of the same
    // payload under one path (the second insert replaces the first file
    // membership): compaction keeps the flat occurrence and the surviving
    // file occurrence as independent records.
    index.push_test(facts.tests[0].clone());
    index.insert_file(PathBuf::from("local.rs"), facts.clone(), false);
    index.insert_file(PathBuf::from("local.rs"), facts, false);
    assert_eq!(index.test_facts.len(), 3);
    index.finalize()?;
    assert_eq!(index.tests().len(), 1);
    assert_eq!(index.test_facts.len(), 2);
    let local = index
        .files()
        .get(Path::new("local.rs"))
        .ok_or("missing local file")?;
    assert_eq!(index.tests().at(0), &local.tests[0]);
    assert!(!std::ptr::eq(index.tests().at(0), &local.tests[0]));
    assert_eq!(index.tests()[0].file, Path::new("tests/large.rs"));
    Ok(())
}

#[test]
fn whole_index_wire_stays_expanded_and_cloning_does_not_pin_a_generation() -> Result<(), String> {
    let facts = large_file()?;
    let mut index = RustIndex::default();
    let path = PathBuf::from("tests/large.rs");
    index.insert_file(path.clone(), facts.clone(), true);
    let expected = OwnedRustIndex {
        functions: facts.functions.clone(),
        tests: facts.tests.clone(),
        files: BTreeMap::from([(path.clone(), facts)]),
        ..OwnedRustIndex::default()
    };
    let wire = serde_json::to_value(&index).map_err(|e| e.to_string())?;
    assert_eq!(
        wire,
        serde_json::to_value(&expected).map_err(|e| e.to_string())?
    );
    let decoded: RustIndex = serde_json::from_value(wire.clone()).map_err(|e| e.to_string())?;
    assert_eq!(
        serde_json::to_value(&decoded).map_err(|e| e.to_string())?,
        wire
    );
    let cloned = index.clone();
    assert!(!std::ptr::eq(index.tests().at(0), &cloned.tests()[0]));
    // #5415 step 2: bodies share the file allocation, so a clone reuses the
    // bytes instead of duplicating them (this used to assert the opposite).
    assert_eq!(
        index.tests()[0].body.as_ptr(),
        cloned.tests()[0].body.as_ptr()
    );
    drop(index);
    let local = cloned.files().get(&path).ok_or("missing cloned file")?;
    assert!(std::ptr::eq(&cloned.tests()[0], &local.tests[0]));
    Ok(())
}

#[test]
fn invalid_membership_fails_finalization_before_compaction() -> Result<(), String> {
    let mut index = RustIndex::default();
    index.insert_file(PathBuf::from("large.rs"), large_file()?, true);
    let original_count = index.function_facts.len();
    index.function_order.push(FactId::new(original_count));
    let error = index
        .finalize()
        .err()
        .ok_or("invalid membership was accepted")?;
    assert!(error.starts_with("invalid function fact membership:"));
    assert_eq!(index.function_facts.len(), original_count);
    Ok(())
}

#[test]
fn finalization_observes_deadline_at_return_after_compaction_and_membership_refresh()
-> Result<(), String> {
    use crate::analysis::cancellation::{AnalysisCancellationToken, with_token};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    let path = PathBuf::from("tests/surviving.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, "#[test]\nfn surviving() {}\n")?;
    assert_eq!(facts.functions.len(), 1);
    assert_eq!(facts.tests.len(), 1);
    let mut candidate = RustIndex::default();
    candidate.insert_file(PathBuf::from("discard.rs"), facts.clone(), false);
    candidate.insert_file(path.clone(), facts, true);
    assert!(candidate.remove_file(Path::new("discard.rs")));
    assert_eq!(candidate.function_facts.len(), 2);
    assert_eq!(candidate.test_facts.len(), 2);
    assert_eq!(candidate.functions().len(), 1);
    assert_eq!(candidate.tests().len(), 1);

    let calls = Arc::new(AtomicUsize::new(0));
    let clock_calls = Arc::clone(&calls);
    let started = Instant::now();
    let token = AnalysisCancellationToken::with_budget(
        started,
        Duration::from_millis(1),
        Arc::new(move || {
            if clock_calls.fetch_add(1, Ordering::SeqCst) < 11 {
                started
            } else {
                started + Duration::from_millis(1)
            }
        }),
    );
    // This fixed nonempty shape reaches entry (1), validation (2-3),
    // pre-compaction (4), both compacted arenas (5-6), refresh entry (7),
    // validation (8-9), reverse memberships (10-11), and return (12).
    // Do not calibrate this threshold from a successful run: removing the
    // return checkpoint must turn this result into Ok and fail the oracle.
    let result = with_token(&token, || candidate.finalize());
    assert_eq!(
        result.err().as_deref(),
        Some("analysis cancelled: DeadlineExceeded")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 12);
    assert_eq!(candidate.function_facts.len(), 1);
    assert_eq!(candidate.test_facts.len(), 1);
    assert_eq!(
        candidate.function_slot(candidate.functions().at(0)),
        Some(0)
    );
    assert_eq!(candidate.test_slot(candidate.tests().at(0)), Some(0));
    let local = candidate
        .files()
        .get(&path)
        .ok_or("missing surviving file")?;
    assert!(std::ptr::eq(
        candidate.functions().at(0),
        local.functions.at(0)
    ));
    assert!(std::ptr::eq(candidate.tests().at(0), local.tests.at(0)));
    Ok(())
}

#[test]
fn shared_harness_demotion_splits_only_disagreeing_containing_paths() -> Result<(), String> {
    let target = Path::new("tests/container.rs");
    for fact_path in [Path::new("src/claimed.rs"), target] {
        let facts = RaRustSyntaxAdapter.summarize_file(fact_path, "#[test]\nfn subject() {}\n")?;
        assert_eq!(facts.functions.len(), 1);
        assert_eq!(facts.tests.len(), 1);
        assert_eq!(facts.functions[0].file, fact_path);
        assert_eq!(facts.tests[0].file, fact_path);
        assert_eq!(
            facts.functions[0].source_role,
            FunctionSourceRole::TestAttribute
        );
        let mut index = RustIndex::default();
        index.insert_file(target.to_path_buf(), facts, true);
        let local = index.files().get(target).ok_or("missing harness target")?;
        assert!(std::ptr::eq(index.functions().at(0), local.functions.at(0)));
        assert!(std::ptr::eq(index.tests().at(0), local.tests.at(0)));
        assert_eq!(index.function_facts.len(), 1);
        assert_eq!(index.test_facts.len(), 1);

        super::super::harness_registry::demote_harness_target_functions(&mut index, target);
        index.finalize()?;
        let local = index.files().get(target).ok_or("missing demoted target")?;
        assert_eq!(local.functions.len(), 1);
        assert_eq!(index.functions().len(), 1);
        assert_eq!(
            local.functions[0].source_role,
            FunctionSourceRole::HarnessHelper
        );
        assert!(local.tests.is_empty());
        if fact_path == target {
            // Aligned selection must mutate the shared occurrence in place,
            // rather than unconditionally clone for the two ordered views.
            assert_eq!(
                index.functions()[0].source_role,
                FunctionSourceRole::HarnessHelper
            );
            assert!(std::ptr::eq(index.functions().at(0), local.functions.at(0)));
            assert_eq!(index.function_facts.len(), 1);
            assert!(index.tests().is_empty());
            assert!(index.test_facts.is_empty());
        } else {
            // Containing-file selection demotes only the local view. The flat
            // view still follows the fact's own declared path and role.
            assert_eq!(
                index.functions()[0].source_role,
                FunctionSourceRole::TestAttribute
            );
            assert!(!std::ptr::eq(
                index.functions().at(0),
                local.functions.at(0)
            ));
            assert_eq!(index.function_facts.len(), 2);
            assert_eq!(index.tests().len(), 1);
            assert_eq!(index.test_facts.len(), 1);
            assert_eq!(index.tests()[0].file, fact_path);
        }
    }
    Ok(())
}

#[test]
fn flat_slot_identity_survives_reordering_compaction_and_repeated_membership() -> Result<(), String>
{
    let facts = large_file()?;
    let mut index = RustIndex::default();
    index.insert_file(PathBuf::from("discard.rs"), facts.clone(), false);
    index.push_test(facts.tests[0].clone());
    index.push_function(facts.functions[0].clone());
    index.insert_file(PathBuf::from("local.rs"), facts.clone(), false);
    index.insert_file(PathBuf::from("shared.rs"), facts, true);
    let detached = index.tests()[0].clone();
    assert_eq!(index.test_slot(&detached), None);
    let local = index
        .files()
        .get(Path::new("local.rs"))
        .ok_or("missing local file")?;
    assert_eq!(index.test_slot(local.tests.at(0)), None);
    assert_eq!(index.function_slot(local.functions.at(0)), None);
    let duplicate = index.test_order[0];
    index.test_order.push(duplicate);
    index.test_order.reverse();
    index.function_order.reverse();
    index.refresh_memberships()?;
    for (position, test) in index.tests().iter().enumerate() {
        let expected = index
            .tests()
            .iter()
            .position(|candidate| std::ptr::eq(candidate, test));
        assert_eq!(index.test_slot(test), expected, "test position {position}");
    }
    index.remove_file(Path::new("discard.rs"));
    index.finalize()?;
    for (position, function) in index.functions().iter().enumerate() {
        assert_eq!(index.function_slot(function), Some(position));
    }
    for test in index.tests() {
        let expected = index
            .tests()
            .iter()
            .position(|candidate| std::ptr::eq(candidate, test));
        assert_eq!(index.test_slot(test), expected);
    }
    let other = index.clone();
    assert_ne!(index.storage_identity(), other.storage_identity());
    assert_eq!(index.test_slot(other.tests().at(0)), None);
    assert_eq!(index.function_slot(other.functions().at(0)), None);
    Ok(())
}

#[test]
fn fact_view_equality_compares_values_and_order_across_storage_layouts() {
    let dense = [10, 20, 10];
    let arena = [20, 10, 99, 10];
    let order = [FactId::new(1), FactId::new(0), FactId::new(3)];
    let indexed = FactSlice {
        arena: &arena,
        order: Some(&order),
    };
    assert_eq!(FactSlice::from_slice(&dense), indexed);
    assert_eq!(format!("{indexed:?}"), "[10, 20, 10]");
    assert_ne!(FactSlice::from_slice(&[20, 10, 10]), indexed);
    assert_ne!(FactSlice::from_slice(&[10, 20]), indexed);
    assert_ne!(FactSlice::from_slice(&[10, 20, 99]), indexed);
}

#[test]
fn file_view_equality_compares_complete_metadata_and_local_membership() -> Result<(), String> {
    let facts = large_file()?;
    let path = PathBuf::from("tests/large.rs");
    let mut first = RustIndex::default();
    first.insert_file(path.clone(), facts.clone(), true);
    let mut second = RustIndex::default();
    // An unrelated flat-only occurrence shifts the arena offsets, but is not
    // part of the file map's semantic value.
    second.push_function(facts.functions[0].clone());
    second.insert_file(path.clone(), facts, false);
    assert_eq!(first.files(), second.files());
    assert_eq!(first.files().get(&path), second.files().get(&path));
    assert_ne!(first.functions(), second.functions());
    let provenance = second
        .files
        .get(&path)
        .ok_or("missing second file")?
        .role_provenance
        .clone();
    second
        .files
        .get_mut(&path)
        .ok_or("missing second file")?
        .role_provenance
        .earliest_unresolved_reason = Some("unresolved control".into());
    assert_ne!(first.files(), second.files());
    second
        .files
        .get_mut(&path)
        .ok_or("missing second file")?
        .role_provenance = provenance;
    assert_eq!(first.files(), second.files());
    second
        .files
        .get_mut(&path)
        .ok_or("missing second file")?
        .functions
        .reverse();
    assert_ne!(first.files(), second.files());
    Ok(())
}

fn membership_fixture(path: &Path) -> Result<FileFacts, String> {
    let mut source = String::from("fn identity(n: usize) -> usize { n }\n");
    for ordinal in 0..48 {
        source.push_str(&format!(
            "fn owner_{ordinal:02}(n: usize) -> usize {{ let marker = \"{}\"; let value = identity(n); if marker.len() > 7 {{ value + {ordinal} }} else {{ value }} }}\n",
            "material_payload".repeat(8),
        ));
    }
    source.push_str("#[test] fn checks_owner() { assert_eq!(owner_00(2), 2); }\n");
    let facts = RaRustSyntaxAdapter.summarize_file(path, &source)?;
    assert_eq!(facts.functions.len(), 50);
    assert_eq!(facts.tests.len(), 1);
    assert!(
        facts
            .functions
            .iter()
            .all(|function| !function.body.is_empty())
    );
    assert!(
        facts
            .functions
            .iter()
            .any(|function| !function.calls.is_empty())
    );
    Ok(facts)
}

#[test]
fn file_function_membership_retains_only_handle_capacity() -> Result<(), String> {
    let path = PathBuf::from("src/membership.rs");
    let facts = membership_fixture(&path)?;
    let count = facts.functions.len();
    let source_pointer = facts.functions.as_ptr() as usize;
    let source_capacity = facts.functions.capacity();
    let source_record_size = std::mem::size_of::<FunctionFact>();
    let handle_size = std::mem::size_of::<FactId<FunctionFact>>();
    assert!(source_record_size > handle_size);
    assert!(source_capacity >= count);
    let payload_pointers = facts
        .functions
        .iter()
        .map(|fact| (fact.body.as_ptr(), fact.calls.as_ptr()))
        .collect::<Vec<_>>();
    let expected = OwnedRustIndex {
        functions: facts.functions.clone(),
        tests: facts.tests.clone(),
        files: BTreeMap::from([(path.clone(), facts.clone())]),
        ..OwnedRustIndex::default()
    };
    let expected_wire = serde_json::to_value(&expected).map_err(|error| error.to_string())?;
    let mut index = RustIndex::default();
    index.insert_file(path.clone(), facts, true);
    let local = index.files.get(&path).ok_or("missing membership file")?;
    let destination_pointer = local.functions.as_ptr() as usize;
    let destination_capacity = local.functions.capacity();
    assert_eq!(
        serde_json::to_value(&index).map_err(|error| error.to_string())?,
        expected_wire
    );
    let file = index.files().get(&path).ok_or("missing borrowed file")?;
    assert_eq!(file.functions.len(), count);
    assert_eq!(index.functions().len(), count);
    for ((flat, local), (body, calls)) in index
        .functions()
        .iter()
        .zip(file.functions.iter())
        .zip(payload_pointers)
    {
        assert!(std::ptr::eq(flat, local));
        assert_eq!(flat.body.as_ptr(), body);
        assert_eq!(flat.calls.as_ptr(), calls);
    }
    // The capacity bound is the regression oracle. Pointer inequality is
    // diagnostic only: allocator address reuse must not decide correctness.
    assert!(
        destination_capacity <= count,
        "function membership retained fact-sized backing: {destination_capacity} handles for {count} functions; source allocation reused: {}",
        source_pointer == destination_pointer,
    );
    index.finalize()?;
    let wire = serde_json::to_value(&index).map_err(|error| error.to_string())?;
    assert_eq!(wire, expected_wire);
    let decoded: RustIndex =
        serde_json::from_value(wire.clone()).map_err(|error| error.to_string())?;
    assert_eq!(
        serde_json::to_value(&decoded).map_err(|error| error.to_string())?,
        wire
    );
    Ok(())
}

#[test]
fn compact_membership_preserves_occurrences_replacement_and_finalized_slots() -> Result<(), String>
{
    let a = PathBuf::from("src/a.rs");
    let b = PathBuf::from("src/b.rs");
    let discarded = PathBuf::from("src/discarded.rs");
    let mut first = membership_fixture(&a)?;
    // Equal complete facts remain distinct occurrences; equal keys do not alias.
    first.functions.push(first.functions[2].clone());
    let second = membership_fixture(&b)?;
    let replacement = first.clone();
    let mut expected_functions = first.functions.clone();
    expected_functions.extend(second.functions.clone());
    let mut expected_tests = first.tests.clone();
    expected_tests.extend(second.tests.clone());
    let expected = OwnedRustIndex {
        files: BTreeMap::from([
            (a.clone(), replacement.clone()),
            (b.clone(), second.clone()),
        ]),
        functions: expected_functions,
        tests: expected_tests,
        ..OwnedRustIndex::default()
    };
    let expected_wire = serde_json::to_value(&expected).map_err(|error| error.to_string())?;

    let mut index = RustIndex::default();
    index.insert_file(a.clone(), first, true);
    index.insert_file(b.clone(), second, true);
    index.insert_file(discarded.clone(), membership_fixture(&discarded)?, false);
    index.insert_file(a.clone(), replacement, false);
    assert!(index.remove_file(&discarded));
    index.finalize()?;
    assert_eq!(
        serde_json::to_value(&index).map_err(|error| error.to_string())?,
        expected_wire
    );
    assert_eq!(index.function_facts.len(), 152);
    assert_eq!(index.test_facts.len(), 3);
    assert_eq!(index.functions().len(), 101);
    let local_a = index.files().get(&a).ok_or("replacement a")?;
    let local_b = index.files().get(&b).ok_or("retained b")?;
    assert_eq!(local_a.functions.len(), 51);
    assert_eq!(local_b.functions.len(), 50);
    assert!(!std::ptr::eq(
        index.functions().at(0),
        local_a.functions.at(0)
    ));
    assert!(std::ptr::eq(
        index.functions().at(51),
        local_b.functions.at(0)
    ));
    assert_eq!(local_a.functions.at(2), local_a.functions.at(50));
    assert!(!std::ptr::eq(
        local_a.functions.at(2),
        local_a.functions.at(50)
    ));
    for path in [&a, &b] {
        let containing = index.files.get(path).ok_or("containing membership")?;
        assert!(containing.functions.capacity() <= containing.functions.len());
    }
    for (position, function) in index.functions().iter().enumerate() {
        assert_eq!(index.function_slot(function), Some(position));
    }
    for function in local_a.functions {
        assert_eq!(index.function_slot(function), None);
    }
    assert_eq!(index.test_slot(local_a.tests.at(0)), None);
    for (position, test) in index.tests().iter().enumerate() {
        assert_eq!(index.test_slot(test), Some(position));
    }
    let decoded: RustIndex =
        serde_json::from_value(expected_wire.clone()).map_err(|error| error.to_string())?;
    assert_eq!(
        serde_json::to_value(&decoded).map_err(|error| error.to_string())?,
        expected_wire
    );
    Ok(())
}

#[test]
fn unresolved_property_macros_survive_file_view_wire_and_owned_round_trip() -> Result<(), String> {
    let path = PathBuf::from("tests/property.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(
        &path,
        "macro_rules! proptest { ($($t:tt)*) => {} }\nproptest! { #[test] fn phantom() { assert_eq!(gate(10), true); } }\n",
    )?;
    assert_eq!(facts.unresolved_property_macros.len(), 1);
    assert!(facts.tests.is_empty());
    let mut index = RustIndex::default();
    index.insert_file(path.clone(), facts.clone(), true);
    let view = index.files().get(&path).ok_or("missing property file")?;
    assert_eq!(
        view.unresolved_property_macros,
        facts.unresolved_property_macros
    );
    assert_eq!(
        serde_json::to_value(view).map_err(|e| e.to_string())?,
        serde_json::to_value(&facts).map_err(|e| e.to_string())?
    );
    assert_eq!(index.owned_file(&path).as_ref(), Some(&facts));
    let wire = serde_json::to_value(&index).map_err(|e| e.to_string())?;
    let decoded: RustIndex = serde_json::from_value(wire).map_err(|e| e.to_string())?;
    assert_eq!(decoded.owned_file(&path).as_ref(), Some(&facts));
    Ok(())
}

#[test]
fn finalization_observes_deadline_while_releasing_build_slack() -> Result<(), String> {
    use crate::analysis::cancellation::{AnalysisCancellationToken, with_token};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    let path = PathBuf::from("tests/surviving.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, "#[test]\nfn surviving() {}\n")?;
    let mut candidate = RustIndex::default();
    candidate.insert_file(path.clone(), facts, true);

    let calls = Arc::new(AtomicUsize::new(0));
    let clock_calls = Arc::clone(&calls);
    let started = Instant::now();
    let token = AnalysisCancellationToken::with_budget(
        started,
        Duration::from_millis(1),
        Arc::new(move || {
            // Stay inside the budget through the checkpoint that follows the
            // membership refresh, then expire at the first checkpoint of the
            // slack pass.
            if clock_calls.fetch_add(1, Ordering::SeqCst) < 12 {
                started
            } else {
                started + Duration::from_millis(1)
            }
        }),
    );
    let result = with_token(&token, || candidate.finalize());
    assert_eq!(
        result.err().as_deref(),
        Some("analysis cancelled: DeadlineExceeded")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 13);
    Ok(())
}

/// Unused capacity per shrunk field, keyed by a label naming the field, so a
/// missing shrink shows up on its own field rather than in a grand total.
fn spare_by_field(index: &RustIndex) -> std::collections::BTreeMap<&'static str, usize> {
    fn spare<T>(items: &Vec<T>) -> usize {
        items.capacity() - items.len()
    }
    fn spare_str(text: &String) -> usize {
        text.capacity() - text.len()
    }
    let mut spare_of = std::collections::BTreeMap::new();
    let mut note = |label: &'static str, slots: usize| {
        *spare_of.entry(label).or_insert(0) += slots;
    };
    note("function_order", spare(&index.function_order));
    note("test_order", spare(&index.test_order));
    note("function_positions", spare(&index.function_positions));
    note("test_positions", spare(&index.test_positions));
    note("function_records", spare(&index.function_facts.records));
    note("test_records", spare(&index.test_facts.records));
    for fact in index.function_facts.iter() {
        note("fn.name", spare_str(&fact.name));
        note("fn.calls", spare(&fact.calls));
        note("fn.returns", spare(&fact.returns));
        note("fn.literals", spare(&fact.literals));
        note("fn.attrs", spare(&fact.attrs));
        note("fn.impl_attrs", spare(&fact.impl_attrs));
        note("fn.nested_fn_names", spare(&fact.nested_fn_names));
        note("fn.let_bindings", spare(&fact.let_bindings));
        for call in &fact.calls {
            note("fn.call.name", spare_str(&call.name));
        }
        for ret in &fact.returns {
            note("fn.return.text", spare_str(&ret.text));
        }
        for literal in &fact.literals {
            note("fn.literal.value", spare_str(&literal.value));
        }
        for attr in &fact.attrs {
            note("fn.attr", spare_str(attr));
        }
        for attr in &fact.impl_attrs {
            note("fn.impl_attr", spare_str(attr));
        }
        for name in &fact.nested_fn_names {
            note("fn.nested_fn_name", spare_str(name));
        }
        for binding in &fact.let_bindings {
            note("fn.let_binding.name", spare_str(&binding.name));
        }
    }
    for fact in index.test_facts.iter() {
        note("test.name", spare_str(&fact.name));
        note("test.calls", spare(&fact.calls));
        note("test.assertions", spare(&fact.assertions));
        note("test.literals", spare(&fact.literals));
        note("test.attrs", spare(&fact.attrs));
        note("test.nested_fn_names", spare(&fact.nested_fn_names));
        note("test.let_bindings", spare(&fact.let_bindings));
        for call in &fact.calls {
            note("test.call.name", spare_str(&call.name));
        }
        for assertion in &fact.assertions {
            note("test.assertion.text", spare_str(&assertion.text));
            note(
                "test.assertion.observed_tokens",
                spare(&assertion.observed_tokens),
            );
            for token in &assertion.observed_tokens {
                note("test.assertion.observed_token", spare_str(token));
            }
        }
        for literal in &fact.literals {
            note("test.literal.value", spare_str(&literal.value));
        }
        for attr in &fact.attrs {
            note("test.attr", spare_str(attr));
        }
        for name in &fact.nested_fn_names {
            note("test.nested_fn_name", spare_str(name));
        }
        for binding in &fact.let_bindings {
            note("test.let_binding.name", spare_str(&binding.name));
        }
    }
    for file in index.files.values() {
        note("file.functions", spare(&file.functions));
        note("file.tests", spare(&file.tests));
        note("file.returns", spare(&file.data.returns));
        note("file.literals", spare(&file.data.literals));
        note("file.probe_shapes", spare(&file.data.probe_shapes));
        note(
            "file.module_declarations",
            spare(&file.data.module_declarations),
        );
        note(
            "file.unresolved_property_macros",
            spare(&file.data.unresolved_property_macros),
        );
        note("file.edges", spare(&file.data.role_provenance.edges));
        for ret in &file.data.returns {
            note("file.return.text", spare_str(&ret.text));
        }
        for literal in &file.data.literals {
            note("file.literal.value", spare_str(&literal.value));
        }
        for edge in &file.data.role_provenance.edges {
            note("file.edge.declaration", spare_str(&edge.declaration));
        }
    }
    spare_of
}

#[test]
fn finishing_an_index_releases_the_spare_capacity_the_builders_left() -> Result<(), String> {
    use crate::analysis::facts::model::{
        ReturnFact, SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind,
    };

    let mut facts = large_file()?;
    // Producers push facts one at a time and leave spare room on every
    // buffer. Populate every nested collection the pass shrinks (the parsed
    // functions carry no impl attributes, nested functions or provenance
    // edges), then reserve extra on each so a deleted shrink call leaves its
    // own field with the room it started with.
    let test = facts.tests.first_mut().ok_or("missing test")?;
    let binding = test
        .let_bindings
        .first()
        .cloned()
        .ok_or("missing binding")?;
    let literal = test.literals.first().cloned().ok_or("missing literal")?;
    let function = facts.functions.first_mut().ok_or("missing function")?;
    function.attrs.push("#[inline]".into());
    function.impl_attrs.push("#[cfg(test)]".into());
    function.nested_fn_names.push("inner".into());
    function.let_bindings.push(binding);
    function.literals.push(literal.clone());
    function.returns.push(ReturnFact {
        line: 1,
        text: "input.len()".into(),
    });
    function.name.reserve_exact(8);
    function.calls.reserve_exact(8);
    function.returns.reserve_exact(8);
    function.literals.reserve_exact(8);
    function.attrs.reserve_exact(8);
    function.impl_attrs.reserve_exact(8);
    function.nested_fn_names.reserve_exact(8);
    function.let_bindings.reserve_exact(8);
    for call in &mut function.calls {
        call.name.reserve_exact(8);
    }
    for ret in &mut function.returns {
        ret.text.reserve_exact(8);
    }
    for literal in &mut function.literals {
        literal.value.reserve_exact(8);
    }
    for attr in function.attrs.iter_mut().chain(&mut function.impl_attrs) {
        attr.reserve_exact(8);
    }
    for name in &mut function.nested_fn_names {
        name.reserve_exact(8);
    }
    for binding in &mut function.let_bindings {
        binding.name.reserve_exact(8);
    }
    let test = facts.tests.first_mut().ok_or("missing test")?;
    test.attrs.push("#[ignore]".into());
    test.nested_fn_names.push("helper".into());
    test.name.reserve_exact(8);
    test.calls.reserve_exact(8);
    test.assertions.reserve_exact(8);
    test.literals.reserve_exact(8);
    test.attrs.reserve_exact(8);
    test.nested_fn_names.reserve_exact(8);
    test.let_bindings.reserve_exact(8);
    for call in &mut test.calls {
        call.name.reserve_exact(8);
    }
    for assertion in &mut test.assertions {
        assertion.text.reserve_exact(8);
        assertion.observed_tokens.push("token".into());
        assertion.observed_tokens.reserve_exact(8);
        for token in &mut assertion.observed_tokens {
            token.reserve_exact(8);
        }
    }
    for literal in &mut test.literals {
        literal.value.reserve_exact(8);
    }
    for attr in &mut test.attrs {
        attr.reserve_exact(8);
    }
    for name in &mut test.nested_fn_names {
        name.reserve_exact(8);
    }
    for binding in &mut test.let_bindings {
        binding.name.reserve_exact(8);
    }
    facts.literals.push(literal);
    facts.returns.push(ReturnFact {
        line: 1,
        text: "input.len()".into(),
    });
    for ret in &mut facts.returns {
        ret.text.reserve_exact(8);
    }
    for literal in &mut facts.literals {
        literal.value.reserve_exact(8);
    }
    facts.returns.reserve_exact(8);
    facts.literals.reserve_exact(8);
    facts.probe_shapes.reserve_exact(8);
    facts.module_declarations.reserve_exact(8);
    facts.unresolved_property_macros.reserve_exact(8);
    facts.role_provenance.edges.push(SourceRoleProvenanceEdge {
        kind: SourceRoleProvenanceEdgeKind::Module,
        parent: PathBuf::from("src/lib.rs"),
        child: PathBuf::from("tests/large.rs"),
        declaration: "mod large;".into(),
        line: 1,
        requires_test: true,
    });
    facts.role_provenance.edges.reserve_exact(8);
    for edge in &mut facts.role_provenance.edges {
        edge.declaration.reserve_exact(8);
    }
    let mut index = RustIndex::default();
    let path = PathBuf::from("tests/large.rs");
    index.insert_file(path.clone(), facts, true);
    let file = index.files.get_mut(&path).ok_or("missing file")?;
    file.functions.reserve_exact(8);
    file.tests.reserve_exact(8);
    let before = spare_by_field(&index);
    // A label only appears once the fixture holds an element for it, and the
    // checks below walk the labels present, so pin the count: a field the
    // fixture stops populating must fail here rather than drop out.
    assert_eq!(before.len(), 47, "labels: {:?}", before.keys());
    // The fixture must leave room on every claimed field, so a field that
    // stays at its starting room after finalization names the shrink call
    // that stopped running.
    let empty: Vec<_> = before
        .iter()
        .filter(|(_, spare)| **spare == 0)
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(empty, Vec::<&str>::new(), "fields the fixture left tight");
    index.finalize()?;
    let after = spare_by_field(&index);
    let unreleased: Vec<_> = before
        .iter()
        .filter(|(label, spare)| after.get(*label).is_none_or(|left| left >= *spare))
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        unreleased,
        Vec::<&str>::new(),
        "fields finalize left as wide"
    );
    Ok(())
}

/// Facts are shrunk in three passes (functions, tests, files); a stride of 2
/// puts checkpoints at the first and third fact of each pass.
fn slack_pass_fixture() -> Result<RustIndex, String> {
    let source = (0..4)
        .map(|n| format!("fn helper_{n}() {{}}\n#[test]\nfn case_{n}() {{}}\n"))
        .collect::<String>();
    let path = PathBuf::from("tests/strided.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, &source)?;
    let mut index = RustIndex::default();
    index.insert_file(path, facts, true);
    for_each_name(&mut index, |name| name.reserve_exact(8));
    Ok(index)
}

fn for_each_name(index: &mut RustIndex, mut visit: impl FnMut(&mut String)) {
    for fact in index.function_facts.iter_mut() {
        visit(&mut fact.name);
    }
    for fact in index.test_facts.iter_mut() {
        visit(&mut fact.name);
    }
}

#[test]
fn the_slack_pass_stops_at_each_interior_checkpoint_and_leaves_later_work_untouched()
-> Result<(), String> {
    use crate::analysis::cancellation::{AnalysisCancellationToken, with_token};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    let stride = 2;
    let fixture = slack_pass_fixture()?;
    let (functions, tests) = (fixture.function_facts.len(), fixture.test_facts.len());
    let checkpoints = |facts: usize| facts.div_ceil(stride);
    let total = checkpoints(functions) + checkpoints(tests) + fixture.files.len();
    assert!(checkpoints(functions) > 1 && checkpoints(tests) > 1);
    // Facts a pass has released when it stops at its `local`th checkpoint.
    let released_before = |facts: usize, local: usize| {
        if local < checkpoints(facts) {
            stride * local
        } else {
            facts
        }
    };
    for expire_at in 0..total {
        let mut index = slack_pass_fixture()?;
        let calls = Arc::new(AtomicUsize::new(0));
        let clock_calls = Arc::clone(&calls);
        let started = Instant::now();
        let token = AnalysisCancellationToken::with_budget(
            started,
            Duration::from_millis(1),
            Arc::new(move || {
                if clock_calls.fetch_add(1, Ordering::SeqCst) < expire_at {
                    started
                } else {
                    started + Duration::from_millis(1)
                }
            }),
        );
        let result = with_token(&token, || index.release_build_slack_in_strides(stride));
        assert_eq!(
            result.err().as_deref(),
            Some("analysis cancelled: DeadlineExceeded"),
            "checkpoint {expire_at} was not observed inside the pass"
        );
        assert_eq!(calls.load(Ordering::SeqCst), expire_at + 1);
        // Work before the stopping checkpoint is released; later work, in
        // this pass and the ones after it, stays untouched.
        let mut released = Vec::new();
        for_each_name(&mut index, |name| {
            released.push(name.capacity() == name.len())
        });
        let (function_work, test_work) = released.split_at(functions);
        let released_functions = released_before(functions, expire_at);
        let released_tests =
            released_before(tests, expire_at.saturating_sub(checkpoints(functions)));
        let expect =
            |count: usize, facts: usize| (0..facts).map(|n| n < count).collect::<Vec<bool>>();
        assert_eq!(
            function_work,
            expect(released_functions, functions),
            "function work at checkpoint {expire_at}"
        );
        let test_count = if expire_at < checkpoints(functions) {
            0
        } else {
            released_tests
        };
        assert_eq!(
            test_work,
            expect(test_count, tests),
            "test work at checkpoint {expire_at}"
        );
    }

    // With no checkpoint expiring the pass finishes and releases everything.
    let mut index = slack_pass_fixture()?;
    index.release_build_slack_in_strides(stride)?;
    let mut released = Vec::new();
    for_each_name(&mut index, |name| {
        released.push(name.capacity() == name.len())
    });
    assert!(released.iter().all(|done| *done));
    Ok(())
}
