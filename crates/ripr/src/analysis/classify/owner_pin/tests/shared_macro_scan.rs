//! #11509: the workspace macro-site scan is parsed once per index, shared by
//! every owner-pin syntax instance, and recomputed when its key changes.
use super::*;

type Answers = (BTreeSet<String>, Vec<Option<(PathBuf, MacroBindingSite)>>);

/// What a syntax instance decides from the scan: the ambiguity set and the
/// refusal site for every trusted name and test root.
fn answers(syntax: &OwnerPinSyntax, index: &RustIndex) -> Answers {
    let ambiguities = syntax.trusted_macro_ambiguities(index);
    let mut sites = Vec::new();
    for name in assertion_context_macros() {
        for root in [None, Some(Path::new(LIB)), Some(Path::new(TESTS))] {
            sites.push(syntax.workspace_macro_site(name, root, index));
        }
    }
    (ambiguities, sites)
}

#[test]
fn every_syntax_instance_over_one_index_shares_one_scan() {
    let index = convergence_index();
    let baseline_index = convergence_index();
    let baseline = answers(&OwnerPinSyntax::default(), &baseline_index);
    let per_instance = baseline_index.call_identity_memos.macro_site_scans.runs();
    assert_eq!(per_instance, 2, "ambiguity pass plus one completion");

    // The diff pass, the seam walk, the receiver owner and the inventory each
    // hold their own instance over the same index.
    for _ in 0..4 {
        assert_eq!(answers(&OwnerPinSyntax::default(), &index), baseline);
    }
    let scans = &index.call_identity_memos.macro_site_scans;
    assert_eq!(scans.runs(), per_instance, "four instances, one scan");
    assert_eq!(scans.scanned_files(), index.files().len());
    // Withheld bindings are applied after the shared scan, per instance.
    let withheld = OwnerPinSyntax::default().with_withheld(Default::default());
    assert_eq!(answers(&withheld, &index), baseline);
    assert_eq!(scans.runs(), per_instance);
}

#[test]
fn a_changed_source_forces_a_recompute_with_the_new_answer() {
    let mut index = convergence_index();
    let before = answers(&OwnerPinSyntax::default(), &index);
    let runs = index.call_identity_memos.macro_site_scans.runs();
    // Replace one file's source in place, bypassing the producers that clear
    // the memos: only the key can notice.
    let replacement: std::sync::Arc<str> =
        "macro_rules! assert_eq { ($a:expr, $b:expr) => {}; }\npub fn drain() {}\n".into();
    let data = index.file_data_mut(Path::new(CONVERGENCE_GLOB));
    assert!(data.is_some(), "the glob file must be indexed");
    if let Some(data) = data {
        data.source = replacement;
    }
    let after = answers(&OwnerPinSyntax::default(), &index);
    assert!(
        index.call_identity_memos.macro_site_scans.runs() > runs,
        "a changed file must be scanned again"
    );
    assert_ne!(after, before, "the new definition must be visible");
    let mut rebuilt = index.clone();
    rebuilt.call_identity_memos.clear();
    assert_eq!(after, answers(&OwnerPinSyntax::default(), &rebuilt));
}

#[test]
fn a_changed_macro_context_forces_a_recompute() {
    let mut index = convergence_index();
    let _ = answers(&OwnerPinSyntax::default(), &index);
    let runs = index.call_identity_memos.macro_site_scans.runs();
    // The workspace macro scope is not file text: a glob import from a
    // workspace-owned crate is read differently.
    index.package_names.insert("rayon".to_string());
    let after = answers(&OwnerPinSyntax::default(), &index);
    assert!(
        index.call_identity_memos.macro_site_scans.runs() > runs,
        "a changed macro scope must not be served the old scan"
    );
    let mut rebuilt = index.clone();
    rebuilt.call_identity_memos.clear();
    assert_eq!(after, answers(&OwnerPinSyntax::default(), &rebuilt));
}

#[test]
fn an_index_mutation_through_the_producers_clears_the_shared_scan() {
    let mut index = convergence_index();
    let _ = answers(&OwnerPinSyntax::default(), &index);
    assert!(index.call_identity_memos.macro_site_scans.runs() > 0);
    let facts = summarize_file(
        PathBuf::from("src/extra.rs"),
        "pub fn extra() {}\n".to_string(),
    );
    index.insert_file_only(PathBuf::from("src/extra.rs"), facts);
    assert_eq!(index.call_identity_memos.macro_site_scans.runs(), 0);
}

#[test]
fn a_cloned_index_starts_with_no_shared_scan() {
    let index = convergence_index();
    let _ = answers(&OwnerPinSyntax::default(), &index);
    let cloned = index.clone();
    assert_eq!(cloned.call_identity_memos.macro_site_scans.runs(), 0);
}
