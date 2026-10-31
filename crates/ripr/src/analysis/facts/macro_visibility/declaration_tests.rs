use super::*;

const GENERATOR: &str = "macro_rules! t { ($n:ident) => { #[test] fn $n() {} }; }\n";

fn visibility(files: &[(String, String)]) -> MacroVisibility {
    MacroVisibility::new(
        files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), Arc::<str>::from(text.as_str()))),
    )
}

fn scope_parses() -> usize {
    SCOPE_PARSES.with(std::cell::Cell::get)
}

/// The whole-workspace index the lookup replaced: every file's scope, every
/// non-plain module declaration keyed by name.
fn reference_index(visibility: &MacroVisibility) -> BTreeMap<String, BTreeSet<PathBuf>> {
    let mut index = BTreeMap::<String, BTreeSet<PathBuf>>::new();
    for path in visibility.sources.keys() {
        let Some(scope) = visibility.scope(path) else {
            continue;
        };
        for module in scope.modules.iter().filter(|module| !module.plain) {
            index
                .entry(module.name.clone())
                .or_default()
                .insert(path.clone());
        }
    }
    index
}

fn filler(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|n| {
            (
                format!("src/filler{n}.rs"),
                format!(
                    "#[derive(Debug)]\npub struct S{n};\n#[cfg(test)]\nmod tests {{ #[test] fn a() {{}} }}\n"
                ),
            )
        })
        .collect()
}

#[test]
fn resolving_one_file_parses_only_files_that_can_declare_its_modules() {
    let mut files = vec![
        (
            "src/lib.rs".to_string(),
            "#[macro_use]\nmod macros;\nmod cases;\n".to_string(),
        ),
        ("src/macros.rs".to_string(), GENERATOR.to_string()),
        ("src/cases.rs".to_string(), "t!(a);\n".to_string()),
    ];
    files.extend(filler(40));
    // Plain declarations of the same name carry no attribute to look up.
    files.extend((0..10).map(|n| {
        (
            format!("src/sub{n}.rs"),
            "mod cases;\n#[test]\nfn t() {}\n".to_string(),
        )
    }));
    let visibility = visibility(&files);
    let before = scope_parses();
    let imported = visibility.imported_macros(Path::new("src/cases.rs"));
    let parsed = scope_parses() - before;
    assert!(imported.definitions.contains("macro_rules! t "));
    // cases.rs (own scope), lib.rs (the declarer and its attributed check).
    // None of the filler files can spell an attributed `mod cases`.
    assert_eq!(parsed, 3, "parsed {parsed} scopes");
    // A second ask for the same file re-parses nothing.
    let again = scope_parses();
    let _ = visibility.imported_macros(Path::new("src/cases.rs"));
    assert_eq!(scope_parses(), again);
}

#[test]
fn the_lookup_matches_the_whole_workspace_index() {
    let attributed = [
        "#[path = \"x.rs\"]\nmod cases;\n",
        "#[cfg(unix)] mod cases;\n",
        "#[path = \"x.rs\"] mod /* c */ cases;\n",
        "#[path = \"x.rs\"] mod // c\n cases;\n",
        "#[path = \"x.rs\"] pub mod\tcases ;\n",
        "#[cfg(test)]\nmod cases;\n",
        "#[cfg(test)] #[path = \"x.rs\"] mod cases;\n",
        "#[path = \"x.rs\"] #[cfg(test)] mod cases;\n",
        "#[macro_use] #[allow(unused)] pub(crate) mod cases;\n",
        "#[ cfg ( test ) ]\nmod cases;\n",
        "#![allow(unused)]\nmod cases;\n",
        "#[doc = \"a]b\"] #[cfg(feature = \"x\")] mod cases;\n",
        "#[path = \"x.rs\"] pub(crate) mod cases;\n",
        "#[path = \"x.rs\"]\n/// d\nmod cases;\n",
        "#[path = \"x.rs\"] pub (in crate) mod cases;\n",
        "#[path = \"x.rs\"]\nmod r#cases;\n",
        "#[path = \"x.rs\"] mod\u{200e}cases;\n",
        "#[path = \"x.rs\"] mod cases_more;\n",
        "#[path = \"x.rs\"] mod not_cases;\n",
        "mod cases;\n",
        "#[macro_use] mod cases;\n",
        "#[path = \"x.rs\"] mod cases { }\n",
        "fn f() { let cases = 1; }\n",
        "// #[path] mod cases;\n",
        "",
    ];
    for (n, text) in attributed.iter().enumerate() {
        let files = vec![
            (
                "src/lib.rs".to_string(),
                "#[macro_use]\nmod macros;\nmod cases;\n".to_string(),
            ),
            ("src/macros.rs".to_string(), GENERATOR.to_string()),
            ("src/cases.rs".to_string(), "t!(a);\n".to_string()),
            ("src/other.rs".to_string(), (*text).to_string()),
        ];
        let visibility = visibility(&files);
        let reference = reference_index(&visibility);
        for name in ["cases", "macros", "other", "nope"] {
            for file in ["src/lib.rs", "src/other.rs", "src/cases.rs"] {
                let expected = reference
                    .get(name)
                    .is_some_and(|set| set.iter().any(|candidate| candidate != Path::new(file)));
                assert_eq!(
                    visibility.attributed_elsewhere(name, Path::new(file)),
                    expected,
                    "case {n} {text:?} name {name} file {file}"
                );
            }
        }
        // End to end: a fresh instance resolves exactly as the reference
        // index would allow.
        let fresh = self::visibility(&files);
        let ambiguous = reference.get("cases").is_some_and(|set| {
            set.iter()
                .any(|candidate| candidate != Path::new("src/cases.rs"))
        });
        let imported = fresh.imported_macros(Path::new("src/cases.rs"));
        assert_eq!(
            imported.definitions.is_empty(),
            ambiguous,
            "case {n} {text:?}"
        );
    }
}

#[test]
fn the_text_gate_never_hides_a_declaration_the_parser_lists() {
    let mut checked = 0;
    for name in ["cases", "é", "r#type", "x"] {
        let bare = name.strip_prefix("r#").unwrap_or(name);
        for gap in [
            " ",
            "\t",
            "\n",
            "\r\n",
            "/* c */",
            " /* c */ ",
            "// c\n",
            " // c\n // d\n",
            "\u{0085}",
            "\u{200e}",
            "\u{2028}",
            "\u{b}",
            " \u{b} ",
        ] {
            for prefix in [
                "",
                "pub ",
                "pub(crate) ",
                "pub (in crate::a) ",
                "/// doc\n",
                "// c\n",
                "/* c */ pub ",
                "unsafe ",
                "pub /* c */ ",
                "pub(in /* ( */ crate) ",
                "pub\u{b}(crate) ",
                "#[cfg(test)] ",
                "#[macro_use] pub ",
                "#[allow(unused)]\n",
            ] {
                for spelled in [name, bare] {
                    let source = format!("#[path = \"p.rs\"] {prefix}mod{gap}{spelled};\n");
                    let Some(scope) = macro_scope(&source) else {
                        // Only a keyword name or `unsafe mod` fails to parse.
                        assert!(
                            spelled == "type" || prefix == "unsafe ",
                            "unexpected parse failure: {source:?}"
                        );
                        continue;
                    };
                    for module in scope.modules.iter().filter(|module| !module.plain) {
                        for attributed in [true, false] {
                            assert!(
                                may_declare_module_named(&source, &module.name, attributed),
                                "false negative for {:?} in {source:?}",
                                module.name
                            );
                        }
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 100, "only {checked} declarations were parsed");
    // A plain or unattributed declaration is a maybe only when not asked for
    // an attribute.
    for source in [
        "mod cases;",
        "pub mod cases;",
        "pub(crate) mod cases;",
        "#[derive(Debug)] struct S; mod cases;",
        "fn f() {}\nmod cases;",
        "#[cfg(test)] mod cases;",
        "#[cfg(test)]\n#[allow(unused)] pub(crate) mod cases;",
        "#[ macro_use ] mod cases;",
        "#[doc(hidden)] mod cases;",
    ] {
        assert!(
            may_declare_module_named(source, "cases", false),
            "{source:?}"
        );
        assert!(
            !may_declare_module_named(source, "cases", true),
            "{source:?}"
        );
    }
    for source in [
        "mod cases_more;",
        "mod not_cases;",
        "fn cases() {}",
        "let cases = 1;",
        "use cases;",
        "mod other; fn cases() {}",
        "/// Resolves the cases of the run.\nfn f() {}\n",
        "let url = \"http://x\"; // the cases\n",
        "struct S {\n    a: u8, // the first\n    b: u8, // cases\n}\n",
    ] {
        assert!(
            !may_declare_module_named(source, "cases", false),
            "{source:?}"
        );
    }
}

#[test]
fn the_attributed_lookup_stops_at_the_second_declarer() {
    let mut files = vec![("src/cases.rs".to_string(), "t!(a);\n".to_string())];
    for n in 0..8 {
        files.push((
            format!("src/decl{n}.rs"),
            "#[cfg(unix)]\nmod cases;\n".to_string(),
        ));
    }
    let visibility = visibility(&files);
    let before = scope_parses();
    assert!(visibility.attributed_elsewhere("cases", Path::new("src/decl0.rs")));
    assert_eq!(scope_parses() - before, 2);
    // Every file is told about a declarer other than itself, from the memo.
    for n in 0..8 {
        let path = format!("src/decl{n}.rs");
        assert!(visibility.attributed_elsewhere("cases", Path::new(&path)));
    }
    assert_eq!(scope_parses() - before, 2);
    // A lone declarer is not "elsewhere" for itself.
    let lone = self::visibility(&files[..2]);
    assert!(!lone.attributed_elsewhere("cases", Path::new("src/decl0.rs")));
    assert!(lone.attributed_elsewhere("cases", Path::new("src/cases.rs")));
}

#[test]
fn the_text_gate_keeps_every_contrived_attributed_declaration() {
    let vt = "\u{b}";
    let sources = [
        "#[path = \"x.rs\"] pub /* c */ (crate) mod cases;\n".to_string(),
        "#[path = \"x.rs\"] pub // c\n (crate) mod cases;\n".to_string(),
        "#[path = \"x.rs\"] pub(in /* ( */ crate) mod cases;\n".to_string(),
        "#[path = \"x.rs\"] pub(in crate /* ) */) mod cases;\n".to_string(),
        "#[path = \"x.rs\"] pub (in crate) mod cases;\n".to_string(),
        format!("#[path = \"x.rs\"] mod{vt}cases;\n"),
        format!("#[path = \"x.rs\"]{vt}mod cases;\n"),
        format!("#[path = \"x.rs\"] pub{vt}(crate){vt}mod{vt}cases;\n"),
        "#[path = \"x.rs\"]\nmod cases;\n".to_string(),
        "#[cfg(unix)] mod cases;\n".to_string(),
        "#[cfg_attr(unix, path = \"x.rs\")] mod cases;\n".to_string(),
        "#[doc = \"]\"] #[path = \"x.rs\"] mod cases;\n".to_string(),
        "#[path = \"a[0]\"] mod cases;\n".to_string(),
        "#[path =\n    \"x.rs\"\n] mod cases;\n".to_string(),
        "# [path = \"x.rs\"] mod cases;\n".to_string(),
        "#[ path = \"x.rs\" ]\n#[allow(unused)]\nmod cases;\n".to_string(),
        "#[allow(unused)]\n#[path = \"x.rs\"]\nmod cases;\n".to_string(),
    ];
    for source in &sources {
        let listed = macro_scope(source)
            .map(|scope| {
                scope
                    .modules
                    .iter()
                    .any(|module| !module.plain && module.name == "cases")
            })
            .unwrap_or(false);
        assert!(listed, "parser premise: {source:?}");
        for attributed in [true, false] {
            assert!(
                may_declare_module_named(source, "cases", attributed),
                "false negative ({attributed}): {source:?}"
            );
        }
    }
}
