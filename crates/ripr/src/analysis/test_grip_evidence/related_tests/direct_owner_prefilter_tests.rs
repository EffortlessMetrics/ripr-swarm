use super::*;
use crate::analysis::facts::WorkspaceRootAuthority;
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::analysis::seam_inventory::inventory_seams_from_index;
use std::cell::RefCell;

thread_local! {
    static RECEIVER_CHECKS: RefCell<Option<Vec<usize>>> = const { RefCell::new(None) };
}

pub(super) fn record_inherent_receiver_check(test_index: usize) {
    RECEIVER_CHECKS.with_borrow_mut(|checks| {
        if let Some(checks) = checks {
            checks.push(test_index);
        }
    });
}

struct ObserveReceiverChecks;

impl Drop for ObserveReceiverChecks {
    fn drop(&mut self) {
        RECEIVER_CHECKS.with_borrow_mut(|checks| *checks = None);
    }
}

fn observed_matches(
    context: &CompactGripContext<'_>,
    prefix: Option<&str>,
    owner: &OwnerContext<'_>,
) -> Result<(BTreeMap<usize, RelationReason>, Vec<usize>), String> {
    RECEIVER_CHECKS.with_borrow_mut(|checks| *checks = Some(Vec::new()));
    let _observation = ObserveReceiverChecks;
    let mut candidates = BTreeMap::new();
    match_direct_owner_call(&mut candidates, context, prefix, owner);
    let checks = RECEIVER_CHECKS
        .with_borrow_mut(Option::take)
        .ok_or("receiver-check observation missing")?;
    Ok((candidates, checks))
}

#[test]
fn direct_owner_package_prefilter_skips_excluded_receiver_work_without_changing_credit()
-> Result<(), String> {
    let source = r#"
pub struct Inventory { stock: u32 }
impl Inventory {
    pub fn new(stock: u32) -> Self { Self { stock } }
    pub fn remove(&mut self, qty: u32) -> u32 {
        if self.stock >= qty { self.stock -= qty; }
        self.stock
    }
}
pub struct Other;
impl Other { pub fn remove(&mut self, _qty: u32) -> u32 { 99 } }
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn valid_caller() {
        let mut inventory = Inventory::new(3);
        assert_eq!(inventory.remove(1), 2);
    }
    #[test] fn wrong_receiver() {
        let mut other = Other;
        assert_eq!(other.remove(1), 99);
    }
    #[test] fn shadowed_receiver() {
        struct Inventory;
        impl Inventory { fn remove(&mut self, _qty: u32) -> u32 { 99 } }
        let mut inventory = Inventory;
        assert_eq!(inventory.remove(1), 99);
    }
}
"#;
    let excluded = r#"
#[test] fn excluded_caller() {
    let mut inventory = crate::Inventory::new(3);
    assert_eq!(inventory.remove(1), 2);
}
"#;
    let owner_file = PathBuf::from("crates/prefilter_owner/src/lib.rs");
    let files = [
        (owner_file.clone(), source),
        (PathBuf::from("crates/unrelated/tests/call.rs"), excluded),
        (
            PathBuf::from("crates/prefilter_owner_extra/tests/call.rs"),
            excluded,
        ),
    ];
    let root = super::super::tests::claim_index_fixture_root(12_506_001)?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"prefilter-fixture\"\nversion = \"0.1.0\"\n",
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
    let seam = seams
        .iter()
        .find(|seam| {
            seam.kind() == SeamKind::PredicateBoundary
                && seam.owner().ends_with("::impl Inventory::remove")
        })
        .ok_or("Inventory::remove predicate missing")?;
    let context = CompactGripContext::new(&index);
    let owner = OwnerContext::resolve(seam, &context);
    assert!(owner.inherent_receiver && owner.inherent_owner.is_some());
    assert_eq!(owner.prefix.as_deref(), Some("crates/prefilter_owner/"));
    let bucket = context
        .tests_by_call_name
        .get(&owner.name)
        .ok_or("same-name remove bucket missing")?;
    assert_eq!(bucket.len(), 5, "all real remove callers must be indexed");
    let (candidates, checked) = observed_matches(&context, owner.prefix.as_deref(), &owner)?;
    let admitted: Vec<_> = bucket
        .iter()
        .copied()
        .filter(|index| context.tests[*index].test.file == owner_file)
        .collect();
    assert_eq!(admitted.len(), 3);
    assert_eq!(
        checked, admitted,
        "excluded callers must never resolve receivers"
    );
    assert_eq!(candidates.len(), 1);
    let (valid_index, reason) = candidates.iter().next().ok_or("valid caller refused")?;
    assert!(
        context.tests[*valid_index]
            .test
            .name
            .ends_with("valid_caller")
    );
    assert_eq!(*reason, RelationReason::DirectOwnerCall);

    // A missing prefix retains the old unfiltered receiver checks, including
    // both foreign paths and the prefix-sharing package name.
    let (unfiltered, checked) = observed_matches(&context, None, &owner)?;
    assert_eq!(checked, *bucket);
    for test_index in admitted {
        assert_eq!(unfiltered.get(&test_index), candidates.get(&test_index));
    }
    Ok(())
}
