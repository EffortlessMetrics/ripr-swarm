use super::super::build_index;
use super::{DocExample, doc_examples};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> Result<TempDir, Box<dyn Error>> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-{name}-{stamp}"));
    fs::create_dir_all(&root)?;
    Ok(TempDir(root))
}

const WIRE: &str = "/// Reads a big-endian `u16`.
///
/// ```
/// assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);
/// ```
pub fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}
";

fn examples(source: &str) -> Vec<(usize, Vec<String>, usize)> {
    doc_examples(source, None)
        .into_iter()
        .map(
            |DocExample {
                 fence_line,
                 code,
                 item_line,
             }| (fence_line, code, item_line),
        )
        .collect()
}

#[test]
fn a_plain_fence_on_a_function_is_one_example_at_its_lines() {
    assert_eq!(
        examples(WIRE),
        vec![(
            3,
            vec!["assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);".to_string()],
            6
        )]
    );
}

#[test]
fn only_plain_rust_and_edition_fences_run() {
    for (info, runs) in [
        ("", true),
        ("rust", true),
        ("rust,edition2021", true),
        ("rust edition2021", true),
        ("edition2021 ignore", false),
        ("edition2024", true),
        ("ignore", false),
        ("rust,ignore", false),
        ("no_run", false),
        ("compile_fail", false),
        ("should_panic", false),
        ("text", false),
        ("toml", false),
    ] {
        let source = WIRE.replace("/// ```\n/// assert", &format!("/// ```{info}\n/// assert"));
        assert_eq!(!examples(&source).is_empty(), runs, "fence `{info}`");
    }
}

#[test]
fn hidden_lines_are_restored_and_a_main_refuses() {
    let hidden = "/// ```\n/// # use wire::read_u16;\n/// #\n/// ## not hidden\n/// assert_eq!(read_u16(&[0, 1]), 1);\n/// ```\npub fn read_u16() {}\n";
    assert_eq!(
        examples(hidden),
        vec![(
            1,
            vec![
                "use wire::read_u16;".to_string(),
                String::new(),
                "# not hidden".to_string(),
                "assert_eq!(read_u16(&[0, 1]), 1);".to_string(),
            ],
            7
        )]
    );
    let main = "/// ```\n/// fn main() {\n///     assert_eq!(wire::f(), 1);\n/// }\n/// ```\npub fn f() -> u8 { 1 }\n";
    assert!(examples(main).is_empty());
}

#[test]
fn an_unclosed_fence_inner_docs_and_plain_comments_yield_nothing() {
    for source in [
        "/// ```\n/// assert_eq!(wire::f(), 1);\npub fn f() -> u8 { 1 }\n",
        "//! ```\n//! assert_eq!(wire::f(), 1);\n//! ```\npub fn f() -> u8 { 1 }\n",
        "//// ```\n//// assert_eq!(wire::f(), 1);\n//// ```\npub fn f() -> u8 { 1 }\n",
        "// ```\n// assert_eq!(wire::f(), 1);\n// ```\npub fn f() -> u8 { 1 }\n",
    ] {
        assert!(examples(source).is_empty(), "{source}");
    }
}

#[test]
fn backticks_markdown_may_not_read_as_a_code_block_yield_nothing() {
    // An HTML comment (a commented-out example), a `~~~` fence around the
    // backticks, or an indented code block: rustdoc runs none of them.
    let example = "/// ```\n/// assert_eq!(wire::f(), 1);\n/// ```\n";
    let item = "pub fn f() -> u8 { 1 }\n";
    assert_eq!(examples(&format!("{example}{item}")).len(), 1);
    for wrapped in [
        format!("/// <!--\n{example}/// -->\n{item}"),
        format!("/// ~~~text\n{example}/// ~~~\n{item}"),
        format!(
            "/// Text.\n///\n///     ```\n///     assert_eq!(wire::f(), 1);\n///     ```\n{item}"
        ),
        format!("/// Text.\n///\n///\t```\n///\tassert_eq!(wire::f(), 1);\n///\t```\n{item}"),
    ] {
        assert!(examples(&wrapped).is_empty(), "{wrapped}");
    }
}

#[test]
fn the_synthetic_source_keeps_the_comment_lines() {
    let example = &doc_examples(WIRE, None)[0];
    let source = example.synthetic_source();
    let lines: Vec<&str> = source.lines().collect();
    assert_eq!(lines[2], "#[test] fn ripr_doctest() {");
    assert_eq!(
        lines[3],
        "assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);"
    );
    assert_eq!(lines[4], "}");
}

fn package(root: &Path, manifest_extra: &str, lib: &str) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"wire\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{manifest_extra}"
        ),
    )?;
    fs::write(root.join("src/lib.rs"), lib)?;
    Ok(())
}

/// (name, start line, calls `read_u16`) of each registered doc example.
type Doctests = Vec<(String, usize, bool)>;

fn doctests(root: &Path, files: &[&str]) -> Result<Doctests, Box<dyn Error>> {
    let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let index = build_index(root, &files)?;
    Ok(index
        .tests()
        .iter()
        .filter(|test| index.doctest_source(test).is_some())
        .map(|test| {
            (
                test.name.clone(),
                test.start_line,
                test.calls.iter().any(|call| call.name == "read_u16"),
            )
        })
        .collect())
}

#[test]
fn a_library_doc_example_is_indexed_as_a_test() -> Result<(), Box<dyn Error>> {
    let dir = temp_dir("doctest-indexed")?;
    package(&dir.0, "", WIRE)?;
    let index = build_index(&dir.0, &[PathBuf::from("src/lib.rs")])?;
    let test = index
        .tests()
        .iter()
        .find(|test| index.doctest_source(test).is_some())
        .cloned()
        .ok_or("doc example not indexed")?;
    assert_eq!(test.name, "src/lib.rs - read_u16 (line 3)");
    assert_eq!((test.start_line, test.end_line), (3, 5));
    assert!(
        test.calls
            .iter()
            .any(|call| call.name == "read_u16" && call.line == 4)
    );
    assert_eq!(test.assertions.len(), 1);
    assert_eq!(test.assertions[0].line, 4);
    Ok(())
}

#[test]
fn a_module_of_the_library_runs_its_doc_examples() -> Result<(), Box<dyn Error>> {
    let dir = temp_dir("doctest-module")?;
    package(&dir.0, "", "pub mod codec;\n")?;
    fs::write(dir.0.join("src/codec.rs"), WIRE)?;
    assert_eq!(
        doctests(&dir.0, &["src/lib.rs", "src/codec.rs"])?,
        vec![("src/codec.rs - read_u16 (line 3)".to_string(), 3, true)]
    );
    Ok(())
}

#[test]
fn targets_rustdoc_does_not_test_register_nothing() -> Result<(), Box<dyn Error>> {
    // Control and refusals side by side: the same comment counts only in a
    // default library that keeps doctests on.
    for (manifest, lib_file, expected) in [
        ("", "src/lib.rs", 1),
        ("[lib]\ndoctest = false\n", "src/lib.rs", 0),
        ("[lib]\ncrate-type = [\"cdylib\"]\n", "src/lib.rs", 0),
        ("[lib]\npath = \"src/other.rs\"\n", "src/lib.rs", 0),
        ("[package.metadata]\nx = 1\n", "src/main.rs", 0),
        (
            "[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n",
            "src/lib.rs",
            1,
        ),
        ("[lib]\ncrate-type = [\"rlib\"]\n", "src/lib.rs", 1),
        ("[lib]\nproc-macro = true\n", "src/lib.rs", 0),
        ("[lib]\ncrate-type = [\"proc-macro\"]\n", "src/lib.rs", 0),
        ("[lib]\ndoctest = \"yes\"\n", "src/lib.rs", 0),
        ("autolib = false\n", "src/lib.rs", 0),
    ] {
        let dir = temp_dir("doctest-targets")?;
        package(&dir.0, manifest, "")?;
        fs::write(dir.0.join(lib_file), WIRE)?;
        assert_eq!(
            doctests(&dir.0, &[lib_file])?.len(),
            expected,
            "{manifest} {lib_file}"
        );
    }
    Ok(())
}

#[test]
fn a_file_reached_other_than_by_a_plain_mod_registers_nothing() -> Result<(), Box<dyn Error>> {
    // The same module file counts only when a plain `mod` composes it into
    // the library: not under `#[cfg(test)]`, through `include!`, or when
    // the library leaves it unresolved.
    for (lib, expected) in [
        ("mod wire_mod;\n", 1),
        ("#[cfg(test)]\nmod wire_mod;\n", 0),
        ("include!(\"wire_mod.rs\");\n", 0),
        ("", 0),
    ] {
        let dir = temp_dir("doctest-compose")?;
        package(&dir.0, "", lib)?;
        fs::write(dir.0.join("src/wire_mod.rs"), WIRE)?;
        assert_eq!(
            doctests(&dir.0, &["src/lib.rs", "src/wire_mod.rs"])?.len(),
            expected,
            "{lib}"
        );
    }
    Ok(())
}

#[test]
fn html_blocks_and_split_doc_fragments_yield_nothing() {
    let example = "/// ```\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\n";
    for source in [
        // A collapsible example: the backticks are raw HTML in the block.
        format!(
            "/// <details><summary>Example</summary>\n///\n{example}/// </details>\npub fn read_u16() {{}}\n"
        ),
        format!("/// </div>\n{example}pub fn read_u16() {{}}\n"),
        // rustdoc joins fragments, so the fence may open in another one.
        format!("/// ```text\n#[doc = \"x\"]\n{example}pub fn read_u16() {{}}\n"),
        format!("{example}#[cfg_attr(docsrs, doc = \"x\")]\npub fn read_u16() {{}}\n"),
        format!("/// ```text\n#[inline]\n{example}pub fn read_u16() {{}}\n"),
        format!("{example}// note\n/// more\npub fn read_u16() {{}}\n"),
        // A tag whose attribute holds a URL is still a tag.
        format!("/// <img src=\"https://example.com/a.png\">\n{example}pub fn read_u16() {{}}\n"),
    ] {
        assert_eq!(examples(&source), Vec::new(), "{source}");
    }
    // Controls: an autolink is not a tag, and a whole-line attribute
    // between the comment and the function is skipped.
    for source in [
        format!("/// See <https://example.com>.\n{example}pub fn read_u16() {{}}\n"),
        format!("{example}#[inline]\n#[must_use]\npub fn read_u16() {{}}\n"),
    ] {
        assert_eq!(examples(&source).len(), 1, "{source}");
    }
}

#[test]
fn a_one_line_attributed_item_keeps_its_own_comment() -> Result<(), Box<dyn Error>> {
    // The comment documents `helper`, compiled out under `cfg(test)`; it
    // does not pass to the next function.
    let example = "/// ```\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\n";
    let dir = temp_dir("doctest-one-line-attr")?;
    for (lib, expected) in [
        (
            format!("{example}#[cfg(test)] pub fn helper() {{}}\npub fn read_u16() {{}}\n"),
            0,
        ),
        (format!("{example}#[inline] pub fn read_u16() {{}}\n"), 0),
        (format!("{example}#[inline]\npub fn read_u16() {{}}\n"), 1),
        (
            format!("{example}#[inline] // hot\npub fn read_u16() {{}}\n"),
            1,
        ),
        // An attribute that does not close on its line is the item line.
        (
            format!(
                "{example}#[deprecated(note = \"[\")] pub(crate) fn helper() {{}}\npub fn read_u16() {{}}\n"
            ),
            0,
        ),
    ] {
        package(&dir.0, "", &lib)?;
        assert_eq!(doctests(&dir.0, &["src/lib.rs"])?.len(), expected, "{lib}");
    }
    Ok(())
}

#[test]
fn fences_need_matching_ticks_and_items_need_fn_qualifiers_only() -> Result<(), Box<dyn Error>> {
    // A four-backtick fence closes only at four or more backticks, so the
    // inner three-backtick line is code, not a close.
    let four = "/// ````\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\n/// ````\npub fn read_u16() {}\n";
    assert_eq!(examples(four).len(), 1);
    assert_eq!(
        examples(four).first().map(|example| example.1.clone()),
        Some(vec![
            "assert_eq!(wire::read_u16(&[0, 1]), 1);".to_string(),
            "```".to_string()
        ])
    );
    // Only `pub` and fn qualifiers may precede `fn` on the item line.
    let dir = temp_dir("doctest-qualifiers")?;
    for (head, expected) in [
        ("pub const fn read_u16", 1),
        ("pub async unsafe fn read_u16", 1),
        ("pub extern \"C\" fn read_u16", 1),
        ("pub default fn read_u16", 0),
    ] {
        let lib = WIRE.replace("pub fn read_u16", head);
        package(&dir.0, "", &lib)?;
        assert_eq!(doctests(&dir.0, &["src/lib.rs"])?.len(), expected, "{head}");
    }
    Ok(())
}

#[test]
fn doc_examples_in_test_code_or_on_non_functions_register_nothing() -> Result<(), Box<dyn Error>> {
    let dir = temp_dir("doctest-test-code")?;
    let lib = format!(
        "#[cfg(test)]\nmod tests {{\n{}\n}}\n\n/// ```\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\npub struct Frame;\n",
        WIRE
    );
    package(&dir.0, "", &lib)?;
    assert_eq!(doctests(&dir.0, &["src/lib.rs"])?, Vec::new());
    // A comment on a statement whose trailing comment mentions the owner,
    // and examples on functions callers outside the crate cannot reach.
    for lib in [
        "pub fn read_u16(bytes: &[u8]) -> u16 {\n    /// ```\n    /// assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);\n    /// ```\n    let _x = 1; // fn read_u16\n    u16::from_be_bytes([bytes[0], bytes[1]])\n}\n".to_string(),
        // A nested item's comment, whose trailing comment or tail mentions
        // `fn read_u16`, is not the function's either.
        "pub fn read_u16(bytes: &[u8]) -> u16 {\n    /// ```\n    /// assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);\n    /// ```\n    pub struct Inner; // fn read_u16\n    u16::from_be_bytes([bytes[0], bytes[1]])\n}\n".to_string(),
        "pub fn read_u16(bytes: &[u8]) -> u16 {\n    /// ```\n    /// assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);\n    /// ```\n    pub static X: u8 = { fn read_u16() {} 0 };\n    u16::from_be_bytes([bytes[0], bytes[1]])\n}\n".to_string(),
        WIRE.replace("pub fn read_u16", "fn read_u16"),
        WIRE.replace("pub fn read_u16", "pub(crate) fn read_u16"),
    ] {
        package(&dir.0, "", &lib)?;
        assert_eq!(doctests(&dir.0, &["src/lib.rs"])?, Vec::new(), "{lib}");
    }
    // Control: the same function outside the test module registers its
    // example, so the refusal above is the module's, not the comment's.
    let control = format!("{WIRE}\n#[cfg(test)]\nmod tests {{}}\n");
    package(&dir.0, "", &control)?;
    assert_eq!(
        doctests(&dir.0, &["src/lib.rs"])?,
        vec![("src/lib.rs - read_u16 (line 3)".to_string(), 3, true)]
    );
    Ok(())
}

#[test]
fn cfg_gated_items_and_inner_cfg_files_register_nothing() -> Result<(), Box<dyn Error>> {
    let example = "/// ```\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\n";
    let dir = temp_dir("doctest-cfg-gated")?;
    for lib in [
        format!("{example}#[cfg(feature = \"x\")]\npub fn read_u16() {{}}\n"),
        format!("{example}#[cfg(not(doctest))]\npub fn read_u16() {{}}\n"),
        format!("{example}#[cfg_attr(docsrs, doc = \"x\")]\npub fn read_u16() {{}}\n"),
    ] {
        package(&dir.0, "", &lib)?;
        assert_eq!(doctests(&dir.0, &["src/lib.rs"])?, Vec::new(), "{lib}");
        assert_eq!(examples(&lib), Vec::new(), "{lib}");
    }
    package(&dir.0, "", &format!("#![cfg(feature = \"x\")]\n{WIRE}"))?;
    assert_eq!(doctests(&dir.0, &["src/lib.rs"])?, Vec::new());
    // An inner `#![cfg_attr(.., cfg(..))]` can add a gate, so it refuses too.
    package(
        &dir.0,
        "",
        &format!("#![cfg_attr(feature = \"x\", cfg(any()))]\n{WIRE}"),
    )?;
    assert_eq!(doctests(&dir.0, &["src/lib.rs"])?, Vec::new());
    // An inner `#![cfg_attr(..)]` is an ordinary crate-root header, not a gate.
    package(
        &dir.0,
        "",
        &format!("#![cfg_attr(docsrs, feature(doc_cfg))]\n{WIRE}"),
    )?;
    assert_eq!(doctests(&dir.0, &["src/lib.rs"])?.len(), 1);
    let cfg_doctest = format!("{example}#[cfg(doctest)]\npub fn read_u16() {{}}\n");
    assert_eq!(examples(&cfg_doctest).len(), 1);
    // And it registers through the index, not just the example scan.
    package(&dir.0, "", &cfg_doctest)?;
    assert_eq!(doctests(&dir.0, &["src/lib.rs"])?.len(), 1);
    Ok(())
}

#[test]
fn a_spaced_doc_attribute_before_the_run_is_a_shared_document() {
    // rustdoc joins `#[ doc = ".."]` with the `///` run below it, so the
    // opening fence in the attribute closes at the run's first fence.
    let lib = "#[ doc = \"```text\"]\n/// ```\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\npub fn read_u16() {}\n";
    assert_eq!(examples(lib), Vec::new(), "{lib}");
}

#[test]
fn cfg_doctest_attribute_is_allowed_between_doc_and_item() {
    use super::doc_examples;
    assert!(super::attribute_is_cfg_doctest("#[cfg(doctest)]"));
    assert!(!super::between_doc_and_item_cfg_refuses(
        "#[cfg(doctest)]",
        None
    ));
    assert!(super::between_doc_and_item_cfg_refuses(
        "#[cfg(feature = \"x\")]",
        None
    ));
    let example = "/// ```\n/// assert_eq!(1, 1);\n/// ```\n";
    let lib = format!("{example}#[cfg(doctest)]\npub fn read_u16() {{}}\n");
    assert_eq!(doc_examples(&lib, None).len(), 1, "{lib}");
}

#[test]
fn a_doctest_source_answers_only_for_the_registered_test() -> Result<(), Box<dyn Error>> {
    let dir = temp_dir("doctest-identity")?;
    package(&dir.0, "", WIRE)?;
    let index = build_index(&dir.0, &[PathBuf::from("src/lib.rs")])?;
    let test = index
        .tests()
        .iter()
        .find(|test| index.doctest_source(test).is_some())
        .cloned()
        .ok_or("doc example not indexed")?;
    let expected = doc_examples(WIRE, None)
        .first()
        .map(DocExample::synthetic_source);
    assert_eq!(index.doctest_source(&test), expected.as_deref());
    // Another test at the same coordinates does not borrow the source.
    let mut other = test;
    other.name = "read_u16_works".to_string();
    assert_eq!(index.doctest_source(&other), None);
    Ok(())
}

const GATED_EXAMPLE: &str = "/// ```\n/// assert_eq!(wire::read_u16(&[0, 1]), 1);\n/// ```\n";

/// How many doc examples register for `lib` in a package whose manifest has
/// `manifest_extra` appended (RIPR-SPEC-0279, default-feature gates, #12230).
fn registered_with_manifest(manifest_extra: &str, lib: &str) -> Result<usize, Box<dyn Error>> {
    let dir = temp_dir("doctest-default-features")?;
    package(&dir.0, manifest_extra, lib)?;
    Ok(doctests(&dir.0, &["src/lib.rs"])?.len())
}

fn gated_item(gate: &str) -> String {
    format!("{GATED_EXAMPLE}{gate}\npub fn read_u16() {{}}\n")
}

#[test]
fn a_default_feature_gate_registers_the_example() -> Result<(), Box<dyn Error>> {
    let features = "\n[features]\ndefault = [\"std\"]\nstd = [\"alloc\"]\nalloc = []\nextra = []\n";
    for gate in [
        "#[cfg(feature = \"std\")]",
        // Reached through another default feature.
        "#[cfg(feature = \"alloc\")]",
        "#[cfg(feature=\"alloc\")]",
        "#[cfg(all(feature = \"std\", feature = \"alloc\"))]",
        "#[cfg(any(feature = \"extra\", feature = \"alloc\"))]",
        "#[cfg(all(doctest, feature = \"alloc\"))]",
        // Two stacked gates are a conjunction; both hold.
        "#[cfg(feature = \"std\")]\n#[cfg(feature = \"alloc\")]",
    ] {
        assert_eq!(
            registered_with_manifest(features, &gated_item(gate))?,
            1,
            "{gate}"
        );
    }
    // The shape in rust-hex: a default feature that is a bare optional name.
    assert_eq!(
        registered_with_manifest(
            "\n[features]\ndefault = [\"std\"]\nstd = [\"alloc\"]\nalloc = []\n",
            &gated_item("#[must_use]\n#[cfg(feature = \"alloc\")]"),
        )?,
        1
    );
    // An inner gate that holds is no gate at all.
    assert_eq!(
        registered_with_manifest(features, &format!("#![cfg(feature = \"std\")]\n{WIRE}"))?,
        1
    );
    Ok(())
}

#[test]
fn a_gate_that_may_be_off_in_a_plain_cargo_test_still_refuses() -> Result<(), Box<dyn Error>> {
    let features =
        "\n[features]\ndefault = [\"std\"]\nstd = []\nextra = []\nserde = [\"dep:serde\"]\n";
    for gate in [
        // Declared but not default.
        "#[cfg(feature = \"extra\")]",
        // Not declared at all.
        "#[cfg(feature = \"nope\")]",
        // One false conjunct; `all()` has no operand to prove anything.
        "#[cfg(all(feature = \"std\", feature = \"extra\"))]",
        "#[cfg(all())]",
        "#[cfg(any())]",
        // Negations are never resolved, whichever way the default points.
        "#[cfg(not(feature = \"extra\"))]",
        "#[cfg(not(feature = \"std\"))]",
        "#[cfg(not(not(feature = \"std\")))]",
        "#[cfg(any(not(feature = \"extra\"), feature = \"extra\"))]",
        // Other predicates: target, custom cfg, test, a feature next to one.
        "#[cfg(target_os = \"linux\")]",
        "#[cfg(unix)]",
        "#[cfg(my_flag)]",
        "#[cfg(all(feature = \"std\", target_os = \"linux\"))]",
        "#[cfg(test)]",
        "#[cfg(not(doctest))]",
        // A stacked pair is a conjunction: one false gate refuses.
        "#[cfg(feature = \"std\")]\n#[cfg(feature = \"extra\")]",
        // `cfg_attr` never resolves, even around a default feature.
        "#[cfg_attr(feature = \"std\", inline)]",
        // A dependency's feature is not this package's feature.
        "#[cfg(feature = \"serde\")]",
        // Malformed predicates read as not true.
        "#[cfg(feature)]",
        "#[cfg(feature = std)]",
        "#[cfg(feature = \"\")]",
        "#[cfg(all(feature = \"std\",,feature = \"std\"))]",
        "#[cfg(all(feature = \"std\"]",
        "#[cfg(feature = \"std\" \"x\")]",
    ] {
        assert_eq!(
            registered_with_manifest(features, &gated_item(gate))?,
            0,
            "{gate}"
        );
    }
    // An inner gate that is not provably true still gates the file.
    for inner in [
        "#![cfg(feature = \"extra\")]",
        "#![cfg(not(feature = \"std\"))]",
        "#![cfg_attr(feature = \"std\", cfg(feature = \"std\"))]",
    ] {
        assert_eq!(
            registered_with_manifest(features, &format!("{inner}\n{WIRE}"))?,
            0,
            "{inner}"
        );
    }
    Ok(())
}

#[test]
fn only_the_manifest_default_closure_counts_as_enabled() -> Result<(), Box<dyn Error>> {
    let gate = gated_item("#[cfg(feature = \"std\")]");
    // No `[features]` table, or none that names the feature.
    assert_eq!(registered_with_manifest("", &gate)?, 0);
    assert_eq!(
        registered_with_manifest("\n[features]\nstd = []\n", &gate)?,
        0,
        "declared, but there is no `default` list"
    );
    assert_eq!(
        registered_with_manifest("\n[features]\ndefault = []\nstd = []\n", &gate)?,
        0
    );
    // `dep:` and `name/feature` entries add no feature of this package.
    for default in ["\"dep:std\"", "\"std/extra\"", "\"std?/extra\""] {
        assert_eq!(
            registered_with_manifest(
                &format!("\n[features]\ndefault = [{default}]\nstd = []\n"),
                &gate
            )?,
            0,
            "{default}"
        );
    }
    // A value the reader cannot follow resolves nothing.
    for manifest in [
        "\n[features]\ndefault = \"std\"\nstd = []\n",
        "\n[features]\ndefault = [1]\nstd = []\n",
        "\n[features]\ndefault = [\"mid\"]\nmid = \"std\"\nstd = []\n",
    ] {
        assert_eq!(registered_with_manifest(manifest, &gate)?, 0, "{manifest}");
    }
    // A cyclic feature list terminates and still resolves what it reaches.
    assert_eq!(
        registered_with_manifest(
            "\n[features]\ndefault = [\"a\"]\na = [\"b\"]\nb = [\"a\", \"std\"]\nstd = []\n",
            &gate
        )?,
        1
    );
    Ok(())
}

#[test]
fn gate_resolution_answers_only_provably_true() {
    use super::gate_is_provably_true;
    use std::collections::BTreeSet;
    let defaults: BTreeSet<String> = ["a", "b"].iter().map(|name| name.to_string()).collect();
    for (predicate, expected) in [
        ("feature = \"a\"", true),
        ("feature=\"a\"", true),
        (" feature = \"b\" ", true),
        ("doctest", true),
        ("all(feature = \"a\", feature = \"b\")", true),
        ("all(feature = \"a\", feature = \"b\",)", true),
        ("any(feature = \"c\", feature = \"a\")", true),
        ("all(any(feature = \"c\", feature = \"a\"), doctest)", true),
        ("feature = \"c\"", false),
        ("feature = \"\"", false),
        ("feature = a", false),
        ("Feature = \"a\"", false),
        ("all()", false),
        ("any()", false),
        ("not(feature = \"c\")", false),
        ("all(feature = \"a\", not(feature = \"c\"))", false),
        ("all(feature = \"a\", feature = \"c\")", false),
        ("target_os = \"linux\"", false),
        ("test", false),
        ("all(feature = \"a\"", false),
        ("all(feature = \"a\"))", false),
        ("all(feature = \"a\" feature = \"b\")", false),
        ("all(,feature = \"a\")", false),
        ("any(feature = \"a\", ,)", false),
        ("feature = \"a\\\"\"", false),
        ("", false),
    ] {
        assert_eq!(
            gate_is_provably_true(predicate, &defaults),
            expected,
            "{predicate}"
        );
    }
}
