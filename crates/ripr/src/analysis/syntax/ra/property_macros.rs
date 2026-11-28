//! Property-macro syntax is a limitation witness, never executable evidence.
//!
//! The name alone cannot establish a macro's expansion: local no-op macros
//! can use the same spelling as proptest and QuickCheck. Keep their bodies
//! opaque and collect only lexical identifier mentions from the existing
//! parse. No file-sized overlay, second parse, functions or tests are created.

use super::LineIndex;
use crate::analysis::facts::UnresolvedPropertyMacroFact;
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, ast};
use std::collections::BTreeSet;

pub(super) fn unresolved_property_macros(
    source: &SourceFile,
    lines: &LineIndex,
) -> Vec<UnresolvedPropertyMacroFact> {
    source
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter_map(|call| {
            // A template inside another opaque token tree is not an invocation.
            if call
                .syntax()
                .ancestors()
                .skip(1)
                .any(|node| node.kind() == SyntaxKind::TOKEN_TREE)
            {
                return None;
            }
            let path = call.path()?;
            let leaf = path.segment()?.name_ref()?;
            if !matches!(
                leaf.text(),
                "proptest" | "quickcheck" | "prop_assert" | "prop_assert_eq" | "prop_assert_ne"
            ) {
                return None;
            }
            let name = path
                .syntax()
                .text()
                .to_string()
                .replace([' ', '\t', '\n'], "");
            let tree = call.token_tree()?;
            let line = lines.line(call.syntax().text_range().start());
            // Read identifier tokens already owned by the first parse. Do not
            // clone the token-tree source or construct affirmative CallFacts:
            // repeated calls on one long line would duplicate that whole line.
            let tokens: Vec<_> = tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
                .collect();
            let kinds: Vec<_> = tokens.iter().map(|token| token.kind()).collect();
            let binding = binding_positions(&kinds);
            let mut mentioned = BTreeSet::new();
            let mut referenced = BTreeSet::new();
            for (position, token) in tokens.iter().enumerate() {
                if token.kind() != SyntaxKind::IDENT {
                    continue;
                }
                let text = token.text().trim_start_matches("r#");
                mentioned.insert(text);
                if !binding[position] {
                    referenced.insert(text);
                }
            }
            let binding_only_identifiers = mentioned
                .difference(&referenced)
                .map(|text| (*text).to_string())
                .collect();
            let mentioned_identifiers = mentioned.into_iter().map(str::to_string).collect();
            Some(UnresolvedPropertyMacroFact {
                name,
                line,
                mentioned_identifiers,
                binding_only_identifiers,
            })
        })
        .collect()
}

/// Marks token positions that only declare a name inside the opaque body:
/// the identifier after `fn`, and each parameter binding of that signature
/// (`x: T` for QuickCheck, `x in strategy` for proptest, with `mut`/`ref`).
/// Such a token names the property or its input, never a callee, so it
/// cannot bind a provider failure to a same-named production owner.
fn binding_positions(kinds: &[SyntaxKind]) -> Vec<bool> {
    let mut binding = vec![false; kinds.len()];
    let mut cursor = 0;
    while cursor < kinds.len() {
        if kinds[cursor] != SyntaxKind::FN_KW {
            cursor += 1;
            continue;
        }
        cursor += 1;
        if kinds.get(cursor) != Some(&SyntaxKind::IDENT) {
            continue;
        }
        binding[cursor] = true;
        cursor += 1;
        // Generic parameters may precede the parameter list; a body or item
        // end before any `(` means there is no signature to read.
        while cursor < kinds.len()
            && !matches!(
                kinds[cursor],
                SyntaxKind::L_PAREN | SyntaxKind::L_CURLY | SyntaxKind::SEMICOLON
            )
        {
            cursor += 1;
        }
        if kinds.get(cursor) != Some(&SyntaxKind::L_PAREN) {
            continue;
        }
        cursor += 1;
        let mut depth = 1usize;
        let mut segment_start = true;
        while cursor < kinds.len() && depth > 0 {
            match kinds[cursor] {
                SyntaxKind::L_PAREN | SyntaxKind::L_BRACK | SyntaxKind::L_CURLY => depth += 1,
                SyntaxKind::R_PAREN | SyntaxKind::R_BRACK | SyntaxKind::R_CURLY => depth -= 1,
                SyntaxKind::COMMA if depth == 1 => {
                    segment_start = true;
                    cursor += 1;
                    continue;
                }
                SyntaxKind::MUT_KW | SyntaxKind::REF_KW if depth == 1 && segment_start => {
                    cursor += 1;
                    continue;
                }
                SyntaxKind::IDENT if depth == 1 && segment_start => {
                    let next = kinds.get(cursor + 1);
                    let single_colon = next == Some(&SyntaxKind::COLON)
                        && kinds.get(cursor + 2) != Some(&SyntaxKind::COLON);
                    if next == Some(&SyntaxKind::IN_KW) || single_colon {
                        binding[cursor] = true;
                    }
                }
                _ => {}
            }
            segment_start = false;
            cursor += 1;
        }
    }
    binding
}

#[cfg(test)]
mod tests {
    use super::super::{
        is_assertion_macro, is_assertion_macro_leaf, parser_oracles_for_function,
        summarize_file_with_parser,
    };
    use std::path::Path;

    #[test]
    fn empty_local_macro_calls_preserve_real_call_text_and_original_source() -> Result<(), String> {
        let source = "fn owner(x: i32) -> i32 { x }\r\nmacro_rules! discard_tokens { ($($ignored:tt)*) => {} }\r\n#[test]\r\nfn boundary() {\r\n discard_tokens!(owner(100), \"日本語 🦀\"); assert_eq!(owner(90), 90);\r\n}\r\n";
        let facts = summarize_file_with_parser(Path::new("src/lib.rs"), source)?;
        assert_eq!(facts.source.as_ref(), source);
        assert_eq!(facts.tests.len(), 1);
        assert!(facts.tests[0].body.contains("discard_tokens!(owner(100)"));
        let calls: Vec<_> = facts.tests[0]
            .calls
            .iter()
            .filter(|call| call.name == "owner")
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].line, 5);
        assert!(!calls[0].text.contains("owner(100)"));
        assert!(calls[0].text.contains("assert_eq!(owner(90), 90)"));
        assert_eq!(facts.tests[0].assertions.len(), 1);
        // A nonempty transcriber is not this denial authority.
        let nonempty = source.replace("=> {}", "=> { let _marker = (); }");
        let facts = summarize_file_with_parser(Path::new("src/lib.rs"), &nonempty)?;
        assert!(
            facts.tests[0]
                .calls
                .iter()
                .any(|call| call.text.contains("owner(100)"))
        );
        Ok(())
    }

    #[test]
    fn parser_raw_oracles_ignore_discarded_property_assertions() -> Result<(), String> {
        let source = "fn owner(x: i32) -> i32 { x }\n#[test]\nfn boundary() {\n let _ = owner(1);\n proptest! { ensure!(owner(1) == 1, \"discarded\"); }\n}\n";
        let facts = summarize_file_with_parser(Path::new("src/lib.rs"), source)?;
        assert_eq!(facts.tests.len(), 1);
        assert!(facts.tests[0].assertions.is_empty());
        let positive = source.replace(
            " proptest! { ensure!(owner(1) == 1, \"discarded\"); }",
            " ensure!(owner(1) == 1, \"ordinary\");",
        );
        let facts = summarize_file_with_parser(Path::new("src/lib.rs"), &positive)?;
        assert_eq!(facts.tests.len(), 1);
        assert!(
            facts.tests[0]
                .assertions
                .iter()
                .any(|oracle| oracle.text.contains("owner(1)"))
        );
        Ok(())
    }

    #[test]
    fn property_macro_names_do_not_grant_assertion_authority() {
        for name in ["prop_assert", "prop_assert_eq", "prop_assert_ne"] {
            assert!(!is_assertion_macro(name));
            assert!(!is_assertion_macro_leaf(name));
            assert!(!is_assertion_macro(&format!("proptest::{name}")));
        }
        assert!(is_assertion_macro("assert_eq"));
        assert!(!is_assertion_macro("other::assert_eq"));
    }

    #[test]
    fn drop_in_assertion_crate_paths_keep_assertion_authority() {
        for name in [
            "pretty_assertions::assert_eq",
            "pretty_assertions::assert_ne",
            "similar_asserts::assert_eq",
            "std::assert_eq",
            "::core::assert",
            "assert_matches::assert_matches",
        ] {
            assert!(is_assertion_macro(name), "{name}");
        }
        for name in [
            "other::assert_eq",
            "pretty_assertions::nested::assert_eq",
            "pretty_assertions::assert_float",
            "proptest::prop_assert_eq",
        ] {
            assert!(!is_assertion_macro(name), "{name}");
        }
    }

    #[test]
    fn qualified_drop_in_assertions_classify_unless_their_root_is_bound_locally()
    -> Result<(), String> {
        use crate::domain::{OracleKind, OracleStrength};
        let test =
            "#[test]\nfn pins() {\n    pretty_assertions::assert_eq!(make(2).value, 2);\n}\n";
        for prefix in [
            "",
            "use pretty_assertions;\n",
            "use {pretty_assertions};\n",
            "use pretty_assertions::{self};\n",
            "extern crate pretty_assertions;\n",
        ] {
            let source = format!("{prefix}{test}");
            let facts = summarize_file_with_parser(Path::new("src/lib.rs"), &source)?;
            assert_eq!(facts.tests.len(), 1, "{source}");
            let [assertion] = facts.tests[0].assertions.as_slice() else {
                return Err(format!("expected one assertion in {source}"));
            };
            assert_eq!(assertion.kind, OracleKind::ExactValue, "{source}");
            assert_eq!(assertion.strength, OracleStrength::Strong, "{source}");
        }
        // A local module, import or crate rename can bind the root to a
        // macro that does not compare, so the call earns no assertion.
        for prefix in [
            "mod pretty_assertions { pub use crate::noop as assert_eq; }\n",
            "mod tests { mod pretty_assertions {} }\n",
            "use crate::helpers as pretty_assertions;\n",
            "use crate::helpers::pretty_assertions;\n",
            "use crate::{helpers::r#pretty_assertions};\n",
            "extern crate my_macros as pretty_assertions;\n",
            "use crate::helpers::pretty_assertions::{self};\n",
            "use crate::{pretty_assertions};\n",
            // A glob may import a same-named module from another file (#7641).
            "use helpers::*;\n",
            // An item-position macro may emit such a module.
            "my_macros::emit_mod!();\n",
            "mod tests { shim!(); }\n",
        ] {
            let source = format!("{prefix}{test}");
            let facts = summarize_file_with_parser(Path::new("src/lib.rs"), &source)?;
            assert_eq!(facts.tests.len(), 1, "{source}");
            assert!(facts.tests[0].assertions.is_empty(), "{source}");
        }
        // A promoted function is re-parsed alone, so its file's bindings
        // are not visible and a qualified drop-in fails closed there.
        let qualified = "fn pins() {\n    pretty_assertions::assert_eq!(make(2).value, 2);\n}\n";
        assert_eq!(parser_oracles_for_function(qualified, 1), Some(Vec::new()));
        let bare = qualified.replace("pretty_assertions::", "");
        assert_eq!(
            parser_oracles_for_function(&bare, 1).map(|oracles| oracles.len()),
            Some(1)
        );
        Ok(())
    }

    #[test]
    fn opaque_property_blocks_keep_mentions_without_inventing_tests() -> Result<(), String> {
        for macro_name in ["proptest", "quickcheck", "other::proptest"] {
            for attribute in [
                "#[test]",
                "#[doc = \"quickcheck\"]",
                "#[cfg(feature = \"quickcheck\")]",
            ] {
                let source = format!(
                    "fn gate(x: u32) -> bool {{ x > 10 }}\n{macro_name}! {{\n {attribute}\n fn hidden(x: u32) -> bool {{ gate(x) }}\n}}\n"
                );
                let facts = summarize_file_with_parser(Path::new("src/lib.rs"), &source)?;
                assert_eq!(facts.functions.len(), 1);
                assert_eq!(facts.functions[0].name, "gate");
                assert!(facts.tests.is_empty());
                assert_eq!(facts.unresolved_property_macros.len(), 1);
                let witness = &facts.unresolved_property_macros[0];
                assert_eq!(witness.name, macro_name);
                assert_eq!(witness.line, 2);
                assert!(
                    witness
                        .mentioned_identifiers
                        .iter()
                        .any(|call| call == "gate")
                );
            }
        }
        Ok(())
    }

    #[test]
    fn property_declaration_and_parameter_names_are_binding_only() -> Result<(), String> {
        let source = "proptest! {\n #[test] fn discounted_total<T>(mut amount in 0..10u32, ref limit: u32, (pair, other) in any()) {\n final_price(limit, pair);\n }\n}\nquickcheck! {\n fn checks(x: u32) -> bool { discounted_total(x) == x }\n}\n";
        let facts = summarize_file_with_parser(Path::new("tests/property.rs"), source)?;
        assert_eq!(facts.unresolved_property_macros.len(), 2);
        let proptest = &facts.unresolved_property_macros[0];
        for name in ["discounted_total", "amount", "final_price", "limit", "pair"] {
            assert!(
                proptest.mentioned_identifiers.iter().any(|m| m == name),
                "{name}"
            );
        }
        // `limit` and the tuple element are used in the body; only the
        // property name and the unused `amount` binding are binding-only.
        assert_eq!(
            proptest.binding_only_identifiers,
            vec!["amount".to_string(), "discounted_total".to_string()]
        );
        // A call to the same spelling in another invocation is a reference.
        let quickcheck = &facts.unresolved_property_macros[1];
        assert_eq!(
            quickcheck.binding_only_identifiers,
            vec!["checks".to_string()]
        );
        Ok(())
    }

    #[test]
    fn opaque_mentions_ignore_comments_strings_and_identifier_substrings() -> Result<(), String> {
        let source = r#"proptest! {
            #[test] fn hidden() {
                let sample = "gate(10)"; // gate(20)
                /* gate(30) */
                other::gate_suffix(40);
            }
        }"#;
        let facts = summarize_file_with_parser(Path::new("src/lib.rs"), source)?;
        assert_eq!(facts.unresolved_property_macros.len(), 1);
        let identifiers = &facts.unresolved_property_macros[0].mentioned_identifiers;
        assert!(!identifiers.iter().any(|name| name == "gate"));
        assert!(identifiers.iter().any(|name| name == "gate_suffix"));
        assert!(facts.tests.is_empty());
        Ok(())
    }

    #[test]
    fn ordinary_files_and_macro_lookalikes_have_no_property_witness() -> Result<(), String> {
        for source in [
            "#[test] fn ordinary() { assert_eq!(1, 1); }",
            "// proptest! { fn fake() { gate(1); } }\nfn gate(x:u32)->u32{x}",
            "const TEXT: &str = \"quickcheck! { fn fake() { gate(1); } }\";",
            "my_proptest! { #[test] fn fake() { gate(1); } }",
            "macro_rules! wrapper { () => { proptest! { #[test] fn fake() { gate(1); } } } }",
        ] {
            let facts = summarize_file_with_parser(Path::new("src/lib.rs"), source)?;
            assert!(facts.unresolved_property_macros.is_empty());
        }
        Ok(())
    }
}
