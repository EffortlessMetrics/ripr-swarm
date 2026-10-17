use super::*;

fn counts() -> (usize, usize) {
    (
        PROJECTED_WORKSPACE_FILE_VISITS.with(std::cell::Cell::get),
        PROJECTED_WORKSPACE_FILE_PARSES.with(std::cell::Cell::get),
    )
}

fn verdicts(memo: &RefCell<ProjectedCollectionWorkspaceMemo>, index: &RustIndex) -> (bool, bool) {
    (
        ProjectedCollectionWorkspaceMemo::verdict(
            memo,
            index,
            ProjectedCollectionWorkspaceKey::StandardRoots,
            || projected_standard_roots_shadowed(index),
        ),
        ProjectedCollectionWorkspaceMemo::verdict(
            memo,
            index,
            ProjectedCollectionWorkspaceKey::Constructor(LIB.into(), "Bag".into()),
            || projected_constructor_shadowed("Bag", Path::new(LIB), index),
        ),
    )
}

#[test]
fn workspace_collection_memo_reuses_actual_walks_and_parses() {
    let index = index(&[(LIB, "pub struct Bag;\npub fn add() {}\n")]);
    let memo = RefCell::new(ProjectedCollectionWorkspaceMemo::default());
    let before = counts();
    for _ in 0..12 {
        assert_eq!(verdicts(&memo, &index), (false, false));
    }
    let after = counts();
    assert_eq!((after.0 - before.0, after.1 - before.1), (2, 1));
    let before = counts();
    for _ in 0..12 {
        assert_eq!(
            verdicts(
                &RefCell::new(ProjectedCollectionWorkspaceMemo::default()),
                &index
            ),
            (false, false)
        );
    }
    let after = counts();
    assert_eq!((after.0 - before.0, after.1 - before.1), (24, 12));
}

#[test]
fn workspace_collection_memo_preserves_refusals_and_constructor_keys() {
    for (source, expected) in [
        ("pub struct Bag;", (false, false)),
        ("mod std {}", (true, false)),
        ("mod r#core {}", (true, false)),
        ("use external as std;", (true, false)),
        ("fn Bag() {}", (false, true)),
        ("#[derive(Clone)] pub struct Bag;", (false, true)),
        ("unknown!();", (false, true)),
        ("pub struct Bag {", (false, true)),
    ] {
        let index = index(&[(LIB, source)]);
        let memo = RefCell::new(ProjectedCollectionWorkspaceMemo::default());
        let direct = (
            projected_standard_roots_shadowed(&index),
            projected_constructor_shadowed("Bag", Path::new(LIB), &index),
        );
        assert_eq!(direct, expected, "fixture admission: {source}");
        assert_eq!(verdicts(&memo, &index), direct);
        assert_eq!(verdicts(&memo, &index), direct);
    }
    let index = index(&[(LIB, "pub struct Bag;")]);
    let memo = RefCell::new(ProjectedCollectionWorkspaceMemo::default());
    assert_eq!(verdicts(&memo, &index), (false, false));
    assert!(ProjectedCollectionWorkspaceMemo::verdict(
        &memo,
        &index,
        ProjectedCollectionWorkspaceKey::Constructor(TESTS.into(), "Bag".into()),
        || projected_constructor_shadowed("Bag", Path::new(TESTS), &index),
    ));
    assert!(!ProjectedCollectionWorkspaceMemo::verdict(
        &memo,
        &index,
        ProjectedCollectionWorkspaceKey::Constructor(TESTS.into(), "Other".into()),
        || projected_constructor_shadowed("Other", Path::new(TESTS), &index),
    ));
}

#[test]
fn workspace_collection_memo_isolates_changed_sources_and_nested_misses() {
    let original = index(&[(LIB, "pub struct Bag;")]);
    let changed = index(&[(LIB, "mod std {}\nfn Bag() {}")]);
    let memo = RefCell::new(ProjectedCollectionWorkspaceMemo::default());
    assert_eq!(verdicts(&memo, &original), (false, false));
    let before = counts();
    assert_eq!(verdicts(&memo, &original.clone()), (false, false));
    assert_eq!(counts(), before, "clones retain source allocations");
    let equal_new_allocation = index(&[(LIB, "pub struct Bag;")]);
    assert_eq!(verdicts(&memo, &equal_new_allocation), (false, false));
    assert!(
        counts().0 > before.0,
        "new allocation must invalidate conservatively"
    );
    assert_eq!(verdicts(&memo, &changed), (true, true));
    assert_eq!(verdicts(&memo, &original), (false, false));
    let mut added = original.clone();
    added.insert_file_only(
        PathBuf::from(HELPERS),
        summarize_file(PathBuf::from(HELPERS), "mod core {}".to_owned()),
    );
    assert_eq!(verdicts(&memo, &added), (true, false));
    assert!(added.remove_file(Path::new(HELPERS)));
    assert_eq!(verdicts(&memo, &added), (false, false));
    let mut renamed = original.clone();
    assert!(renamed.remove_file(Path::new(LIB)));
    let mut renamed_facts = summarize_file(PathBuf::from(HELPERS), "pub struct Bag;".to_owned());
    renamed_facts.source = original.files().at(Path::new(LIB)).source.clone();
    renamed.insert_file_only(PathBuf::from(HELPERS), renamed_facts);
    assert_eq!(
        verdicts(&memo, &renamed),
        (false, true),
        "same source allocation under another path changes constructor authority"
    );
    assert_eq!(verdicts(&memo, &original), (false, false));
    let key = ProjectedCollectionWorkspaceKey::Constructor(LIB.into(), "Nested".into());
    assert!(ProjectedCollectionWorkspaceMemo::verdict(
        &memo,
        &original,
        key.clone(),
        || {
            assert_eq!(verdicts(&memo, &changed), (true, true));
            true
        }
    ));
    let recomputed = std::cell::Cell::new(false);
    assert!(ProjectedCollectionWorkspaceMemo::verdict(
        &memo,
        &original,
        key,
        || {
            recomputed.set(true);
            true
        }
    ));
    assert!(
        !recomputed.get(),
        "nested different-index lookup must not replace the outer answer"
    );
}
