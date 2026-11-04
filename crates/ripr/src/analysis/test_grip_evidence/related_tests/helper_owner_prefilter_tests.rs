use super::*;
use crate::analysis::facts::WorkspaceRootAuthority;
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::analysis::seam_inventory::inventory_seams_from_index;
use std::cell::RefCell;

thread_local! {
    static HELPER_CHECKS: RefCell<Option<Vec<usize>>> = const { RefCell::new(None) };
}

pub(super) fn record_inherent_helper_check(test_index: usize) {
    HELPER_CHECKS.with_borrow_mut(|checks| {
        if let Some(checks) = checks {
            checks.push(test_index);
        }
    });
}

struct ObserveHelperChecks;

impl Drop for ObserveHelperChecks {
    fn drop(&mut self) {
        HELPER_CHECKS.with_borrow_mut(|checks| *checks = None);
    }
}

fn observed_matches(
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext<'_>,
) -> Result<(BTreeMap<usize, RelationReason>, Vec<usize>), String> {
    HELPER_CHECKS.with_borrow_mut(|checks| *checks = Some(Vec::new()));
    let _observation = ObserveHelperChecks;
    let mut candidates = BTreeMap::new();
    match_helper_owner_call(&mut candidates, context, prefix, owner);
    let checks = HELPER_CHECKS
        .with_borrow_mut(Option::take)
        .ok_or("helper-check observation missing")?;
    Ok((candidates, checks))
}

#[test]
fn helper_owner_package_prefilter_skips_excluded_traversal_without_changing_credit()
-> Result<(), String> {
    let source = r#"
pub struct Inventory { count: usize }
impl Inventory {
    pub fn new() -> Self { Self { count: 1 } }
    pub fn is_empty(&self) -> bool { self.count == 0 }
}
pub struct Other;
impl Other { pub fn is_empty(&self) -> bool { true } }
fn inventory_is_empty() -> bool {
    let inventory = Inventory::new(); inventory.is_empty()
}
fn other_is_empty() -> bool { let other = Other; other.is_empty() }
pub fn empty_free(value: usize) -> bool { if value == 0 { true } else { false } }
fn read_empty_free() -> bool { empty_free(1) }
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn valid_helper_caller() { assert!(!inventory_is_empty()); }
    #[test] fn wrong_helper_receiver() { assert!(other_is_empty()); }
    #[test] fn free_helper_caller() { assert!(!read_empty_free()); }
}
"#;
    let excluded = r#"
struct Foreign;
impl Foreign { fn is_empty(&self) -> bool { true } }
fn foreign_is_empty() -> bool { let foreign = Foreign; foreign.is_empty() }
#[test] fn excluded_helper_caller() { assert!(foreign_is_empty()); }
"#;
    let owner_file = PathBuf::from("crates/helper_prefilter_owner/src/lib.rs");
    let files = [
        (owner_file.clone(), source),
        (PathBuf::from("crates/unrelated/tests/call.rs"), excluded),
        (
            PathBuf::from("crates/helper_prefilter_owner_extra/tests/call.rs"),
            excluded,
        ),
    ];
    let root = super::super::tests::claim_index_fixture_root(12_506_002)?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"helper-prefilter-fixture\"\nversion = \"0.1.0\"\n",
    )
    .map_err(|error| error.to_string())?;
    let mut index = RustIndex::default();
    for (path, source) in &files {
        let facts = RaRustSyntaxAdapter.summarize_file(path, source)?;
        assert!(!facts.used_lexical_fallback);
        index.extend_tests(facts.tests.iter().cloned());
        index.extend_functions(facts.functions.iter().cloned());
        index.insert_file_only(path.clone(), facts);
        let target = root.join(path);
        let parent = target.parent().ok_or("fixture file parent missing")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        std::fs::write(target, source).map_err(|error| error.to_string())?;
    }
    index.workspace_authority = Some(WorkspaceRootAuthority::from_sources(
        &root,
        files.iter().map(|(path, source)| (path, *source)),
    ));
    crate::analysis::facts::compose_fixture_source_roles(&mut index, &root);
    let seams = inventory_seams_from_index(std::slice::from_ref(&owner_file), &index);
    let context = CompactGripContext::new(&index);
    let seam = seams
        .iter()
        .find(|seam| seam.owner().ends_with("::impl Inventory::is_empty"))
        .ok_or("Inventory::is_empty seam missing")?;
    let owner = OwnerContext::resolve(seam, &context);
    assert!(owner.inherent_receiver && owner.inherent_owner.is_some());
    assert_eq!(
        owner.prefix.as_deref(),
        Some("crates/helper_prefilter_owner/")
    );
    let bucket = context
        .tests_by_helper_owner_call_name
        .get(&owner.name)
        .ok_or("same-name helper bucket missing")?;
    assert_eq!(bucket.len(), 4, "all real helper callers must be indexed");
    let admitted: Vec<_> = bucket
        .iter()
        .copied()
        .filter(|index| context.tests[*index].test.file == owner_file)
        .collect();
    assert_eq!(admitted.len(), 2);
    let (candidates, checked) = observed_matches(&context, owner.prefix.as_deref(), &owner)?;
    assert_eq!(checked, admitted, "excluded helpers must never traverse");
    assert_eq!(candidates.len(), 1);
    let (valid_index, reason) = candidates.iter().next().ok_or("valid helper refused")?;
    assert!(
        context.tests[*valid_index]
            .test
            .name
            .ends_with("valid_helper_caller")
    );
    assert_eq!(*reason, RelationReason::HelperOwnerCall);
    let (unfiltered, checked) = observed_matches(&context, None, &owner)?;
    assert_eq!(checked, *bucket);
    for test_index in admitted {
        assert_eq!(unfiltered.get(&test_index), candidates.get(&test_index));
    }
    let mut priority = BTreeMap::from([(*valid_index, RelationReason::DirectOwnerCall)]);
    match_helper_owner_call(&mut priority, &context, owner.prefix.as_deref(), &owner);
    assert_eq!(
        priority.get(valid_index),
        Some(&RelationReason::DirectOwnerCall)
    );

    let free_seam = seams
        .iter()
        .find(|seam| seam.owner().ends_with("::empty_free"))
        .ok_or("free owner seam missing")?;
    let free_owner = OwnerContext::resolve(free_seam, &context);
    assert!(free_owner.free_function && !free_owner.inherent_receiver);
    let (free, checked) = observed_matches(&context, free_owner.prefix.as_deref(), &free_owner)?;
    assert!(
        checked.is_empty(),
        "free projection must not traverse inherent receivers"
    );
    assert_eq!(free.len(), 1);
    let (test_index, reason) = free.iter().next().ok_or("free helper refused")?;
    assert!(
        context.tests[*test_index]
            .test
            .name
            .ends_with("free_helper_caller")
    );
    assert_eq!(*reason, RelationReason::HelperOwnerCall);
    Ok(())
}
