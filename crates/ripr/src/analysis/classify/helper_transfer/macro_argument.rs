//! Helper-chain edges through a local macro argument (#6614,
//! RIPR-SPEC-0117 boundary).
//!
//! `fn doubled_rate() -> u32 { twice!(base_rate) }` never calls
//! `base_rate` itself: the owner reaches the wrapper only as an identifier
//! token handed to a local `macro_rules!` macro. No call fact names the
//! owner, so the bounded chain found no caller and a test pinning
//! `doubled_rate()` related to the owner only by file proximity, which read
//! as an actionable `weakly_exposed` gap the analyzer never established.
//!
//! This module names that edge so the chain resolver can record it as a
//! hop marked [`super::HelperHop::through_macro`]. ripr does not expand
//! macros, so such a hop never forwards the owner's result, binds no
//! activation rows and never pairs a boundary: it only lets the relation
//! stage see the wrapper and the propagation stage abstain
//! (`helper_result_not_forwarded`), the same decision that already governs a
//! call reaching the owner inside `format!`.
//!
//! The edge is syntax-backed and fails closed (no edge, the prior evidence
//! stands) whenever it cannot be established:
//!
//! - the caller is a production function whose text parses cleanly;
//! - the owner name occurs as an `IDENT` token inside the invocation's
//!   token tree (any delimiter), not preceded by `.` or a path separator and
//!   not followed by `!`; string-literal and comment contents are separate
//!   tokens and never match;
//! - the invoked macro is a bare name with exactly one `macro_rules!`
//!   definition in the index, and every source that mentions
//!   `macro_rules` parses cleanly so no other definition can hide;
//! - exactly one function and one invocation site carry such a token.

use crate::analysis::facts::{FunctionSummary, RustIndex};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, T};

/// The unique production function passing `callee_name` as an identifier
/// argument to a local `macro_rules!` macro, with that invocation's text.
pub(super) fn macro_argument_caller<'a>(
    callee_name: &str,
    index: &'a RustIndex,
) -> Option<(&'a FunctionSummary, String)> {
    if callee_name.is_empty() {
        return None;
    }
    let mut found: Option<(&'a FunctionSummary, String, String)> = None;
    for function in index.functions().iter() {
        if function.name == callee_name
            || function.source_role.is_evidence_role()
            || !function.body.contains(callee_name)
        {
            continue;
        }
        let Some(parse) = parse_clean_source_file(&function.body) else {
            continue;
        };
        for (macro_name, text) in invocations_naming(parse.tree().syntax(), callee_name) {
            if found.is_some() {
                return None;
            }
            found = Some((function, macro_name, text));
        }
    }
    let (caller, macro_name, text) = found?;
    unique_local_macro_rules(&macro_name, index).then_some((caller, text))
}

/// Every macro invocation under `root` whose token tree carries
/// `callee_name` as a bare identifier token: `(macro name, invocation
/// text)`. An invocation whose path is not a single bare name yields its
/// own entry with an empty name, so the caller's single-site rule still
/// counts it and the unresolvable macro fails closed.
fn invocations_naming(root: &SyntaxNode, callee_name: &str) -> Vec<(String, String)> {
    root.descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| {
            call.token_tree()
                .is_some_and(|tree| token_tree_names(tree.syntax(), callee_name))
        })
        .map(|call| {
            let name = call
                .path()
                .filter(|path| path.qualifier().is_none() && path.coloncolon_token().is_none())
                .and_then(|path| path.segment()?.name_ref())
                .map(|name| name.text().to_string())
                .unwrap_or_default();
            (name, call.syntax().text().to_string())
        })
        .collect()
}

/// Whether `tree` holds `name` as a bare identifier token: not a field or
/// method (`.name`), not a path segment after `::`, not a macro name
/// (`name!`). Literal and comment tokens are never `IDENT`.
fn token_tree_names(tree: &SyntaxNode, name: &str) -> bool {
    let tokens: Vec<_> = tree
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.iter().enumerate().any(|(at, token)| {
        token.kind() == SyntaxKind::IDENT
            && token.text() == name
            && !at
                .checked_sub(1)
                .and_then(|before| tokens.get(before))
                .is_some_and(|before| matches!(before.kind(), T![.] | T![:] | T![::]))
            && !tokens
                .get(at + 1)
                .is_some_and(|after| after.kind() == T![!])
    })
}

/// Whether `macro_name` has exactly one `macro_rules!` definition across
/// the index. A source that mentions `macro_rules` but does not parse
/// cleanly could hide another definition, so it refuses.
fn unique_local_macro_rules(macro_name: &str, index: &RustIndex) -> bool {
    if macro_name.is_empty() {
        return false;
    }
    let mut definitions = 0usize;
    for file in index.files().values() {
        let source = &file.data().source;
        if !source.contains("macro_rules") || !source.contains(macro_name) {
            continue;
        }
        let Some(parse) = parse_clean_source_file(source) else {
            return false;
        };
        definitions += parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::MacroRules::cast)
            .filter(|rules| rules.name().is_some_and(|name| name.text() == macro_name))
            .count();
    }
    definitions == 1
}

/// Whether some related test exactly pins a caller whose reach rests on a
/// macro-argument hop: an `assert_eq!` (exact value or whole object) with an
/// operand that is a complete bare call of one of `callers`
/// (`assert_eq!(doubled_rate(), 14)`). Only such a pin makes a gap claim
/// unsupported; a projected (`doubled_rate().to_string()`), compared
/// (`assert!(doubled_rate() > 0)`) or unrelated assertion keeps the existing
/// evidence, because the oracle's own weakness is established without
/// expanding the macro.
pub(in crate::analysis::classify) fn exactly_pins_a_caller(
    callers: &[&str],
    related_tests: &[(
        &crate::analysis::facts::TestSummary,
        crate::domain::RelationReason,
    )],
) -> bool {
    use crate::domain::{OracleKind, RelationReason};
    related_tests
        .iter()
        .filter(|(_, reason)| *reason == RelationReason::HelperOwnerCall)
        .flat_map(|(test, _)| test.assertions.iter())
        .filter(|assertion| {
            matches!(
                assertion.kind,
                OracleKind::ExactValue | OracleKind::WholeObjectEquality
            )
        })
        .any(|assertion| {
            assert_eq_bare_callees(&assertion.text)
                .iter()
                .any(|callee| callers.contains(&callee.as_str()))
        })
}

/// Bare callee names of the first two operands of a plain `assert_eq!`
/// that are each a complete call (`banner()`, `rate(3)`). A projected call
/// (`banner().len()`), a path call (`m::banner()`), or any other assertion
/// macro names nothing.
fn assert_eq_bare_callees(text: &str) -> Vec<String> {
    let source = format!("fn __assertion() {{ {} }}", text.trim());
    let Some(parse) = parse_clean_source_file(&source) else {
        return Vec::new();
    };
    let Some(call) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::MacroCall::cast)
    else {
        return Vec::new();
    };
    if call
        .path()
        .map(|path| path.syntax().text().to_string())
        .as_deref()
        != Some("assert_eq")
    {
        return Vec::new();
    }
    let Some(tree) = call.token_tree() else {
        return Vec::new();
    };
    let tokens = tree.syntax().text().to_string();
    let Some(inner) = tokens
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return Vec::new();
    };
    let source = format!("fn __operands() {{ ({inner},) }}");
    let Some(parse) = parse_clean_source_file(&source) else {
        return Vec::new();
    };
    let Some(tuple) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::TupleExpr::cast)
    else {
        return Vec::new();
    };
    tuple
        .fields()
        .take(2)
        .filter_map(|operand| {
            let ast::Expr::CallExpr(call) = operand else {
                return None;
            };
            let ast::Expr::PathExpr(path) = call.expr()? else {
                return None;
            };
            let path = path.path()?;
            if path.qualifier().is_some() || path.coloncolon_token().is_some() {
                return None;
            }
            Some(path.segment()?.name_ref()?.text().to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{assert_eq_bare_callees, invocations_naming};
    use crate::analysis::syntax::parse_clean_source_file;
    use ra_ap_syntax::AstNode;

    fn names(body: &str, owner: &str) -> Vec<String> {
        let Some(parse) = parse_clean_source_file(body) else {
            return vec!["<parse failed>".to_string()];
        };
        invocations_naming(parse.tree().syntax(), owner)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    #[test]
    fn owner_identifier_token_in_any_delimiter_names_the_macro() {
        for body in [
            "fn doubled_rate() -> u32 { twice!(base_rate) }",
            "fn doubled_rate() -> u32 { twice![base_rate] }",
            "fn doubled_rate() -> u32 { twice! { base_rate } }",
            "fn doubled_rate() -> u32 { let x = 1; twice!(x, base_rate) }",
        ] {
            assert_eq!(names(body, "base_rate"), ["twice"], "{body}");
        }
    }

    #[test]
    fn string_comment_field_path_and_macro_name_tokens_never_match() {
        for body in [
            "fn doubled_rate() -> u32 { twice!(\"base_rate\") }",
            "fn doubled_rate() -> u32 { twice!(other /* base_rate */) }",
            "fn doubled_rate() -> u32 { twice!(self.base_rate) }",
            "fn doubled_rate() -> u32 { twice!(rates::base_rate) }",
            "fn doubled_rate() -> u32 { twice!(base_rate!()) }",
            "fn doubled_rate() -> u32 { twice!(other_rate) }",
            "fn doubled_rate() -> u32 { twice!(base_rates) }",
            "fn doubled_rate() -> u32 { base_rate + 1 }",
        ] {
            assert!(names(body, "base_rate").is_empty(), "{body}");
        }
    }

    #[test]
    fn qualified_macro_path_yields_an_unresolvable_name() {
        assert_eq!(
            names(
                "fn doubled_rate() -> u32 { m::twice!(base_rate) }",
                "base_rate"
            ),
            [""]
        );
    }

    #[test]
    fn only_a_complete_bare_call_operand_of_assert_eq_is_a_pin() {
        assert_eq!(
            assert_eq_bare_callees("assert_eq!(doubled_rate(), 14);"),
            ["doubled_rate"]
        );
        assert_eq!(
            assert_eq_bare_callees("assert_eq!(14, rate(2), \"msg\");"),
            ["rate"]
        );
        for text in [
            "assert_eq!(banner().len(), 6);",
            "assert_eq!(m::banner(), \"[x]\");",
            "assert_ne!(banner(), \"[x]\");",
            "assert!(doubled_rate() > 0);",
            "assert!(banner() == \"[x]\");",
            "assert_eq!(\"doubled_rate()\", x);",
        ] {
            assert!(assert_eq_bare_callees(text).is_empty(), "{text}");
        }
    }

    fn chain_for(source: &str) -> Result<super::super::HelperChain, String> {
        let path = std::path::PathBuf::from("src/lib.rs");
        let facts = crate::analysis::syntax::ra::summarize_file_with_parser(&path, source)?;
        let index =
            crate::analysis::facts::RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
                functions: facts.functions.clone(),
                tests: facts.tests.clone(),
                files: [(path, facts)].into_iter().collect(),
                ..Default::default()
            });
        Ok(super::super::resolve_chain("base_rate", &index, true, &[]))
    }

    const RULE: &str = "macro_rules! twice { ($f:ident) => { $f() * 2 }; }\n";
    const OWNER: &str = "pub fn base_rate() -> u32 { 7 }\n";

    #[test]
    fn local_macro_identifier_argument_is_a_non_forwarding_hop() -> Result<(), String> {
        let chain = chain_for(&format!(
            "{RULE}{OWNER}pub fn doubled_rate() -> u32 {{ twice!(base_rate) }}\n"
        ))?;
        assert_eq!(chain.hops.len(), 1, "{chain:?}");
        assert_eq!(chain.hops[0].caller.name, "doubled_rate");
        assert!(chain.hops[0].through_macro);
        assert!(chain.hops[0].arguments.is_empty());
        assert!(!super::super::chain_forwards_owner_result(
            "base_rate",
            &chain
        ));
        Ok(())
    }

    #[test]
    fn unresolvable_or_ambiguous_macro_edges_stay_unrelated() -> Result<(), String> {
        for source in [
            // No local definition: the macro is not known to call anything.
            format!("{OWNER}pub fn doubled_rate() -> u32 {{ twice!(base_rate) }}\n"),
            // Two definitions of the same macro name.
            format!(
                "{RULE}mod m {{ macro_rules! twice {{ ($f:ident) => {{ 0 }}; }} }}\n{OWNER}pub fn doubled_rate() -> u32 {{ twice!(base_rate) }}\n"
            ),
            // Two functions hand the owner to the macro.
            format!(
                "{RULE}{OWNER}pub fn doubled_rate() -> u32 {{ twice!(base_rate) }}\npub fn again() -> u32 {{ twice!(base_rate) }}\n"
            ),
            // The owner name is only a string literal argument.
            format!("{RULE}{OWNER}pub fn doubled_rate() -> u32 {{ twice!(\"base_rate\") }}\n"),
        ] {
            let chain = chain_for(&source)?;
            assert!(chain.hops.is_empty(), "{source}: {chain:?}");
        }
        Ok(())
    }
}
