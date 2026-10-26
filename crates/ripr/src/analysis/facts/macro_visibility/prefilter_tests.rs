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
        let names: BTreeSet<String> = scope.into_iter().flat_map(|s| s.macros.into_iter().map(|m| m.name)).collect();
        assert_eq!(names, BTreeSet::from([name.to_string()]));
        for path in ["", "crate::", "crate /* path */ ::", "other::"] {
            for gap in ["", " ", "\t", "\r\n", "\u{0085}", "\u{200e}", "\u{200f}", "\u{2028}", "\u{2029}", "/* c */", "/* outer /* inner */ tail */", "// c\n"] {
                let call = format!("{path}{name}{gap}!();");
                for source in [call.clone(), format!("mod m {{ {call} }}"), format!("fn f() {{ {call} }}"), format!("outer! {{ {call} }}")] {
                    assert!(first_macro_call(&source, &names).is_some(), "parser premise: {source:?}");
                    assert!(may_call_generator(&source, &names), "false negative: {source:?}");
                    checked += 1;
                }
            }
        }
        for before in ["\u{0085}", "\u{200e}", "\u{200f}", "\u{2028}", "\u{2029}"] {
            let source = format!("{before}{name}!();");
            assert!(first_macro_call(&source, &names).is_some(), "parser premise: {source:?}");
            assert!(may_call_generator(&source, &names), "false negative: {source:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 985);
}

#[test]
fn identifier_lookalikes_and_inert_spellings_are_not_parser_calls() {
    let names = BTreeSet::from(["t".to_string()]);
    for source in ["fn t() {}", "fn f() { let t = 1; }", "at!();", "t_!();", "t1!();", "ét!();", "tét!();", "outer!(t);", "/* t!() */", "const S: &str = \"t!()\";", "macro_rules! other { () => { t!(); }; }"] {
        assert_eq!(first_macro_call(source, &names), None, "{source:?}");
    }
}
