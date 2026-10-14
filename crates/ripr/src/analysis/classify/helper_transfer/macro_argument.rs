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
//! (`helper_result_not_forwarded`) unless every caller oracle is weak on its
//! own terms (`hop_reach_keeps_weak_oracle_gap`, #7549), the same decision
//! that governs a call reaching the owner inside `format!`.
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
//! - exactly one function and one invocation site carry such a token, and
//!   that function binds no parameter or local of the owner's name.

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
            || !may_hand_name_to_macro(&function.body, callee_name)
        {
            continue;
        }
        let Some(parse) = parse_clean_source_file(&function.body) else {
            continue;
        };
        // A parameter or local of the owner's name (`let base_rate = 3;
        // twice!(base_rate)`) hides the owner from the macro argument.
        if binds_name(parse.tree().syntax(), callee_name) {
            continue;
        }
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

/// Cheap text prefilter before the full parse: this runs for every chain
/// level without direct callers, so a common owner name (`new`, `len`)
/// would otherwise parse most bodies. The body must invoke some macro and
/// mention `name` as a whole word; the parse below stays the authority.
fn may_hand_name_to_macro(body: &str, name: &str) -> bool {
    let is_word = |c: char| c == '_' || c.is_alphanumeric();
    body.contains('!')
        && body.match_indices(name).any(|(at, _)| {
            let before = body.get(..at).and_then(|head| head.chars().next_back());
            let after = body
                .get(at + name.len()..)
                .and_then(|tail| tail.chars().next());
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
}

/// Whether any pattern under `root` (a parameter, `let`, closure or `match`
/// binding) binds `name`.
fn binds_name(root: &SyntaxNode, name: &str) -> bool {
    root.descendants()
        .filter_map(ast::IdentPat::cast)
        .any(|pattern| pattern.name().is_some_and(|bound| bound.text() == name))
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
            && tokens.get(at + 1).is_none_or(|after| after.kind() != T![!])
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

#[cfg(test)]
mod tests {
    use super::{invocations_naming, may_hand_name_to_macro};
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
    fn prefilter_keeps_every_naming_body_and_skips_macro_free_or_partial_words() {
        for body in [
            "fn doubled_rate() -> u32 { twice!(base_rate) }",
            "fn doubled_rate() -> u32 { twice![base_rate] }",
            "fn doubled_rate() -> u32 { twice!(x,base_rate) }",
        ] {
            assert!(may_hand_name_to_macro(body, "base_rate"), "{body}");
        }
        for body in [
            "fn doubled_rate() -> u32 { base_rate() + 1 }",
            "fn doubled_rate() -> u32 { twice!(base_rates) }",
            "fn doubled_rate() -> u32 { twice!(my_base_rate) }",
        ] {
            assert!(!may_hand_name_to_macro(body, "base_rate"), "{body}");
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
            // A local or parameter of the owner's name is what the macro gets.
            format!(
                "{RULE}{OWNER}pub fn doubled_rate() -> u32 {{ let base_rate = || 3; twice!(base_rate) }}\n"
            ),
            format!(
                "{RULE}{OWNER}pub fn doubled_rate(base_rate: fn() -> u32) -> u32 {{ twice!(base_rate) }}\n"
            ),
        ] {
            let chain = chain_for(&source)?;
            assert!(chain.hops.is_empty(), "{source}: {chain:?}");
        }
        Ok(())
    }
}
