use super::super::{
    STD_ROOTS, bare_name_bound_only_by_std_globs, bare_name_renames_another_function,
};
use super::*;
use crate::analysis::rust_index::{RustIndex, summarize_file};

/// The standard-glob prefilter as the binding check spelled it before the
/// memo, kept verbatim as the reference.
fn reference_spells_std_glob(source: &str) -> bool {
    source.contains('*')
        && STD_ROOTS
            .iter()
            .any(|root| source.contains(&format!("{root}::")))
}

/// Hand-written sources covering every branch of the per-hit test: plain,
/// braced, raw and multi-line renames, block and line comments before the
/// word, a `//` earlier on the line or on an earlier line only, `xr#`, casts,
/// near-miss words, non-ASCII identifiers and glob spellings.
const HAND_SOURCES: [&str; 22] = [
    "use super::other as discount;",
    "use super::{other as\n    discount};",
    "use super::other as r#discount;",
    "use super::other as /* alias */ discount;",
    "use super::other as // alias\n    discount;",
    "use super::other as discounted;",
    "use super::other as my_discount;",
    "use super::has discount;",
    "let x = alias(discount);",
    "use super::discount;",
    "// note\nlet x = discount;",
    "let y = 1; // trailing\nlet x = discount;",
    "let x = 1 // a\n\n   discount",
    "xr#discount as r#discount",
    "let n = m as usize; total as\tdiscount",
    "use crate::módulo as dulo; módulo",
    "a/ /discount a//discount /*x*/discount",
    "use std::cmp::*;\nfn t() { max(1, 2); }",
    "use std::cmp::{*};",
    "use core::ops::Add; let x = 2 * 3;",
    "",
    "as",
];

/// Deterministic pseudo-random sources from fragments that exercise the
/// boundaries: words, `as`, `r#`, comments, newlines, slashes, non-ASCII.
fn generated_sources() -> Vec<String> {
    const FRAGMENTS: [&str; 22] = [
        "as",
        " ",
        "\n",
        "\t",
        "//",
        "/*",
        "*/",
        "r#",
        "discount",
        "x",
        "é",
        "_",
        "::",
        "*",
        "#",
        "/",
        "std::",
        "as discount",
        "módulo",
        "\u{301}",
        ";",
        "r",
    ];
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..4000)
        .map(|_| {
            let len = 1 + (next() % 24) as usize;
            (0..len)
                .map(|_| FRAGMENTS[(next() % FRAGMENTS.len() as u64) as usize])
                .collect()
        })
        .collect()
}

/// The once-per-file read answers exactly what the per-test scan answered,
/// for every word of every source and for names that are not one word (the
/// fallback). Both answers must occur, so the comparison cannot pass on a
/// constant.
#[test]
fn the_file_read_answers_exactly_the_per_test_scan() {
    let mut sources: Vec<String> = HAND_SOURCES.iter().map(|s| (*s).to_string()).collect();
    sources.extend(generated_sources());
    let fixed_names = [
        "discount",
        "x",
        "as",
        "r",
        "_",
        "é",
        "dulo",
        "módulo",
        "max",
        "discounted",
        "",
        "r#discount",
        "a b",
        "::",
        "dis",
        "count",
    ];
    let (mut yes, mut no, mut std_yes, mut std_no) = (0, 0, 0, 0);
    for source in &sources {
        let spellings = FileSpellings::read(source);
        let words = source
            .split(|ch: char| !is_ident_char(ch))
            .filter(|word| !word.is_empty());
        for name in fixed_names.iter().copied().chain(words) {
            let expected = source_spells_rename_to(source, name);
            assert_eq!(
                spellings.spells_rename_to(source, name),
                expected,
                "source {source:?}, name {name:?}"
            );
            if expected {
                yes += 1;
            } else {
                no += 1;
            }
        }
        let expected = reference_spells_std_glob(source);
        assert_eq!(spellings.spells_std_glob(), expected, "{source:?}");
        if expected {
            std_yes += 1;
        } else {
            std_no += 1;
        }
    }
    assert!(
        yes > 1000 && no > 1000,
        "rename answers: {yes} yes, {no} no"
    );
    assert!(
        std_yes > 100 && std_no > 100,
        "std-glob answers: {std_yes} yes, {std_no} no"
    );
}

/// The memo reads a file once for any number of lookups, and never reuses a
/// read for another source under the same path.
#[test]
fn the_memo_reads_each_source_once_and_never_reuses_another() {
    let memo = FileSpellingsMemo::default();
    let file = Path::new("tests/it.rs");
    let renamed: Arc<str> = Arc::from("use super::other as discount;");
    let plain: Arc<str> = Arc::from("use super::discount;");
    let before = FILE_SPELLING_READS.with(std::cell::Cell::get);
    for _ in 0..50 {
        assert!(file_spellings(&memo, file, &renamed).spells_rename_to(&renamed, "discount"));
    }
    assert_eq!(FILE_SPELLING_READS.with(std::cell::Cell::get) - before, 1);
    // Equal text in another allocation is another source: read again.
    let same_text: Arc<str> = Arc::from(&*renamed);
    assert!(file_spellings(&memo, file, &same_text).spells_rename_to(&same_text, "discount"));
    assert_eq!(FILE_SPELLING_READS.with(std::cell::Cell::get) - before, 2);
    // Different text under the same path answers from its own text.
    assert!(!file_spellings(&memo, file, &plain).spells_rename_to(&plain, "discount"));
    assert_eq!(FILE_SPELLING_READS.with(std::cell::Cell::get) - before, 3);
}

/// Production path: the relation asks both binding checks for every test
/// of a many-test file, and the file's text is walked once for all of them.
#[test]
fn many_tests_in_one_file_read_its_text_once() -> Result<(), String> {
    let mut tests = String::from("use std::cmp::*;\nuse modules::other as discount;\n");
    for case in 0..200 {
        tests.push_str(&format!(
            "\n#[test]\nfn case_{case}() {{\n    assert_eq!(discount({case}), 10);\n}}\n"
        ));
    }
    let lib = "pub fn discount(x: u32) -> u32 {\n    x\n}\n\npub fn other(x: u32) -> u32 {\n    x + 10\n}\n";
    let mut index = RustIndex::default();
    index.package_names.insert("modules".to_string());
    for (path, text) in [("src/lib.rs", lib), ("tests/it.rs", tests.as_str())] {
        let facts = summarize_file(PathBuf::from(path), text.to_string());
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from(path), facts);
    }
    crate::analysis::facts::compose_fixture_source_roles(&mut index, Path::new(""));
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == "discount")
        .ok_or("owner `discount` must be indexed")?;
    assert_eq!(index.tests().len(), 200, "every generated test is indexed");
    let before = FILE_SPELLING_READS.with(std::cell::Cell::get);
    let mut renamed = 0;
    for test in index.tests() {
        if bare_name_renames_another_function(&index, owner, test) {
            renamed += 1;
        }
        let _ = bare_name_bound_only_by_std_globs(&index, owner, test);
    }
    assert_eq!(renamed, 200, "each bare call is the renamed `other`");
    assert_eq!(
        FILE_SPELLING_READS.with(std::cell::Cell::get) - before,
        1,
        "one walk of the test file serves every test"
    );
    Ok(())
}
