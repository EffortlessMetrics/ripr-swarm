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
//! assertion that names no owner call, keeps its parsed reading. A call
//! qualified by another type (`Other::new(..)` for the owner
//! `impl Widget::new`), or a `.` method or field on a receiver when the owner
//! is a free function (`other.pick(0)` for `pick`), is not a use of the owner
//! and is not counted.
//!
//! #16192: the owner's result can also reach the macro through a local
//! binding, `let r = pick(0); assert_eq!(check_ok!(r), Mode::A);`. A plain
//! `let` binding initialized directly from an owner call counts like the
//! call itself, but only when every use of the binding in the test sits
//! inside this assertion's untrusted macros: a use anywhere else (another
//! assertion, a clone, a helper argument) may observe the result, so the
//! assertion keeps its parsed reading.

use std::collections::BTreeSet;

use ra_ap_syntax::{
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken,
    ast::{self, HasName},
};

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

/// The owner a call must name: its function name, plus the self type and
/// trait of an impl owner, which a type-qualified call may spell.
pub(super) struct OwnerName<'a> {
    pub(super) name: &'a str,
    pub(super) types: Vec<String>,
}

impl<'a> OwnerName<'a> {
    pub(super) fn of(owner: &'a super::FunctionSummary) -> Self {
        let id = owner.id.0.as_str();
        let types = [
            crate::analysis::classify::impl_self_type_name(id),
            crate::analysis::classify::owner_trait_name(id),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            name: &owner.name,
            types,
        }
    }

    /// Whether the owner is an associated function, which a `.` method call
    /// (`w.read()`) may reach. A free function is never `x.name(..)`.
    fn is_method(&self) -> bool {
        !self.types.is_empty()
    }

    /// Whether a call qualified by `qualifier` (`Q::name(..)`) may be a call
    /// of this owner: a lowercase module path, `Self`, or one of its types.
    fn admits_qualifier(&self, qualifier: &str) -> bool {
        qualifier == "Self"
            || !qualifier.starts_with(|first: char| first.is_ascii_uppercase())
            || self.types.iter().any(|known| known == qualifier)
    }
}

/// A plain local bound directly to an owner call in the test body, with the
/// number of times the body names it after its declaration.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct OwnerResultLocal {
    name: String,
    uses: usize,
}

/// Locals of the test `body` declared once as `let name = <owner call>;`
/// (a plain or `mut` identifier pattern, the initializer exactly one call of
/// the owner). A body that does not parse cleanly yields none, which keeps
/// the parsed reading.
fn owner_result_locals(body: &str, owner: &OwnerName<'_>) -> Vec<OwnerResultLocal> {
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file_memoized(body) else {
        return Vec::new();
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return Vec::new();
    };
    let mut locals = Vec::new();
    for statement in function
        .syntax()
        .descendants()
        .filter_map(ast::LetStmt::cast)
    {
        let Some(ast::Pat::IdentPat(pattern)) = statement.pat() else {
            continue;
        };
        if pattern.ref_token().is_some() || pattern.pat().is_some() {
            continue;
        }
        let (Some(name), Some(initializer)) = (pattern.name(), statement.initializer()) else {
            continue;
        };
        if statement.let_else().is_some() || !is_owner_call(&initializer, owner) {
            continue;
        }
        let declared_at = name.syntax().text_range();
        let name = name.text().to_string();
        let declarations = function
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .filter(|other| other.name().is_some_and(|other| other.text() == name))
            .count();
        if declarations != 1 {
            continue;
        }
        let tokens: Vec<SyntaxToken> = function
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .collect();
        // The declaring name token is excluded by its range, so a typed
        // `let r: T = ..` (whose name a `:` follows) is not miscounted.
        let uses = (0..tokens.len())
            .filter(|&at| {
                !declared_at.contains_range(tokens[at].text_range())
                    && reads_local(&tokens, at, &name)
            })
            .count();
        locals.push(OwnerResultLocal { name, uses });
    }
    locals
}

/// Whether `tokens[at]` may read the local `name`: the identifier itself,
/// not a member (`x.name`), a path segment (`m::name`) or a struct field
/// label (`name: ..`), or a string whose format template captures it
/// (`"{name}"`, `"{name:?}"`). Over-counting a read only withholds the
/// demotion.
fn reads_local(tokens: &[SyntaxToken], at: usize, name: &str) -> bool {
    let token = &tokens[at];
    let kind_at = |index: Option<usize>| {
        index
            .and_then(|index| tokens.get(index))
            .map(|token| token.kind())
    };
    match token.kind() {
        SyntaxKind::IDENT if token.text() == name => {
            let member = matches!(
                kind_at(at.checked_sub(1)),
                Some(SyntaxKind::DOT | SyntaxKind::COLON2)
            ) || (kind_at(at.checked_sub(1)) == Some(SyntaxKind::COLON)
                && kind_at(at.checked_sub(2)) == Some(SyntaxKind::COLON));
            let label = kind_at(at.checked_add(1)) == Some(SyntaxKind::COLON)
                && kind_at(at.checked_add(2)) != Some(SyntaxKind::COLON);
            !member && !label
        }
        SyntaxKind::STRING => captures_local(token.text(), name),
        _ => false,
    }
}

/// Whether a string literal's text holds an implicit format capture of
/// `name` (`{name}`, `{name:..}`), outside `{{` escapes.
fn captures_local(text: &str, name: &str) -> bool {
    text.match_indices('{').any(|(at, _)| {
        let escaped = text[..at].ends_with('{') || text[at + 1..].starts_with('{');
        !escaped
            && text[at + 1..]
                .strip_prefix(name)
                .is_some_and(|after| after.starts_with('}') || after.starts_with(':'))
    })
}

/// Whether `expression` is exactly one call of the owner: `owner(..)`,
/// `Q::owner(..)` with an admitted qualifier, or `x.owner(..)` for an
/// associated-function owner.
fn is_owner_call(expression: &ast::Expr, owner: &OwnerName<'_>) -> bool {
    match expression {
        ast::Expr::CallExpr(call) => {
            let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
                return false;
            };
            let Some(path) = callee.path() else {
                return false;
            };
            let named = path
                .segment()
                .and_then(|segment| segment.name_ref())
                .is_some_and(|name| name.text() == owner.name);
            named
                && path.qualifier().is_none_or(|qualifier| {
                    qualifier
                        .segment()
                        .and_then(|segment| segment.name_ref())
                        .is_some_and(|name| owner.admits_qualifier(name.text()))
                })
        }
        ast::Expr::MethodCallExpr(call) => {
            owner.is_method()
                && call
                    .name_ref()
                    .is_some_and(|name| name.text() == owner.name)
        }
        _ => false,
    }
}

/// Marks `oracle` unknown when `owner` is called in it only inside untrusted
/// inner macros. `file_source` is the test file's text, read for local
/// `macro_rules!` definitions that shadow a standard name.
fn demote_macro_held_owner_call(
    oracle: &mut OracleFact,
    owner: &OwnerName<'_>,
    locals: &[OwnerResultLocal],
    file_source: Option<&str>,
) -> bool {
    if !owner_use_only_inside_untrusted_macro(&oracle.text, owner, locals, file_source) {
        return false;
    }
    oracle.kind = OracleKind::Unknown;
    oracle.strength = OracleStrength::Unknown;
    oracle.observed_tokens.clear();
    true
}

/// Whether the assertion `text` calls `owner` at least once and every such
/// call sits inside an untrusted inner macro's token tree.
#[cfg(test)]
fn owner_call_only_inside_untrusted_macro(
    text: &str,
    owner: &OwnerName<'_>,
    file_source: Option<&str>,
) -> bool {
    owner_use_only_inside_untrusted_macro(text, owner, &[], file_source)
}

/// Whether the assertion `text` uses the owner's result at least once, by a
/// call or through one of `locals`, and every such use sits inside an
/// untrusted inner macro's token tree. A local counts only when all of its
/// uses in the test are held here.
fn owner_use_only_inside_untrusted_macro(
    text: &str,
    owner: &OwnerName<'_>,
    locals: &[OwnerResultLocal],
    file_source: Option<&str>,
) -> bool {
    let named = !owner.name.is_empty() && text.contains(owner.name)
        || locals.iter().any(|local| text.contains(&local.name));
    if !named || text.matches('!').count() < 2 {
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
    // The file is parsed only when a standard name needs the shadow check.
    let shadowed = std::cell::OnceCell::new();
    let trusted = |name: &str| {
        EVALUATING_STD_MACROS.contains(&name)
            && !shadowed
                .get_or_init(|| file_source.map(local_macro_rules_names).unwrap_or_default())
                .contains(name)
    };
    let mut counts = UseCounts {
        held: 0,
        free: 0,
        local_held: vec![0; locals.len()],
    };
    walk(tree.syntax(), owner, locals, false, &trusted, &mut counts);
    // A local is evidence only when every read of it in the test sits in
    // this assertion's untrusted macros; a local read elsewhere adds nothing,
    // but does not undo a held owner call.
    let local_evidence = locals
        .iter()
        .zip(&counts.local_held)
        .any(|(local, held)| *held > 0 && *held == local.uses);
    counts.free == 0 && (counts.held > 0 || local_evidence)
}

/// Owner uses in one assertion: held calls, free uses (calls or locals), and
/// held uses of each owner-result local.
struct UseCounts {
    held: usize,
    free: usize,
    local_held: Vec<usize>,
}

/// Counts owner calls in `tree`, split by whether an untrusted inner macro
/// holds them, unless a type the owner does not belong to qualifies the name.
/// Inside an untrusted macro only the owner identifier directly followed by a
/// parenthesized group counts as a held call. Outside every untrusted macro
/// any occurrence of the name counts as free, so a turbofish call
/// (`pick::<u8>(1)`) or a function value (`map(pick)`) keeps the parsed
/// reading.
fn walk(
    tree: &SyntaxNode,
    owner: &OwnerName<'_>,
    locals: &[OwnerResultLocal],
    inside_untrusted: bool,
    trusted: &dyn Fn(&str) -> bool,
    counts: &mut UseCounts,
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
                    walk(group, owner, locals, untrusted, trusted, counts);
                    position += 3;
                    continue;
                }
                let after_dot = position
                    .checked_sub(1)
                    .and_then(|at| children.get(at))
                    .is_some_and(|before| before.kind() == SyntaxKind::DOT);
                if token.text() == owner.name
                    && (!after_dot || owner.is_method())
                    && qualifier(&children, position)
                        .is_none_or(|qualifier| owner.admits_qualifier(&qualifier))
                {
                    let paren_call = matches!(
                        next,
                        Some(NodeOrToken::Node(group))
                            if group.kind() == SyntaxKind::TOKEN_TREE
                                && group
                                    .first_token()
                                    .is_some_and(|open| open.kind() == SyntaxKind::L_PAREN)
                    );
                    if !inside_untrusted {
                        counts.free += 1;
                    } else if paren_call {
                        counts.held += 1;
                    }
                } else if !after_dot
                    && qualifier(&children, position).is_none()
                    && let Some(at) = locals.iter().position(|local| local.name == token.text())
                {
                    if inside_untrusted {
                        counts.local_held[at] += 1;
                    } else {
                        counts.free += 1;
                    }
                }
            }
            NodeOrToken::Token(token) if token.kind() == SyntaxKind::STRING => {
                // An implicit format capture reads the local where the
                // string is evaluated; inside an untrusted macro it counts
                // as held, like any other use there.
                for (at, local) in locals.iter().enumerate() {
                    if captures_local(token.text(), &local.name) {
                        if inside_untrusted {
                            counts.local_held[at] += 1;
                        } else {
                            counts.free += 1;
                        }
                    }
                }
            }
            NodeOrToken::Node(group) if group.kind() == SyntaxKind::TOKEN_TREE => {
                walk(group, owner, locals, inside_untrusted, trusted, counts);
            }
            _ => {}
        }
        position += 1;
    }
}

/// The path segment before `children[position]` when a `::` precedes it:
/// `Widget` in `Widget::new`.
fn qualifier(children: &[NodeOrToken<SyntaxNode, SyntaxToken>], position: usize) -> Option<String> {
    let before = |back: usize| position.checked_sub(back).and_then(|at| children.get(at));
    let (segment_at, separated) = match before(1).map(|child| child.kind()) {
        Some(SyntaxKind::COLON2) => (2, true),
        Some(SyntaxKind::COLON) => (
            3,
            before(2).is_some_and(|colon| colon.kind() == SyntaxKind::COLON),
        ),
        _ => (0, false),
    };
    if !separated {
        return None;
    }
    match before(segment_at) {
        Some(NodeOrToken::Token(segment)) if segment.kind() == SyntaxKind::IDENT => {
            Some(segment.text().to_string())
        }
        _ => None,
    }
}

/// Names of the `macro_rules!` items the parsed `source` declares. Comments
/// and strings that mention `macro_rules!` declare nothing, and trivia inside
/// a declaration (`macro_rules /* .. */ ! vec`) does not hide it. A file
/// that does not parse cleanly (or exceeds the nesting budget) declares
/// nothing here, which keeps the standard names trusted, as before the pass.
fn local_macro_rules_names(source: &str) -> BTreeSet<String> {
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file_memoized(source) else {
        return BTreeSet::new();
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::MacroRules::cast)
        .filter_map(|rules| rules.name())
        .map(|name| name.text().to_string())
        .collect()
}

/// Whether any assertion of `test` holds an owner call only inside an
/// untrusted inner macro.
pub(super) fn test_has_macro_held_owner_call(
    test: &TestSummary,
    owner: &OwnerName<'_>,
    index: &super::RustIndex,
) -> bool {
    let source = index
        .files()
        .get(&test.file)
        .map(|facts| facts.data().source.as_ref());
    let locals = test_locals(test, owner);
    test.assertions
        .iter()
        .any(|oracle| owner_use_only_inside_untrusted_macro(&oracle.text, owner, &locals, source))
}

/// The owner-result locals of `test`, read only when an assertion holds an
/// inner macro and the body names the owner at all.
fn test_locals(test: &TestSummary, owner: &OwnerName<'_>) -> Vec<OwnerResultLocal> {
    let nested = test
        .assertions
        .iter()
        .any(|oracle| oracle.text.matches('!').count() >= 2);
    if !nested || owner.name.is_empty() || !test.body.as_str().contains(owner.name) {
        return Vec::new();
    }
    owner_result_locals(test.body.as_str(), owner)
}

/// The test's view with every assertion an untrusted inner macro holds the
/// owner call of marked unknown; `None` when nothing changes.
pub(super) fn demoted_view(
    test: &TestSummary,
    owner: &OwnerName<'_>,
    file_source: Option<&str>,
) -> Option<Vec<OracleFact>> {
    let locals = test_locals(test, owner);
    let first = test.assertions.iter().position(|oracle| {
        owner_use_only_inside_untrusted_macro(&oracle.text, owner, &locals, file_source)
    })?;
    let mut assertions = test.assertions.clone();
    for oracle in &mut assertions[first..] {
        demote_macro_held_owner_call(oracle, owner, &locals, file_source);
    }
    Some(assertions)
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
                "assert_eq!(check_ok!(pick(0)), pick::<u8>(1))",
                Some(local),
                false,
            ),
            (
                "assert_eq!(check_ok!(pick(0)), [0].map(pick))",
                Some(local),
                false,
            ),
            (
                "assert_eq!(check_ok!(pick::<u8>(0)), Mode::A)",
                Some(local),
                false,
            ),
            (
                "assert_eq!(check_ok!(pick(0)), other.pick(0))",
                Some(local),
                true,
            ),
            (
                "assert_eq!(check_ok!(other.pick(0)), Mode::A)",
                Some(local),
                false,
            ),
            (
                "assert_eq!(check_ok!(pick(0)), cfg.pick)",
                Some(local),
                true,
            ),
            (
                "assert_eq!(check_ok!(other(0)), Mode::A)",
                Some(local),
                false,
            ),
            ("assert_eq!(vec![check_ok!(pick(0))], v)", Some(local), true),
            ("assert!(picked)", None, false),
        ];
        let pick = OwnerName {
            name: "pick",
            types: Vec::new(),
        };
        for (text, source, expected) in cases {
            assert_eq!(
                owner_call_only_inside_untrusted_macro(text, &pick, *source),
                *expected,
                "{text}"
            );
        }
    }

    #[test]
    fn a_call_qualified_by_another_type_is_not_the_owner() {
        let new = OwnerName {
            name: "new",
            types: vec!["Widget".to_string()],
        };
        let cases: &[(&str, bool)] = &[
            ("assert_eq!(mystery!(Widget::new()), w)", true),
            ("assert_eq!(mystery!(Self::new()), w)", true),
            ("assert_eq!(mystery!(widgets::new()), w)", true),
            ("assert_eq!(mystery!(Other::new()), w)", false),
            ("assert_eq!(mystery!(Other::new()), Widget::new())", false),
            ("assert_eq!(mystery!(Widget::new()), Other::new())", true),
            ("assert_eq!(mystery!(w.new()), w)", true),
        ];
        for (text, expected) in cases {
            assert_eq!(
                owner_call_only_inside_untrusted_macro(text, &new, None),
                *expected,
                "{text}"
            );
        }
    }

    #[test]
    fn a_local_bound_to_the_owner_counts_only_when_every_use_is_held() {
        let pick = OwnerName {
            name: "pick",
            types: Vec::new(),
        };
        let local = "macro_rules! check_ok { ($e:expr) => { 1 }; }";
        let cases: &[(&str, &str, bool)] = &[
            (
                "let r = pick(0);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let mut r = pick(0);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            ("let r = pick(0);", "assert_eq!(r, Mode::A)", false),
            ("let r = pick(0);", "assert_eq!(check_ok!(r), r)", false),
            (
                "let r = pick(0); let s = r.clone();",
                "assert_eq!(check_ok!(r), s)",
                false,
            ),
            (
                "let r = other(0);",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = pick(0).unwrap();",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = x.pick(0);",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = pick(0); let r = 1;",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(vec![r], vec![Mode::A])",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(check_ok!(r.0), Mode::A)",
                true,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{r:?}{}\", check_ok!(r)), \"Ok(A)\")",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{{r}}{}\", check_ok!(r)), \"Ok(A)\")",
                true,
            ),
            (
                "let r = pick(0); let _ = &r;",
                "assert_eq!(check_ok!((pick(0), r)), Mode::A)",
                true,
            ),
            (
                "let r = pick(0); let _ = &r;",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = pick(0); let _ = fixture.r; let _ = m::r; let _ = S { r: 1 };",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let r: Result<Mode, ()> = pick(0); let s = r.clone();",
                "assert_eq!(check_ok!(r), s)",
                false,
            ),
            (
                "let r: Result<Mode, ()> = pick(0);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let r = pick(0); let _ = S { r };",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = pick(0); println!(\"{r}\");",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
        ];
        for (setup, assertion, expected) in cases {
            let body = format!("fn t() {{ {setup} {assertion}; }}");
            let locals = owner_result_locals(&body, &pick);
            assert_eq!(
                owner_use_only_inside_untrusted_macro(assertion, &pick, &locals, Some(local)),
                *expected,
                "{setup} {assertion}"
            );
        }
    }

    #[test]
    fn macro_rules_declarations_come_from_syntax() {
        let names = |source: &str| {
            local_macro_rules_names(source)
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert_eq!(names("macro_rules! vec { () => {} }"), ["vec"]);
        assert_eq!(names("macro_rules!vec{}"), ["vec"]);
        assert_eq!(
            names("macro_rules /* shadow */ ! vec { () => {} }"),
            ["vec"]
        );
        assert_eq!(
            names("mod t { fn f() { macro_rules! vec { () => {} } } }"),
            ["vec"]
        );
        assert_eq!(names("macro_rules! vec2 { () => {} }"), ["vec2"]);
        assert!(names("// macro_rules! vec is not needed\nfn f() {}").is_empty());
        assert!(names("const S: &str = \"macro_rules! vec {}\";").is_empty());
        assert!(names("let vec = 1;").is_empty());
    }
}
