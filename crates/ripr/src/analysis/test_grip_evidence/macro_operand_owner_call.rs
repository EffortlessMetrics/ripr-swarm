//! An owner call that only an untrusted inner macro holds (#16188).
//!
//! `assert_eq!(check_ok!(pick(0)), Mode::A)` with a local
//! `macro_rules! check_ok { ($e:expr) => { Mode::A }; }` never evaluates
//! `pick(0)`: the macro decides what its tokens mean. The diff path refuses
//! such an assertion through owner-pin execution admission
//! (`assertion_inert`), but the repo audit credits a plain assertion from its
//! extracted oracle, so the seam read `strongly_gripped` through
//! `direct_owner_call`.
//!
//! The repo view now reads such an assertion as `unknown` for this owner
//! when every call of the owner the assertion spells sits inside the token
//! tree of an inner macro the analyzer does not trust: a name outside
//! [`EVALUATING_STD_MACROS`], or one the test's file defines with
//! `macro_rules!`. The macro may still evaluate the call, so the reading is
//! `unknown`, not absent. An owner call outside every such macro, or an
//! assertion that names no owner call, keeps its parsed reading.

use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxNode, ast};

use crate::domain::{OracleKind, OracleStrength};

use super::{OracleFact, TestSummary};

/// Standard macros that evaluate their operands as ordinary expressions, so
/// an owner call inside one runs when the assertion does.
const EVALUATING_STD_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "dbg",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "format",
    "format_args",
    "matches",
    "vec",
];

/// Marks `oracle` unknown when `owner` is called in it only inside untrusted
/// inner macros. `file_source` is the test file's text, read for local
/// `macro_rules!` definitions that shadow a standard name.
pub(super) fn demote_macro_held_owner_call(
    oracle: &mut OracleFact,
    owner: &str,
    file_source: Option<&str>,
) -> bool {
    if !owner_call_only_inside_untrusted_macro(&oracle.text, owner, file_source) {
        return false;
    }
    oracle.kind = OracleKind::Unknown;
    oracle.strength = OracleStrength::Unknown;
    oracle.observed_tokens.clear();
    true
}

/// Whether the assertion `text` calls `owner` at least once and every such
/// call sits inside an untrusted inner macro's token tree.
pub(super) fn owner_call_only_inside_untrusted_macro(
    text: &str,
    owner: &str,
    file_source: Option<&str>,
) -> bool {
    if owner.is_empty() || !text.contains(owner) || text.matches('!').count() < 2 {
        return false;
    }
    let wrapped = format!(
        "fn __ripr_assertion() {{ {} }}",
        text.trim().trim_end_matches(';')
    );
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(&wrapped) else {
        return false;
    };
    let Some(outer) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::MacroCall::cast)
    else {
        return false;
    };
    let Some(tree) = outer.token_tree() else {
        return false;
    };
    let trusted = |name: &str| {
        EVALUATING_STD_MACROS.contains(&name)
            && !file_source.is_some_and(|source| defines_macro_rules(source, name))
    };
    let mut held = 0usize;
    let mut free = 0usize;
    walk(tree.syntax(), owner, false, &trusted, &mut held, &mut free);
    held > 0 && free == 0
}

/// Counts owner calls in `tree`, split by whether an untrusted inner macro
/// holds them. A call is the owner identifier directly followed by a
/// parenthesized group. A turbofish call (`pick::<u8>(0)`) is not counted,
/// which can only leave the parsed reading in place.
fn walk(
    tree: &SyntaxNode,
    owner: &str,
    inside_untrusted: bool,
    trusted: &dyn Fn(&str) -> bool,
    held: &mut usize,
    free: &mut usize,
) {
    let children: Vec<NodeOrToken<SyntaxNode, _>> = tree
        .children_with_tokens()
        .filter(|child| !child.kind().is_trivia())
        .collect();
    let mut position = 0;
    while position < children.len() {
        let child = &children[position];
        match child {
            NodeOrToken::Token(token) if token.kind() == SyntaxKind::IDENT => {
                let next = children.get(position + 1);
                let after = children.get(position + 2);
                let bang = next.is_some_and(|next| next.kind() == SyntaxKind::BANG);
                if bang
                    && let Some(NodeOrToken::Node(group)) = after
                    && group.kind() == SyntaxKind::TOKEN_TREE
                {
                    let untrusted = inside_untrusted || !trusted(token.text());
                    walk(group, owner, untrusted, trusted, held, free);
                    position += 3;
                    continue;
                }
                if token.text() == owner
                    && let Some(NodeOrToken::Node(group)) = next
                    && group.kind() == SyntaxKind::TOKEN_TREE
                    && group
                        .first_token()
                        .is_some_and(|open| open.kind() == SyntaxKind::L_PAREN)
                {
                    if inside_untrusted {
                        *held += 1;
                    } else {
                        *free += 1;
                    }
                }
            }
            NodeOrToken::Node(group) if group.kind() == SyntaxKind::TOKEN_TREE => {
                walk(group, owner, inside_untrusted, trusted, held, free);
            }
            _ => {}
        }
        position += 1;
    }
}

/// Whether `source` holds a `macro_rules! name` definition.
fn defines_macro_rules(source: &str, name: &str) -> bool {
    source.match_indices("macro_rules!").any(|(at, keyword)| {
        let rest = source[at + keyword.len()..].trim_start();
        rest.strip_prefix(name).is_some_and(|after| {
            !after
                .chars()
                .next()
                .is_some_and(|next| next.is_alphanumeric() || next == '_')
        })
    })
}

/// Whether any assertion of `test` holds an owner call only inside an
/// untrusted inner macro.
pub(super) fn test_has_macro_held_owner_call(
    test: &TestSummary,
    owner: &str,
    index: &super::RustIndex,
) -> bool {
    let source = index
        .files()
        .get(&test.file)
        .map(|facts| facts.data().source.as_ref());
    test.assertions
        .iter()
        .any(|oracle| owner_call_only_inside_untrusted_macro(&oracle.text, owner, source))
}

/// The test's view with every assertion an untrusted inner macro holds the
/// owner call of marked unknown; `None` when nothing changes.
pub(super) fn demoted_view(
    test: &TestSummary,
    owner: &str,
    file_source: Option<&str>,
) -> Option<Vec<OracleFact>> {
    let mut assertions = test.assertions.clone();
    let mut changed = false;
    for oracle in &mut assertions {
        changed |= demote_macro_held_owner_call(oracle, owner, file_source);
    }
    changed.then_some(assertions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_untrusted_macro_holding_every_owner_call_demotes() {
        let local = "macro_rules! check_ok { ($e:expr) => { 1 }; }";
        let cases: &[(&str, Option<&str>, bool)] = &[
            ("assert_eq!(check_ok!(pick(0)), Mode::A)", Some(local), true),
            ("assert_eq!(mystery!(pick(0)), Mode::A);", None, true),
            ("assert_eq!(pick(0), Mode::A)", None, false),
            ("assert_eq!(vec![pick(0)], vec![Mode::A])", None, false),
            ("assert_eq!(format!(\"{:?}\", pick(0)), \"A\")", None, false),
            (
                "assert_eq!(vec![pick(0)], vec![Mode::A])",
                Some("macro_rules! vec { ($($t:tt)*) => { 0 }; }"),
                true,
            ),
            (
                "assert_eq!(check_ok!(pick(0)), pick(1))",
                Some(local),
                false,
            ),
            (
                "assert_eq!(check_ok!(other(0)), Mode::A)",
                Some(local),
                false,
            ),
            ("assert_eq!(vec![check_ok!(pick(0))], v)", Some(local), true),
            ("assert!(picked)", None, false),
        ];
        for (text, source, expected) in cases {
            assert_eq!(
                owner_call_only_inside_untrusted_macro(text, "pick", *source),
                *expected,
                "{text}"
            );
        }
    }

    #[test]
    fn macro_rules_definitions_match_whole_names() {
        assert!(defines_macro_rules("macro_rules! vec { () => {} }", "vec"));
        assert!(defines_macro_rules("macro_rules!vec{}", "vec"));
        assert!(!defines_macro_rules(
            "macro_rules! vec2 { () => {} }",
            "vec"
        ));
        assert!(!defines_macro_rules("let vec = 1;", "vec"));
    }
}
