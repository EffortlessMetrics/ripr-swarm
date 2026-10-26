use super::*;

#[test]
fn overlapping_raw_names_do_not_hide_a_parser_established_call() {
    let names = BTreeSet::from(["r#r".to_string()]);
    let source = "outer! { r#ar#r#r!(); }";
    assert_eq!(first_macro_call(source, &names), Some(14));
    assert!(may_call_generator(source, &names));
}

#[test]
fn lexical_matrix_keeps_every_parser_established_call() {
    let mut checked = 0;
    for name in ["t", "test_table", "é", "r#type", "r#r"] {
        let definition = format!("macro_rules! {name} {{ () => {{ #[test] fn a() {{}} }}; }}");
        let scope = macro_scope(&definition);
        assert!(scope.is_some(), "{definition}");
        let names: BTreeSet<String> = scope
            .into_iter()
            .flat_map(|s| s.macros.into_iter().map(|m| m.name))
            .collect();
        assert_eq!(names, BTreeSet::from([name.to_string()]));
        for path in ["", "crate::", "crate /* path */ ::", "other::"] {
            for gap in [
                "",
                " ",
                "\t",
                "\r\n",
                "\u{0085}",
                "\u{200e}",
                "\u{200f}",
                "\u{2028}",
                "\u{2029}",
                "/* c */",
                "/* outer /* inner */ tail */",
                "// c\n",
            ] {
                let call = format!("{path}{name}{gap}!();");
                for source in [
                    call.clone(),
                    format!("mod m {{ {call} }}"),
                    format!("fn f() {{ {call} }}"),
                    format!("outer! {{ {call} }}"),
                ] {
                    assert!(
                        first_macro_call(&source, &names).is_some(),
                        "parser premise: {source:?}"
                    );
                    assert!(
                        may_call_generator(&source, &names),
                        "false negative: {source:?}"
                    );
                    checked += 1;
                }
            }
        }
        for before in ["\u{0085}", "\u{200e}", "\u{200f}", "\u{2028}", "\u{2029}"] {
            let source = format!("{before}{name}!();");
            assert!(
                first_macro_call(&source, &names).is_some(),
                "parser premise: {source:?}"
            );
            assert!(
                may_call_generator(&source, &names),
                "false negative: {source:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 985);
}

#[test]
fn identifier_lookalikes_and_inert_spellings_are_not_parser_calls() {
    let names = BTreeSet::from(["t".to_string()]);
    for source in [
        "fn t() {}",
        "fn f() { let t = 1; }",
        "at!();",
        "t_!();",
        "t1!();",
        "ét!();",
        "tét!();",
        "outer!(t);",
        "/* t!() */",
        "const S: &str = \"t!()\";",
        "macro_rules! other { () => { t!(); }; }",
    ] {
        assert_eq!(first_macro_call(source, &names), None, "{source:?}");
    }
}
fn unfiltered(visibility: &MacroVisibility, file: &Path) -> ImportedMacros {
    let Some(source) = visibility.sources.get(file) else {
        return ImportedMacros::default();
    };
    let Some(first_call) = first_macro_call(source, &visibility.generators) else {
        return ImportedMacros::default();
    };
    visibility.resolve_imported_macros(file, source, first_call)
}

fn from_sources(files: &[(&str, &str)]) -> MacroVisibility {
    MacroVisibility::new(
        files
            .iter()
            .map(|(file, source)| (PathBuf::from(file), Arc::from(*source))),
    )
}

#[test]
fn filtered_imports_preserve_unicode_raw_and_token_tree_calls() {
    for (name, source) in [
        ("t", "\u{200e}t!();"),
        ("t", "t\u{0085}!();"),
        ("t", "t /* outer /* inner */ tail */ !();"),
        ("t", "t // comment\n!();"),
        ("r#type", "r#type!();"),
        ("r#r", "outer! { r#ar#r#r!(); }"),
    ] {
        let definition = format!("macro_rules! {name} {{ () => {{ #[test] fn a() {{}} }}; }}");
        let root = format!("{definition}\nmod cases;");
        let v = from_sources(&[("tests/main.rs", &root), ("tests/cases.rs", source)]);
        assert_eq!(v.generators, BTreeSet::from([name.to_string()]));
        assert!(
            first_macro_call(source, &v.generators).is_some(),
            "{source:?}"
        );
        let file = Path::new("tests/cases.rs");
        let actual = v.imported_macros(file);
        assert_eq!(actual.definitions, definition, "{source:?}");
        assert_eq!(actual, unfiltered(&v, file));
    }
}

#[test]
fn the_production_route_skips_parsing_only_for_proven_absence() {
    let root = "macro_rules! t { () => { #[test] fn a() {} }; } mod cases;";
    for (source, expected_parses, expected_import) in [
        ("fn unrelated() {}", 0, false),
        ("at!(); t_!();", 0, false),
        ("/* t!() */", 1, false),
        ("const S: &str = \"t!()\";", 1, false),
        ("t!();", 1, true),
        ("t /* comment */ !();", 1, true),
    ] {
        let v = from_sources(&[("tests/main.rs", root), ("tests/cases.rs", source)]);
        let calls = std::cell::Cell::new(0);
        let file = Path::new("tests/cases.rs");
        let actual = v.imported_macros_with_parser(file, |source, names| {
            calls.set(calls.get() + 1);
            first_macro_call(source, names)
        });
        assert_eq!(calls.get(), expected_parses, "{source:?}");
        assert_eq!(
            !actual.definitions.is_empty(),
            expected_import,
            "{source:?}"
        );
        assert_eq!(actual, v.imported_macros(file));
        assert_eq!(actual, unfiltered(&v, file));
    }
}

#[test]
fn filtered_group_and_path_imports_preserve_the_complete_splice() -> Result<(), String> {
    use crate::analysis::syntax::splice_imported_macros;
    let root = "#[macro_export] macro_rules! t { () => { #[test] fn a() {} }; } const KEEP: u8 = 1; mod cases;";
    for source in ["crate::t!();", "use crate::{KEEP, t}; t!();"] {
        let v = from_sources(&[("src/lib.rs", root), ("src/cases.rs", source)]);
        let file = Path::new("src/cases.rs");
        let actual = v.imported_macros(file);
        assert!(!actual.definitions.is_empty());
        let reference = unfiltered(&v, file);
        assert_eq!(actual, reference);
        let spliced = splice_imported_macros(source, &actual);
        assert_eq!(spliced, splice_imported_macros(source, &reference));
        let spliced = spliced.ok_or_else(|| "missing splice".to_string())?;
        assert!(macro_scope(&spliced).is_some());
        if source.starts_with("use") {
            assert_eq!(actual.path_imports, ["t"]);
            assert!(spliced.contains("use crate::{KEEP,  };"), "{spliced}");
        } else {
            assert_eq!(actual.blanked_prefixes, [(0, 7)]);
            assert!(spliced.ends_with("       t!();"), "{spliced}");
        }
    }
    Ok(())
}

#[test]
fn identical_input_resolver_measurement() {
    // 32 distinct texts exceed the four-entry parse memo. Setup is outside
    // timing; each variant gets its own resolver with equally warm memos.
    let mut sources = vec![(
        PathBuf::from("src/lib.rs"),
        Arc::<str>::from(
            "#[macro_export] macro_rules! t { () => { #[test] fn a() {} }; } mod case0;",
        ),
    )];
    for file in 0..32 {
        let mut source = format!("// distinct input {file}\n");
        for function in 0..64 {
            source.push_str(&format!(
                "fn f{function}(x: u64) -> u64 {{ x + {function} }}\n"
            ));
        }
        if file == 0 {
            source.push_str("crate::t!();\n");
        }
        sources.push((
            PathBuf::from(format!("src/case{file}.rs")),
            Arc::from(source),
        ));
    }
    let filtered = MacroVisibility::new(sources.clone().into_iter());
    let reference = MacroVisibility::new(sources.clone().into_iter());
    let paths: Vec<_> = sources.iter().skip(1).map(|(path, _)| path).collect();
    let run = |with_filter: bool| -> Vec<ImportedMacros> {
        paths
            .iter()
            .map(|path| {
                if with_filter {
                    filtered.imported_macros(path)
                } else {
                    unfiltered(&reference, path)
                }
            })
            .collect()
    };
    let expected = run(false);
    assert_eq!(expected.len(), 32);
    assert_eq!(
        expected
            .iter()
            .filter(|result| !result.definitions.is_empty())
            .count(),
        1
    );
    assert_eq!(run(true), expected);
    let mut filtered_times = Vec::new();
    let mut reference_times = Vec::new();
    for round in 0..6 {
        for with_filter in [round % 2 == 0, round % 2 != 0] {
            let start = std::time::Instant::now();
            let result = std::hint::black_box(run(with_filter));
            let elapsed = start.elapsed();
            assert_eq!(result, expected);
            if with_filter {
                filtered_times.push(elapsed);
            } else {
                reference_times.push(elapsed);
            }
        }
    }
    let bytes: usize = sources.iter().skip(1).map(|(_, source)| source.len()).sum();
    println!(
        "identical inputs: 32 sources, {bytes} bytes, 6 alternating pairs; filtered={filtered_times:?}; unfiltered={reference_times:?}; all resolver fields equal"
    );
}
