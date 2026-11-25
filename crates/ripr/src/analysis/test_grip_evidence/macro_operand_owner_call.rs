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

use std::collections::{BTreeMap, BTreeSet};

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

/// A local of the test body that may hold the owner's result. `evidence`
/// marks a plain local bound to an owner call, with the number of times the
/// body names it after its declaration. Any other local whose initializer
/// names the owner or such a local (`let actual = &pick(0);`, `let s = &r;`)
/// is tracked only so that a read of it outside the untrusted macro counts
/// as a free use of the owner's result.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct OwnerResultLocal {
    name: String,
    uses: usize,
    evidence: bool,
    /// #16220: a function-item alias of the owner (`let f = pick;`). Only a
    /// call of it (`f(0)`) is an owner use; a bare read holds the function.
    callable: bool,
}

/// Whether `tokens[at]` names `owner` as a call or path, not as a member
/// (`Other.pick`) or a qualifier the owner does not admit (`Other::pick`).
/// #16220: the local-carrying fallback must use the resolved owner, not the
/// bare identifier spelling, or an unrelated method's name token reads as a
/// free observation of the owner.
fn token_names_owner(
    tokens: &[SyntaxToken],
    at: usize,
    owner: &OwnerName<'_>,
    foreign: &BTreeSet<String>,
) -> bool {
    let token = &tokens[at];
    if token.kind() != SyntaxKind::IDENT || token.text() != owner.name {
        return false;
    }
    let before = |back: usize| at.checked_sub(back).and_then(|index| tokens.get(index));
    let after = |ahead: usize| at.checked_add(ahead).and_then(|index| tokens.get(index));
    // A field label (`pick: ..`) names a field, not the owner.
    if after(1).is_some_and(|token| token.kind() == SyntaxKind::COLON)
        && after(2).is_none_or(|token| token.kind() != SyntaxKind::COLON)
    {
        return false;
    }
    // A bare path to the function item (`let f = pick;`) holds the function,
    // not its result, so it does not make an alias carry the owner's result.
    // A call (`pick(..)`) or a turbofish call (`pick::<T>(..)`) does.
    if !after(1)
        .is_some_and(|token| matches!(token.kind(), SyntaxKind::L_PAREN | SyntaxKind::COLON2))
    {
        return false;
    }
    let after_dot = before(1).is_some_and(|token| token.kind() == SyntaxKind::DOT);
    if after_dot && !owner.is_method() {
        return false;
    }
    // #16220: a receiver the test binds to a definitely different type
    // (`let other = Other;`) makes `other.pick(..)` an unrelated method.
    if after_dot
        && before(2).is_some_and(|receiver| {
            receiver.kind() == SyntaxKind::IDENT
                && before(3).is_none_or(|dot| dot.kind() != SyntaxKind::DOT)
                && foreign.contains(receiver.text())
        })
    {
        return false;
    }
    let qualifier = match before(1).map(|token| token.kind()) {
        Some(SyntaxKind::COLON2) => before(2)
            .filter(|token| token.kind() == SyntaxKind::IDENT)
            .map(|token| token.text().to_string()),
        Some(SyntaxKind::COLON)
            if before(2).is_some_and(|token| token.kind() == SyntaxKind::COLON) =>
        {
            before(3)
                .filter(|token| token.kind() == SyntaxKind::IDENT)
                .map(|token| token.text().to_string())
        }
        _ => None,
    };
    qualifier.is_none_or(|qualifier| owner.admits_qualifier(&qualifier))
}

/// Locals of the test `body` that may hold the owner's result. Evidence
/// locals are declared once as `let name = <owner call>;` (a plain or `mut`
/// identifier pattern, the initializer one call of the owner, possibly
/// unwrapped or chained: `pick(0).unwrap()`, `pick(0)?`). A body that does
/// not parse cleanly yields none, which keeps the parsed reading.
fn owner_result_locals(
    body: &str,
    owner: &OwnerName<'_>,
    file_source: Option<&str>,
) -> Vec<OwnerResultLocal> {
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file_memoized(body) else {
        return Vec::new();
    };
    let foreign: BTreeSet<String> = if owner.is_method() {
        typed_receiver_bindings(body, owner, &file_type_aliases(file_source))
            .into_iter()
            .filter(|(_, is_owner)| !is_owner)
            .map(|(name, _)| name)
            .collect()
    } else {
        BTreeSet::new()
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return Vec::new();
    };
    let tokens: Vec<SyntaxToken> = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    // #16220: a closure bound to a local is also a path at its call site
    // (`let callback = |v| ..; .inspect(callback)`), and it can capture and
    // record the owner's result. A path that names one of these bindings is
    // not a function item, so it is not transparent.
    let bindings: BTreeSet<String> = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter_map(|pattern| pattern.name())
        .map(|name| name.text().to_string())
        .collect();
    let mut locals: Vec<OwnerResultLocal> = Vec::new();
    for statement in function
        .syntax()
        .descendants()
        .filter_map(ast::LetStmt::cast)
    {
        let Some(initializer) = statement.initializer() else {
            continue;
        };
        if let Some(local) = evidence_local(
            &function,
            &tokens,
            &bindings,
            &statement,
            &initializer,
            owner,
        ) {
            locals.push(local);
            continue;
        }
        if let Some(alias) = function_item_alias(&function, &statement, &initializer, owner) {
            locals.push(alias);
            continue;
        }
        // Locals are visited in source order, so a chain of bindings
        // (`let s = &r; let t = s;`) is followed forward.
        let carries = initializer
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| {
                if token.kind() != SyntaxKind::IDENT {
                    return false;
                }
                let Some(at) = tokens
                    .iter()
                    .position(|candidate| candidate.text_range() == token.text_range())
                else {
                    return false;
                };
                // #16220: a field label (`r: ..`) or a member (`.r`) names a
                // field, not a read of the local, so it does not make the
                // alias carry the owner's result.
                if let Some(local) = locals.iter().find(|local| local.name == token.text()) {
                    // A function-item alias carries the owner's result only
                    // through a call (`f(0)`), not a bare read (`let _ = f;`).
                    if local.callable
                        && tokens
                            .get(at + 1)
                            .is_none_or(|next| next.kind() != SyntaxKind::L_PAREN)
                    {
                        return false;
                    }
                    return reads_local(&tokens, at, token.text());
                }
                if owner.name.is_empty() || token.text() != owner.name {
                    return false;
                }
                token_names_owner(&tokens, at, owner, &foreign)
            });
        if !carries {
            continue;
        }
        let Some(pattern) = statement.pat() else {
            continue;
        };
        let names: Vec<String> = pattern
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .filter_map(|bound| bound.name())
            .map(|name| name.text().to_string())
            .collect();
        for name in names {
            locals.push(OwnerResultLocal {
                name,
                uses: 0,
                evidence: false,
                callable: false,
            });
        }
    }
    locals
}

/// A function-item alias of a free-function owner: `let f = pick;` binding a
/// plain, immutable identifier declared once in the test to an unqualified or
/// admitted path naming the owner. A shadowed (`let f = |_| ..;`) or `mut`
/// (reassignable) alias is not tracked, so its call is not an owner use.
fn function_item_alias(
    function: &ast::Fn,
    statement: &ast::LetStmt,
    initializer: &ast::Expr,
    owner: &OwnerName<'_>,
) -> Option<OwnerResultLocal> {
    if owner.is_method() || owner.name.is_empty() {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = statement.pat()? else {
        return None;
    };
    if pattern.ref_token().is_some() || pattern.mut_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    let declarations = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|other| other.name().is_some_and(|other| other.text() == name))
        .count();
    if declarations != 1 {
        return None;
    }
    let ast::Expr::PathExpr(path) = initializer else {
        return None;
    };
    let path = path.path()?;
    let segment = path.segment()?.name_ref()?;
    // An unqualified name the test imports (`use other::pick;`) or defines
    // (`fn pick(..)`) may not be the owner.
    if path.qualifier().is_none() && test_rebinds_name(function, owner.name) {
        return None;
    }
    if segment.text() != owner.name
        || path.qualifier().is_some_and(|qualifier| {
            qualifier
                .segment()
                .and_then(|segment| segment.name_ref())
                .is_none_or(|name| !owner.admits_qualifier(name.text()))
        })
    {
        return None;
    }
    Some(OwnerResultLocal {
        name,
        uses: 0,
        evidence: false,
        callable: true,
    })
}

/// Whether the test body imports or defines an item called `name`.
fn test_rebinds_name(function: &ast::Fn, name: &str) -> bool {
    let Some(body) = function.body() else {
        return false;
    };
    body.syntax().descendants().any(|node| {
        if let Some(tree) = ast::UseTree::cast(node.clone()) {
            if tree.use_tree_list().is_some() || tree.star_token().is_some() {
                return false;
            }
            return match tree.rename() {
                Some(rename) => rename.name().is_some_and(|bound| bound.text() == name),
                None => tree
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|bound| bound.text() == name),
            };
        }
        ast::Fn::cast(node)
            .is_some_and(|item| item.name().is_some_and(|bound| bound.text() == name))
    })
}

/// Names declared as `type` aliases or `use .. as` renames in the test's file.
/// A binding annotated with one may still be the owner's type, so it is not
/// settled as another.
fn file_type_aliases(file_source: Option<&str>) -> BTreeSet<String> {
    file_source
        .and_then(crate::analysis::syntax::parse_clean_source_file_memoized)
        .map(|parse| {
            parse
                .tree()
                .syntax()
                .descendants()
                .filter_map(ast::TypeAlias::cast)
                .filter_map(|alias| alias.name())
                .chain(
                    parse
                        .tree()
                        .syntax()
                        .descendants()
                        .filter_map(ast::Rename::cast)
                        .filter_map(|rename| rename.name()),
                )
                .map(|name| name.text().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The evidence local `statement` declares, when it binds a plain or `mut`
/// identifier once to an owner-rooted `initializer`.
fn evidence_local(
    function: &ast::Fn,
    tokens: &[SyntaxToken],
    bindings: &BTreeSet<String>,
    statement: &ast::LetStmt,
    initializer: &ast::Expr,
    owner: &OwnerName<'_>,
) -> Option<OwnerResultLocal> {
    let Some(ast::Pat::IdentPat(pattern)) = statement.pat() else {
        return None;
    };
    if pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?;
    if statement.let_else().is_some() || !owner_rooted(initializer, owner, bindings) {
        return None;
    }
    let declared_at = name.syntax().text_range();
    let initialized_by = initializer.syntax().text_range();
    let name = name.text().to_string();
    let declarations = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|other| other.name().is_some_and(|other| other.text() == name))
        .count();
    if declarations != 1 {
        return None;
    }
    // The declaring name token is excluded by its range, so a typed
    // `let r: T = ..` (whose name a `:` follows) is not miscounted, and
    // so is the initializer, whose callee may share the local's name
    // (`let pick = pick(0)`).
    let uses = (0..tokens.len())
        .filter(|&at| {
            let range = tokens[at].text_range();
            !declared_at.contains_range(range)
                && !initialized_by.contains_range(range)
                && reads_local(tokens, at, &name)
        })
        .count();
    Some(OwnerResultLocal {
        name,
        uses,
        evidence: true,
        callable: false,
    })
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
/// `name`: as the argument (`{name}`, `{name:..}`) or as a width or
/// precision parameter (`{:name$}`, `{:.name$}`). `{{` escapes are consumed
/// in order, so the third brace of `{{{name}}}` still opens a capture.
fn captures_local(text: &str, name: &str) -> bool {
    let mut at = 0;
    while let Some(offset) = text[at..].find('{') {
        let open = at + offset + 1;
        if text[open..].starts_with('{') {
            at = open + 1;
            continue;
        }
        let Some(length) = text[open..].find('}') else {
            return false;
        };
        if placeholder_reads(&text[open..open + length], name) {
            return true;
        }
        at = open + length + 1;
    }
    false
}

/// Whether one placeholder's contents (between its braces) read `name`.
fn placeholder_reads(placeholder: &str, name: &str) -> bool {
    let (argument, spec) = placeholder.split_once(':').unwrap_or((placeholder, ""));
    argument == name
        || spec.match_indices(name).any(|(at, _)| {
            // A zero-pad flag may precede the width name (`{:0name$}`);
            // other identifier characters before it make a longer name.
            spec[at + name.len()..].starts_with('$')
                && !spec[..at]
                    .trim_end_matches(|c: char| c.is_ascii_digit())
                    .ends_with(|c: char| c.is_alphanumeric() || c == '_')
        })
}

/// Standard unwrapping and conversion methods that observe nothing beyond
/// their receiver. Any other method, or a closure argument
/// (`inspect(|v| seen = ..)`), may record the owner's result where a later
/// assertion reads it.
const PASS_THROUGH_METHODS: &[&str] = &[
    "unwrap",
    "expect",
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_err",
    "expect_err",
    "clone",
    "cloned",
    "copied",
    "ok",
    "err",
    "into",
    "to_owned",
    "to_string",
    "as_ref",
    "as_deref",
];

/// Methods that invoke a caller-supplied function. They are transparent only
/// when every argument is a function item or a literal: a function item
/// cannot capture a test local, so `map(helper)` cannot record the result in
/// one, while a closure (`inspect(|v| seen = ..)`) can. A function item that
/// records through a static is a known boundary: the reading then withholds
/// credit (`unknown`), it never grants it. #16220.
const TRANSFORMING_METHODS: &[&str] = &[
    "map",
    "and_then",
    "map_err",
    "filter",
    "filter_map",
    "flat_map",
    "inspect",
    "map_or",
    "map_or_else",
    "or_else",
    "unwrap_or_else",
];

/// Whether `expression` is a call of the owner, possibly followed by `?`,
/// `.await`, parentheses or methods that cannot observe the value
/// (`pick(0).unwrap()`, `(pick(0))?`, `pick(0).expect("..")`). The value
/// then derives only from that call, and nothing else in the chain can
/// observe it. A chain rooted anywhere else (`x.pick(0)` for a free
/// function, `other(pick(0))`) is not the owner's result.
fn owner_rooted(
    expression: &ast::Expr,
    owner: &OwnerName<'_>,
    bindings: &BTreeSet<String>,
) -> bool {
    let mut current = expression.clone();
    loop {
        if is_owner_call(&current, owner) {
            return true;
        }
        let next = match &current {
            ast::Expr::MethodCallExpr(call) => {
                // #16220: the method's semantics decide, not the argument's
                // spelling. A pass-through method selects or converts, so its
                // argument is a value, not a callback. A transforming method
                // is transparent only when every argument is a function item
                // or a literal: a function item cannot capture a local, so
                // `map(helper)` cannot record the result in one, while a closure
                // (`inspect(|v| seen = ..)`) can.
                let pass_through = call.name_ref().is_some_and(|name| {
                    let name = name.text();
                    PASS_THROUGH_METHODS.contains(&name)
                        || (TRANSFORMING_METHODS.contains(&name)
                            && ast::HasArgList::arg_list(call).is_some_and(|arguments| {
                                arguments.args().all(|argument| match argument {
                                    ast::Expr::Literal(_) => true,
                                    ast::Expr::PathExpr(path) => {
                                        path_is_function_item(&path, bindings)
                                    }
                                    _ => false,
                                })
                            }))
                });
                if !pass_through {
                    return false;
                }
                call.receiver()
            }
            ast::Expr::TryExpr(tried) => tried.expr(),
            ast::Expr::AwaitExpr(awaited) => awaited.expr(),
            ast::Expr::ParenExpr(inner) => inner.expr(),
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };
        current = next;
    }
}

/// Whether `path` names a function item rather than a local binding. A
/// closure bound to a local is also a path at its call site, and it can
/// capture and record the owner's result, so it is not transparent. A
/// qualified path (`helpers::map`, `Type::method`) is a free or associated
/// function; a single segment is a function item only when no local binding
/// in the test shadows it. A non-local path cannot be a closure, so it cannot
/// capture. #16220.
fn path_is_function_item(path: &ast::PathExpr, bindings: &BTreeSet<String>) -> bool {
    let Some(path) = path.path() else {
        return false;
    };
    if path.qualifier().is_some() {
        return true;
    }
    path.segment()
        .and_then(|segment| segment.name_ref())
        .is_some_and(|name| !bindings.contains(name.text()))
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
    test_body: &str,
) -> bool {
    if !owner_use_only_inside_untrusted_macro(&oracle.text, owner, locals, file_source, test_body) {
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
    owner_use_only_inside_untrusted_macro(text, owner, &[], file_source, "")
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
    test_body: &str,
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
    // #16220: receiver names whose type the test body settles. `true` is a
    // receiver of the owner's type (a plain binding of it, or a receiver a
    // held call uses); `false` is a binding of a definitely different type.
    let mut owner_receivers = BTreeMap::new();
    if owner.is_method() {
        let typed = typed_receiver_bindings(test_body, owner, &file_type_aliases(file_source));
        let mut held = BTreeSet::new();
        collect_held_receivers(tree.syntax(), owner, false, &trusted, &mut held);
        owner_receivers.extend(held.into_iter().map(|name| (name, true)));
        owner_receivers.extend(typed);
    }
    walk(
        tree.syntax(),
        owner,
        locals,
        &owner_receivers,
        false,
        &trusted,
        &mut counts,
    );
    // A local is evidence only when every read of it in the test sits in
    // this assertion's untrusted macros; a local read elsewhere adds nothing,
    // but does not undo a held owner call.
    let local_evidence = locals
        .iter()
        .zip(&counts.local_held)
        .any(|(local, held)| local.evidence && *held > 0 && *held == local.uses);
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
    owner_receivers: &BTreeMap<String, bool>,
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
                    walk(
                        group,
                        owner,
                        locals,
                        owner_receivers,
                        untrusted,
                        trusted,
                        counts,
                    );
                    position += 3;
                    continue;
                }
                let after_dot = position
                    .checked_sub(1)
                    .and_then(|at| children.get(at))
                    .is_some_and(|before| before.kind() == SyntaxKind::DOT);
                let unqualified = !after_dot && qualifier(&children, position).is_none();
                let label = next.is_some_and(|next| next.kind() == SyntaxKind::COLON)
                    && after.is_none_or(|after| after.kind() != SyntaxKind::COLON);
                // A local shadows a same-named owner function (`let pick =
                // pick(0)`), so an unqualified name it binds reads the local.
                let local = locals.iter().position(|local| local.name == token.text());
                if unqualified && label {
                    // A struct field label (`Holder { r: .. }`) reads nothing.
                } else if let (true, Some(at)) = (unqualified, local) {
                    if locals[at].callable {
                        // A function-item alias is an owner use only when called.
                        let called = matches!(
                            next,
                            Some(NodeOrToken::Node(group))
                                if group.kind() == SyntaxKind::TOKEN_TREE
                                    && group
                                        .first_token()
                                        .is_some_and(|open| open.kind() == SyntaxKind::L_PAREN)
                        );
                        if called && inside_untrusted {
                            counts.held += 1;
                        } else if called {
                            counts.free += 1;
                        }
                    } else if inside_untrusted {
                        counts.local_held[at] += 1;
                    } else {
                        counts.free += 1;
                    }
                } else if token.text() == owner.name
                    && (!after_dot || owner.is_method())
                    && receiver_is_owner(
                        &children,
                        position,
                        after_dot,
                        inside_untrusted,
                        owner,
                        locals,
                        owner_receivers,
                    )
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
                    let turbofish = next.is_some_and(|next| next.kind() == SyntaxKind::COLON2)
                        || next.is_some_and(|next| next.kind() == SyntaxKind::COLON)
                            && after.is_some_and(|after| after.kind() == SyntaxKind::COLON);
                    if !inside_untrusted && (!after_dot || paren_call || turbofish) {
                        counts.free += 1;
                    } else if inside_untrusted && paren_call {
                        counts.held += 1;
                    }
                }
            }
            NodeOrToken::Token(token) if token.kind() == SyntaxKind::STRING => {
                // An implicit format capture reads the local where the
                // string is evaluated; inside an untrusted macro it counts
                // as held, like any other use there.
                for (at, local) in locals.iter().enumerate() {
                    if !local.callable && captures_local(token.text(), &local.name) {
                        if inside_untrusted {
                            counts.local_held[at] += 1;
                        } else {
                            counts.free += 1;
                        }
                    }
                }
            }
            NodeOrToken::Node(group) if group.kind() == SyntaxKind::TOKEN_TREE => {
                // #16220: `(f)(0)` calls a function-item alias through
                // parentheses, like `f(0)`.
                let called = matches!(
                    children.get(position + 1),
                    Some(NodeOrToken::Node(next))
                        if next.kind() == SyntaxKind::TOKEN_TREE
                            && next
                                .first_token()
                                .is_some_and(|open| open.kind() == SyntaxKind::L_PAREN)
                );
                let parts: Vec<_> = group
                    .children_with_tokens()
                    .filter(|part| !part.kind().is_trivia())
                    .collect();
                let wrapped_alias = match parts.as_slice() {
                    [open, NodeOrToken::Token(inner), close]
                        if open.kind() == SyntaxKind::L_PAREN
                            && close.kind() == SyntaxKind::R_PAREN
                            && inner.kind() == SyntaxKind::IDENT =>
                    {
                        locals
                            .iter()
                            .any(|local| local.callable && local.name == inner.text())
                    }
                    _ => false,
                };
                if called && wrapped_alias {
                    if inside_untrusted {
                        counts.held += 1;
                    } else {
                        counts.free += 1;
                    }
                } else {
                    walk(
                        group,
                        owner,
                        locals,
                        owner_receivers,
                        inside_untrusted,
                        trusted,
                        counts,
                    );
                }
            }
            _ => {}
        }
        position += 1;
    }
}

/// Match a bare receiver name, optionally wrapped in parentheses. A group
/// after a callee is an argument list, so `make(picker).pick()` does not
/// use `picker` as its receiver.
fn receiver_name(
    children: &[NodeOrToken<SyntaxNode, SyntaxToken>],
    method_at: usize,
) -> Option<String> {
    let receiver_at = method_at.checked_sub(2)?;
    let receiver = children.get(receiver_at)?;
    // A field receiver (`holder.picker.pick(..)`) names a field, not a binding.
    if matches!(receiver, NodeOrToken::Token(_))
        && receiver_at
            .checked_sub(1)
            .and_then(|at| children.get(at))
            .is_some_and(|before| before.kind() == SyntaxKind::DOT)
    {
        return None;
    }
    if matches!(receiver, NodeOrToken::Node(_))
        && receiver_at
            .checked_sub(1)
            .and_then(|at| children.get(at))
            .is_some_and(|before| {
                !matches!(
                    before.kind(),
                    SyntaxKind::L_PAREN
                        | SyntaxKind::L_BRACK
                        | SyntaxKind::L_CURLY
                        | SyntaxKind::COMMA
                        | SyntaxKind::AMP
                )
            })
    {
        return None;
    }
    let mut receiver = receiver.clone();
    loop {
        match receiver {
            NodeOrToken::Token(token)
                if matches!(
                    token.kind(),
                    SyntaxKind::IDENT | SyntaxKind::SELF_KW | SyntaxKind::SELF_TYPE_KW
                ) =>
            {
                return Some(token.text().to_string());
            }
            NodeOrToken::Node(group) if group.kind() == SyntaxKind::TOKEN_TREE => {
                let mut parts = group
                    .children_with_tokens()
                    .filter(|part| !part.kind().is_trivia());
                match (parts.next(), parts.next(), parts.next(), parts.next()) {
                    (Some(open), Some(inner), Some(close), None)
                        if open.kind() == SyntaxKind::L_PAREN
                            && close.kind() == SyntaxKind::R_PAREN =>
                    {
                        receiver = inner;
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
}

/// Unshadowed top-level bindings whose type the body states: a plain type
/// annotation (`let w: Picker = ..`), a unit-struct initializer
/// (`let w = Picker`) or a struct literal (`let w = Picker { .. }`). Each is
/// paired with whether that type is the owner's type. A binding made any other
/// way (`Picker::new()`, `make_picker()`) is left out, so its receiver stays
/// unresolved rather than being read as unrelated.
fn typed_receiver_bindings(
    body: &str,
    owner: &OwnerName<'_>,
    aliases: &BTreeSet<String>,
) -> Vec<(String, bool)> {
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file_memoized(body) else {
        return Vec::new();
    };
    let Some(block) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .and_then(|function| function.body())
    else {
        return Vec::new();
    };
    let Some(statements) = block.stmt_list() else {
        return Vec::new();
    };
    statements
        .statements()
        .filter_map(|statement| {
            let ast::Stmt::LetStmt(binding) = statement else {
                return None;
            };
            let ast::Pat::IdentPat(pattern) = binding.pat()? else {
                return None;
            };
            if pattern.ref_token().is_some() || pattern.pat().is_some() {
                return None;
            }
            let name = pattern.name()?.text().to_string();
            let path = match binding.ty() {
                Some(ast::Type::PathType(ty)) => ty.path()?,
                Some(_) => return None,
                None => match binding.initializer()? {
                    ast::Expr::PathExpr(initializer) => initializer.path()?,
                    ast::Expr::RecordExpr(initializer) => initializer.path()?,
                    _ => return None,
                },
            };
            if path.qualifier().is_some() {
                return None;
            }
            let ty = path.segment()?.name_ref()?;
            if aliases.contains(ty.text()) {
                return None;
            }
            let declarations = block
                .syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .filter(|pattern| pattern.name().is_some_and(|other| other.text() == name))
                .take(2)
                .count();
            (declarations == 1).then(|| {
                let is_owner = owner.types.iter().any(|known| known == ty.text());
                (name, is_owner)
            })
        })
        .collect()
}

/// A dot call reaches the owner through `self`/`Self`, an owner type, an
/// owner-result local, a binding of the owner's type, or a receiver named in
/// a held call. It is unrelated only when its receiver is a binding of a
/// definitely different type (`let other = Other;`). Any other receiver
/// (`w.clone().new()`, or `w` bound by `Picker::new()`) is unresolved, and the
/// unresolved call settles toward keeping the parsed reading: outside the
/// macro it counts as a free use, inside it does not count as held, so it
/// never supplies the demotion. #16220.
fn receiver_is_owner(
    children: &[NodeOrToken<SyntaxNode, SyntaxToken>],
    position: usize,
    after_dot: bool,
    inside_untrusted: bool,
    owner: &OwnerName<'_>,
    locals: &[OwnerResultLocal],
    owner_receivers: &BTreeMap<String, bool>,
) -> bool {
    if !after_dot {
        return true;
    }
    let Some(receiver) = receiver_name(children, position) else {
        return !inside_untrusted;
    };
    if receiver == "self"
        || receiver == "Self"
        || owner.types.contains(&receiver)
        || locals
            .iter()
            .any(|local| local.evidence && local.name == receiver)
    {
        return true;
    }
    match owner_receivers.get(&receiver) {
        Some(is_owner) => *is_owner,
        None => !inside_untrusted,
    }
}

fn collect_held_receivers(
    tree: &SyntaxNode,
    owner: &OwnerName<'_>,
    inside_untrusted: bool,
    trusted: &dyn Fn(&str) -> bool,
    receivers: &mut BTreeSet<String>,
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
                    collect_held_receivers(group, owner, untrusted, trusted, receivers);
                    position += 3;
                    continue;
                }
                if inside_untrusted
                    && owner.is_method()
                    && token.text() == owner.name
                    && position
                        .checked_sub(1)
                        .and_then(|at| children.get(at))
                        .is_some_and(|before| before.kind() == SyntaxKind::DOT)
                    && matches!(
                        next,
                        Some(NodeOrToken::Node(group))
                            if group.kind() == SyntaxKind::TOKEN_TREE
                                && group
                                    .first_token()
                                    .is_some_and(|open| open.kind() == SyntaxKind::L_PAREN)
                    )
                    && let Some(receiver) = receiver_name(&children, position)
                {
                    receivers.insert(receiver);
                }
            }
            NodeOrToken::Node(group) if group.kind() == SyntaxKind::TOKEN_TREE => {
                collect_held_receivers(group, owner, inside_untrusted, trusted, receivers);
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
    let tree = parse.tree();
    // #16220: a `use .. as name` rename (`use std::stringify as assert_eq;`)
    // shadows a standard macro name just as a local `macro_rules!` does.
    let renamed = tree
        .syntax()
        .descendants()
        .filter_map(ast::Rename::cast)
        .filter_map(|rename| rename.name())
        .map(|name| name.text().to_string());
    tree.syntax()
        .descendants()
        .filter_map(ast::MacroRules::cast)
        .filter_map(|rules| rules.name())
        .map(|name| name.text().to_string())
        .chain(renamed)
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
    let locals = test_locals(test, owner, source);
    test.assertions.iter().any(|oracle| {
        owner_use_only_inside_untrusted_macro(
            &oracle.text,
            owner,
            &locals,
            source,
            test.body.as_str(),
        )
    })
}

/// The owner-result locals of `test`, read only when an assertion holds an
/// inner macro and the body names the owner at all.
fn test_locals(
    test: &TestSummary,
    owner: &OwnerName<'_>,
    file_source: Option<&str>,
) -> Vec<OwnerResultLocal> {
    let nested = test
        .assertions
        .iter()
        .any(|oracle| oracle.text.matches('!').count() >= 2);
    if !nested || owner.name.is_empty() || !test.body.as_str().contains(owner.name) {
        return Vec::new();
    }
    owner_result_locals(test.body.as_str(), owner, file_source)
}

/// The test's view with every assertion an untrusted inner macro holds the
/// owner call of marked unknown; `None` when nothing changes.
pub(super) fn demoted_view(
    test: &TestSummary,
    owner: &OwnerName<'_>,
    file_source: Option<&str>,
) -> Option<Vec<OracleFact>> {
    let locals = test_locals(test, owner, file_source);
    let first = test.assertions.iter().position(|oracle| {
        owner_use_only_inside_untrusted_macro(
            &oracle.text,
            owner,
            &locals,
            file_source,
            test.body.as_str(),
        )
    })?;
    let mut assertions = test.assertions.clone();
    for oracle in &mut assertions[first..] {
        demote_macro_held_owner_call(oracle, owner, &locals, file_source, test.body.as_str());
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
            // Without a binding that states its type, `other` is unresolved,
            // so its call keeps the parsed reading; the typed `let other =
            // Other;` case is pinned end to end.
            ("assert_eq!(mystery!(w.new()), other.new())", false),
            ("assert_eq!(mystery!(w.new()), w.new())", false),
            // An unresolved receiver keeps the parsed reading on either side.
            ("assert_eq!(mystery!(w.new()), w.clone().new())", false),
            ("assert_eq!(mystery!(w.clone().new()), Mode::A)", false),
            ("assert_eq!(mystery!(w.clone().new()), other.new())", false),
            ("assert_eq!(mystery!((w).new()), Mode::A)", true),
            // A field receiver is unresolved, so its call stays a free use.
            ("assert_eq!(mystery!(w.new()), holder.w.new())", false),
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
            // #16220: a call of a function-item alias observes the owner.
            (
                "let r = pick(0); let f = pick;",
                "assert_eq!(check_ok!(r), f(0))",
                false,
            ),
            (
                "let f = pick;",
                "assert_eq!(check_ok!(f(0)), Mode::A)",
                true,
            ),
            (
                "let r = pick(0); let f = pick;",
                "assert_eq!(check_ok!(r), f)",
                true,
            ),
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
                true,
            ),
            (
                "let r = (pick(0))?.clone();",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let r = pick(0).unwrap();",
                "assert_eq!(check_ok!(r), r)",
                false,
            ),
            (
                "let r = pick(0).expect(\"picked\");",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let mut seen = Ok(Mode::B); let r = pick(0).inspect(|v| { seen = Ok(v.clone()); });",
                "assert_eq!(check_ok!(r), seen)",
                false,
            ),
            // #16220: a function item cannot capture, so `map(helper)` is
            // transparent and the demotion fires; `unwrap_or` selects rather
            // than invokes, so its path fallback is transparent too.
            (
                "let r = pick(0).map(helper);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let r = pick(0).unwrap_or(fallback);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let r = pick(0).record();",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = pick(0).unwrap(); let actual = &pick(0);",
                "assert_eq!(check_ok!(r), *actual)",
                false,
            ),
            (
                "let r = pick(0).unwrap(); let a = pick(0); let b = &a;",
                "assert_eq!(check_ok!(r), *b)",
                false,
            ),
            (
                "let r = pick(0).unwrap(); let actual = pick(0).map(helper);",
                "assert_eq!(format!(\"{actual:?}{:?}\", check_ok!(r)), \"\")",
                false,
            ),
            (
                "let r = pick(0).unwrap(); let actual = &pick(0);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            // #16220: an unrelated method's name token is not a free
            // observation of the free function, so the demotion still holds.
            (
                "let r = pick(0).unwrap(); let actual = Other.pick(0);",
                "assert_eq!(check_ok!(r), actual)",
                true,
            ),
            (
                "let r = other(pick(0));",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
            (
                "let r = x.pick(0).unwrap();",
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
                "let r = pick(0);",
                "assert_eq!(format!(\"{{{r}}}{}\", check_ok!(r)), \"{Ok(A)}\")",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{:r$}{}\", \"x\", check_ok!(r)), \"x\")",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{:.r$}{}\", 1.0, check_ok!(r)), \"1\")",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{:0r$}{}\", \"x\", check_ok!(r)), \"x\")",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{:pr$}{}\", \"x\", check_ok!(r)), \"x\")",
                true,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(format!(\"{:p1r$}{}\", \"x\", check_ok!(r)), \"x\")",
                true,
            ),
            (
                "",
                "assert_eq!(check_ok!(pick(0)), Holder { pick: 1 })",
                true,
            ),
            (
                "let pick = pick(0);",
                "assert_eq!(check_ok!(pick), Mode::A)",
                true,
            ),
            (
                "let pick = pick(0);",
                "assert_eq!(check_ok!(pick), pick)",
                false,
            ),
            (
                "let r = pick(0); let s = &r;",
                "assert_eq!(check_ok!(Holder { r: r }), *s)",
                false,
            ),
            (
                "let r = pick(0);",
                "assert_eq!(check_ok!(Holder { r: r }), Mode::A)",
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
            let locals = owner_result_locals(&body, &pick, Some(local));
            assert_eq!(
                owner_use_only_inside_untrusted_macro(
                    assertion,
                    &pick,
                    &locals,
                    Some(local),
                    &body
                ),
                *expected,
                "{setup} {assertion}"
            );
        }
    }

    #[test]
    fn a_function_item_argument_is_transparent_but_a_closure_is_not() {
        // #16220: `map(helper)` invokes a function item, which cannot capture,
        // so the local is evidence and the demotion fires. `inspect(|v| ..)`
        // invokes a closure, which can capture and record, so the local is not
        // evidence and the test keeps its parsed reading. `unwrap_or(fallback)`
        // selects rather than invokes, so its path fallback is transparent.
        let pick = OwnerName {
            name: "pick",
            types: Vec::new(),
        };
        let local = "macro_rules! check_ok { ($e:expr) => { 1 }; }";
        let cases: &[(&str, &str, bool)] = &[
            (
                "let r = pick(0).map(helper);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let r = pick(0).unwrap_or(fallback);",
                "assert_eq!(check_ok!(r), Mode::A)",
                true,
            ),
            (
                "let mut seen = Ok(Mode::B); let r = pick(0).inspect(|v| { seen = Ok(v.clone()); });",
                "assert_eq!(check_ok!(r), seen)",
                false,
            ),
            (
                "let mut seen = Ok(Mode::B); let callback = |v: &Mode| { seen = Ok(v.clone()); }; let r = pick(0).inspect(callback);",
                "assert_eq!(check_ok!(r), seen)",
                false,
            ),
            (
                "let r = pick(0).map(|v| v);",
                "assert_eq!(check_ok!(r), Mode::A)",
                false,
            ),
        ];
        for (setup, assertion, expected) in cases {
            let body = format!("fn t() {{ {setup} {assertion}; }}");
            let locals = owner_result_locals(&body, &pick, Some(local));
            assert_eq!(
                owner_use_only_inside_untrusted_macro(
                    assertion,
                    &pick,
                    &locals,
                    Some(local),
                    &body
                ),
                *expected,
                "{setup} {assertion}"
            );
        }
    }

    #[test]
    fn a_field_label_or_member_does_not_make_an_alias_carry_the_local() {
        // #16220: `Holder { r: .. }.r` names a field, not a read of the local
        // `r`, so `actual` does not carry the owner's result and the demotion
        // still fires. The real borrow control stays tracked.
        let pick = OwnerName {
            name: "pick",
            types: Vec::new(),
        };
        let local = "macro_rules! check_ok { ($e:expr) => { 1 }; }";
        let cases: &[(&str, &str, bool)] = &[
            (
                "let r = pick(0).unwrap(); let actual = Holder { r: Ok(Mode::A) }.r;",
                "assert_eq!(check_ok!(r), actual)",
                true,
            ),
            (
                "let r = pick(0).unwrap(); let actual = Holder { pick: Ok(Mode::A) }.pick;",
                "assert_eq!(check_ok!(r), actual)",
                true,
            ),
            (
                "let r = pick(0).unwrap(); let actual = &pick(0);",
                "assert_eq!(check_ok!(r), *actual)",
                false,
            ),
        ];
        for (setup, assertion, expected) in cases {
            let body = format!("fn t() {{ {setup} {assertion}; }}");
            let locals = owner_result_locals(&body, &pick, Some(local));
            assert_eq!(
                owner_use_only_inside_untrusted_macro(
                    assertion,
                    &pick,
                    &locals,
                    Some(local),
                    &body
                ),
                *expected,
                "{setup} {assertion}"
            );
        }
    }

    #[test]
    fn a_method_owner_field_access_is_not_a_call() {
        // #16220: with a method owner, `Holder { pick: .. }.pick` is a field
        // access, not a call of the owner, so `actual` does not carry the
        // owner's result and the demotion still fires. The method call
        // `picker.pick(0)` and the real borrow control stay tracked.
        let pick = OwnerName {
            name: "pick",
            types: vec!["Picker".to_string()],
        };
        let local = "macro_rules! check_ok { ($e:expr) => { 1 }; }";
        let cases: &[(&str, &str, bool)] = &[
            (
                "let r = picker.pick(0).unwrap(); let actual = Holder { pick: Ok(Mode::A) }.pick;",
                "assert_eq!(check_ok!(r), actual)",
                true,
            ),
            (
                "let r = picker.pick(0).unwrap(); let actual = &picker.pick(0);",
                "assert_eq!(check_ok!(r), *actual)",
                false,
            ),
            (
                "let r = picker.pick(0).unwrap(); let m = cfg.pick;",
                "assert_eq!(check_ok!(r), m)",
                true,
            ),
        ];
        for (setup, assertion, expected) in cases {
            let body = format!("fn t() {{ {setup} {assertion}; }}");
            let locals = owner_result_locals(&body, &pick, Some(local));
            assert_eq!(
                owner_use_only_inside_untrusted_macro(
                    assertion,
                    &pick,
                    &locals,
                    Some(local),
                    &body
                ),
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
