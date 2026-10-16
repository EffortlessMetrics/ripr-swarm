use super::*;

fn index_with_source(source: &str) -> RustIndex {
    let mut index = RustIndex::default();
    let path = PathBuf::from("src/lib.rs");
    let facts = crate::analysis::rust_index::summarize_file(path.clone(), source.to_owned());
    index.insert_file_only(path, facts);
    index
}

fn counts() -> (usize, usize) {
    (
        ITEM_NESTING_FILE_PARSES.with(std::cell::Cell::get),
        ITEM_NESTING_NODE_VISITS.with(std::cell::Cell::get),
    )
}

fn nesting(modules: &[&str], cfg_gated: bool) -> Option<ItemNesting> {
    Some(ItemNesting {
        modules: modules.iter().map(|name| (*name).to_owned()).collect(),
        cfg_gated,
    })
}

#[test]
fn item_nesting_facts_preserve_ranges_raw_names_gates_and_placement() {
    for source in [
        "fn plain() {}\n",
        "#[cfg(any())]\nfn plain() {\n#![cfg_attr(any(), allow(unused))]\n}\n",
        "mod r#outer {\n#![cfg(any())]\nmod inner {\nfn r#type() {}\n}\n}\n",
        "struct T;\n#[cfg(any())]\nimpl T {\nfn same() {}\n}\ntrait Tr { fn same() {} }\n",
        "fn same() {\nfn same() {}\n}\nfn other() {fn nested(){}}\n",
        "fn same() {} fn same() {}\n",
        "mod a {fn same(){}}\nmod b {fn same(){}}\n",
        "fn legacy(gen: i32) {}\n",
        "fn broken( {",
        "mod absent;\n",
        "// Unicode λ\r\nmod m {\r\nfn plain() {}\r\n}\r\n",
    ] {
        let facts = ItemNestingFile::parse(source);
        for name in [
            "plain", "r#plain", "same", "nested", "type", "r#type", "legacy", "missing", "other",
        ] {
            for line in 0..=source.lines().count() + 2 {
                for allow_impl in [false, true] {
                    assert_eq!(
                        facts
                            .as_ref()
                            .and_then(|facts| facts.query(name, line, allow_impl)),
                        item_nesting_with_impl(source, name, line, allow_impl),
                        "{source:?}: {name}:{line}, impl={allow_impl}",
                    );
                }
            }
        }
    }
}

#[test]
fn item_nesting_facts_keep_fixed_ambiguity_and_cfg_verdicts() -> Result<(), &'static str> {
    let source = "#[cfg(any())]\nmod r#outer {\nfn r#type() {\n}\n}\n";
    let facts = ItemNestingFile::parse(source).ok_or("valid source refused")?;
    assert_eq!(facts.query("r#type", 3, false), nesting(&["outer"], true));
    assert_eq!(facts.query("type", 4, false), nesting(&["outer"], true));
    assert_eq!(facts.query("type", 2, false), None);
    let facts =
        ItemNestingFile::parse("fn same(){ fn same(){} }").ok_or("nested source refused")?;
    assert_eq!(facts.query("same", 1, false), None);
    let facts = ItemNestingFile::parse("struct T; impl T {fn plain(){}} trait Tr {fn other(){}}")
        .ok_or("implementation source refused")?;
    assert_eq!(facts.query("plain", 1, false), None);
    assert_eq!(facts.query("plain", 1, true), nesting(&[], false));
    assert_eq!(facts.query("other", 1, true), None);
    Ok(())
}

#[test]
fn item_nesting_facts_parse_and_visit_once_for_distinct_queries() {
    let source = "mod a {\nfn first(){}\nfn second(){}\n}\n";
    let index = index_with_source(source);
    let syntax = OwnerPinSyntax::default();
    let path = Path::new("src/lib.rs");
    let before = counts();
    for _ in 0..12 {
        assert_eq!(
            syntax.item_nesting(&index, path, "first", 2),
            nesting(&["a"], false)
        );
        assert_eq!(
            syntax.item_nesting(&index, path, "second", 3),
            nesting(&["a"], false)
        );
        assert_eq!(syntax.item_nesting(&index, path, "absent", 2), None);
    }
    let once = counts();
    assert_eq!(once.0 - before.0, 1);
    assert!(once.1 > before.1);
    let clone = index.clone();
    assert_eq!(
        syntax.item_nesting(&clone, path, "first", 2),
        nesting(&["a"], false)
    );
    assert_eq!(counts(), once);
    // A fresh invocation performs the same real walk; the shared invocation
    // must avoid that parse for every distinct (name, line), not merely repeats.
    for _ in 0..12 {
        assert_eq!(
            OwnerPinSyntax::default().item_nesting(&index, path, "first", 2),
            nesting(&["a"], false)
        );
    }
    let fresh = counts();
    assert_eq!(fresh.0 - once.0, 12);
    assert_eq!(fresh.1 - once.1, 12 * (once.1 - before.1));
}

#[test]
fn item_nesting_facts_refresh_same_path_source_and_parse_refusals() {
    let syntax = OwnerPinSyntax::default();
    let path = Path::new("src/lib.rs");
    let before = counts();
    for (source, expected) in [
        ("mod first {fn plain(){}}", nesting(&["first"], false)),
        (
            "#[cfg(any())] mod next {fn plain(){}}",
            nesting(&["next"], true),
        ),
        ("fn broken( {", None),
        ("mod first {fn plain(){}}", nesting(&["first"], false)),
    ] {
        let index = index_with_source(source);
        for _ in 0..2 {
            assert_eq!(syntax.item_nesting(&index, path, "plain", 1), expected);
        }
    }
    assert_eq!(counts().0 - before.0, 4);
}

#[test]
fn item_nesting_facts_do_not_leak_across_missing_paths_or_independent_owners() {
    let syntax = OwnerPinSyntax::default();
    let index = index_with_source("mod first {fn plain(){}}");
    let path = Path::new("src/lib.rs");
    let before = counts();
    assert_eq!(
        syntax.item_nesting(&index, path, "plain", 1),
        nesting(&["first"], false)
    );
    assert_eq!(
        syntax.item_nesting(&RustIndex::default(), path, "plain", 1),
        None
    );
    assert_eq!(
        syntax.item_nesting(&index, path, "plain", 1),
        nesting(&["first"], false)
    );
    assert_eq!(counts().0 - before.0, 2);
    let before = counts();
    assert_eq!(
        OwnerPinSyntax::default().item_nesting(&index, path, "plain", 1),
        nesting(&["first"], false)
    );
    assert_eq!(
        syntax.item_nesting(&index, Path::new("renamed.rs"), "plain", 1),
        None
    );
    assert_eq!(counts().0 - before.0, 1);
}
