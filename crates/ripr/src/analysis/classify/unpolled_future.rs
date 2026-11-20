//! RIPR-SPEC-0240 / #13702: a future created but never polled is no reach.
//!
//! Calling an `async fn` only builds a future; the body, and so the changed
//! line, runs when something polls it. A synchronous `#[test]` that writes
//! `let _f = fetch_len("abc");` and drops the future never runs the owner, yet
//! the direct-call relation still read as reach `yes` and the finding as a
//! `reachable_unrevealed` gap.
//!
//! This rule names that as `rust_future_never_polled`. It fires only when
//! every test that names the owner, and every related test, is a directly
//! related plain synchronous `#[test]` whose every owner call builds a future
//! that is provably dropped unpolled:
//!
//! - the call is a bare expression statement (`fetch_len("abc");`), the sole
//!   argument of `drop(..)`, or the initializer of a `let` whose pattern is
//!   `_` or a single name the test never mentions again; and
//! - the test body holds no `.await`, `async` block, executor or runtime
//!   spelling (`block_on`, `block_in_place`, `Runtime`, `Handle`, `spawn`,
//!   `poll`), and every mention of the owner's name is one of those calls (a
//!   mention inside a macro, or a closure whose tail expression is the call,
//!   keeps today's reading; a closure whose body drops the call as a
//!   statement still drops it unpolled).
//!
//! Every related test, whatever its relation, must be one of those judged
//! tests, and every spelling of the owner's name anywhere in the indexed files
//! must be the owner's declaration, a judged test body or a plain `use` leaf:
//! a `use … as …` alias, a `macro_rules!` naming the owner or any other
//! spelling keeps today's reading. A bare `drop(..)` counts only when no
//! user-defined `drop` is indexed or brought into the test's file.
//!
//! An async test attribute (`#[tokio::test]`, `#[async_std::test]`), any
//! attribute other than `#[test]` and lint attributes, an unreadable body, an
//! owner whose declaration is not readable as `async`, and a helper relation
//! all keep today's reading. Like every RIPR-SPEC-0240 rule this only
//! withholds: nothing is credited, no runtime is chosen, and the actionability
//! flip stays fail-closed.

use super::decision::confidence_score;
use super::gap_admission::{is_ignored, withhold_gap};
use super::owner_pin::function_declaration_is_async;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionImplContext;
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::domain::*;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasName},
};

const LIMIT: StaticLimitKind = StaticLimitKind::RustFutureNeverPolled;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether anything polls the future the test creates from the async owner";
const JUDGED_TEST_LIMIT: usize = 3;

/// Spellings that may drive a future to completion. Any of them in a test
/// body keeps today's reading; the rule never decides which one would run it.
///
/// This is a redundant extra guard, not the deciding check: a future can only
/// be polled through its value, and the value of every owner call is already
/// followed by [`future_is_dropped`] (an `.await`, an executor or helper
/// argument, or a binding mentioned again all answer "not dropped"), and every
/// other spelling of the owner fails the mention count. No shape is known that
/// only this list refuses; it stays as a second, independent refusal.
const POLLING_SPELLINGS: [&str; 7] = [
    "await",
    "block_on",
    "block_in_place",
    "Runtime",
    "Handle",
    "spawn",
    "poll",
];

/// Withhold the gap of an async owner whose only reaching tests create its
/// future and drop it unpolled. Returns `true` when the finding was rewritten
/// to `static_unknown`.
pub(in crate::analysis) fn withhold_unpolled_future_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || finding.ripr.reach.state != StageState::Yes
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed
                | ExposureClass::ReachableUnrevealed
                | ExposureClass::InfectionUnknown
                | ExposureClass::PropagationUnknown
        )
    {
        return false;
    }
    let Some(owner) = enclosing_owner(finding, index) else {
        return false;
    };
    if function_declaration_is_async(owner, index) != Some(true) {
        return false;
    }
    let Some(judged) = unpolled_reaching_tests(finding, owner, index) else {
        return false;
    };
    let labels: Vec<String> = judged.iter().map(|test| label(test)).collect();
    let shown = labels
        .iter()
        .take(JUDGED_TEST_LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let (creates, calls, them) = if labels.len() == 1 {
        ("creates", "calls", "it")
    } else {
        ("create", "call", "them")
    };
    let next_step = format!(
        "{shown} {creates} the future of `async fn {name}` but never {polls} it (no `.await`, executor or async test attribute), so the changed line does not run in {them} and ripr does not report a gap. Make a test await the call in an async test, or drive it with an executor, and assert on its result.",
        name = owner.name,
        polls = if labels.len() == 1 { "polls" } else { "poll" },
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    // Reach and infection were read from tests in which the owner's body never
    // runs. Propagation stays: it reads the owner's own code, not the tests.
    let stale = [
        finding.ripr.reach.summary.clone(),
        finding.ripr.infect.summary.clone(),
    ];
    finding.ripr.reach = StageEvidence::new(
        StageState::Unknown,
        Confidence::Low,
        format!(
            "future created but never polled: {shown} {calls} `{}` without `.await` or an executor, so the async body does not run",
            owner.name
        ),
    );
    if matches!(
        finding.ripr.infect.state,
        StageState::Yes | StageState::Weak
    ) {
        finding.ripr.infect = StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "Not established: the future the related tests create is never polled, so the changed line does not run",
        );
    }
    finding
        .evidence
        .retain(|line| !stale.iter().any(|summary| line == summary));
    let ripr = &finding.ripr;
    finding.confidence = finding.confidence.min(confidence_score(
        &ripr.reach,
        &ripr.infect,
        &ripr.propagate,
        &ripr.reveal.observe,
        &ripr.reveal.discriminate,
        &finding.class,
    ));
    true
}

fn enclosing_owner<'a>(finding: &Finding, index: &'a RustIndex) -> Option<&'a FunctionSummary> {
    let owner_id = finding.probe.owner.as_ref()?;
    let line = finding.probe.location.line;
    let mut owners = index.functions().iter().filter(|function| {
        function.id == *owner_id && function.start_line <= line && line <= function.end_line
    });
    match (owners.next(), owners.next()) {
        (Some(owner), None) => Some(owner),
        _ => None,
    }
}

fn label(test: &TestSummary) -> String {
    format!(
        "`{}` ({}:{})",
        test.name,
        test.file.display().to_string().replace('\\', "/"),
        test.start_line
    )
}

/// The tests that reach the owner, when every one of them only drops an
/// unpolled future: every related test, whatever its relation, is a direct
/// owner call, no indexed file spells the owner outside its declaration, a
/// judged test or a plain `use` leaf, and every indexed test that names the owner at all is
/// listed and judged unpolled. `None` keeps today's reading.
fn unpolled_reaching_tests<'a>(
    finding: &Finding,
    owner: &FunctionSummary,
    index: &'a RustIndex,
) -> Option<Vec<&'a TestSummary>> {
    // Every related test, whatever its relation, must be one of the judged
    // tests: a related test that is not judged (a helper, an alias or macro
    // path, a relation that does not itself establish reach) may be the one
    // that polls the owner without spelling its name in its own body.
    let mut reaching = Vec::new();
    for related in &finding.related_tests {
        if related.relation_reason? != RelationReason::DirectOwnerCall {
            return None;
        }
        let test = index.tests().iter().find(|test| {
            test.name == related.name
                && test.file == related.file
                && test.start_line == related.line
        })?;
        reaching.push(test);
    }
    if reaching.is_empty() {
        return None;
    }
    // The related list is capped: any indexed test that names the owner and
    // is not listed may be the one that polls it.
    for test in index.tests() {
        let named = names_identifier(&mask_comments_and_strings(&test.body), &owner.name);
        if named && !reaching.iter().any(|seen| std::ptr::eq(*seen, test)) {
            return None;
        }
    }
    // Any other function that names the owner (a helper, a wrapper that
    // drives it) may be another path to it that this rule does not read.
    let is_reaching_test = |function: &FunctionSummary| {
        reaching.iter().any(|test| {
            test.file == function.file
                && (test.start_line == function.start_line || test.body == function.body)
        })
    };
    if index.functions().iter().any(|function| {
        function.id != owner.id
            && !is_reaching_test(function)
            && names_identifier(&mask_comments_and_strings(&function.body), &owner.name)
    }) {
        return None;
    }
    // A test can reach the owner without spelling its name in its own body:
    // through a `use … as …` alias, a `macro_rules!` whose body names it, a
    // `const`/`static` holding it, or a renamed re-export. Every spelling of
    // the owner anywhere in the indexed files must be the owner's own
    // declaration, a judged test body, or a plain (not renamed) `use` leaf.
    if !owner_spelled_only_where_judged(owner, &reaching, index) {
        return None;
    }
    reaching
        .iter()
        .all(|test| test_drops_owner_future_unpolled(test, &owner.name, index))
        .then_some(reaching)
}

/// Whether every identifier token spelling `owner` in every indexed file sits
/// in the owner's own span, in a judged test's span, or is the last segment of
/// a `use` tree that does not rename it. An unparseable file that spells the
/// name, or a file whose source the index does not hold, answers `false`.
fn owner_spelled_only_where_judged(
    owner: &FunctionSummary,
    judged: &[&TestSummary],
    index: &RustIndex,
) -> bool {
    for (path, view) in index.files().iter() {
        let source = &view.data().source;
        if source.is_empty() {
            if view.functions.is_empty() && view.tests.is_empty() {
                continue;
            }
            return false;
        }
        if !names_identifier(source, &owner.name) {
            continue;
        }
        let Some(snapshot) = index.source_syntax.get(index, path) else {
            return false;
        };
        let Some(tree) = snapshot.tree() else {
            return false;
        };
        let in_span = |file: &std::path::Path, start: usize, end: usize, line: usize| {
            file == path.as_path() && (start..=end).contains(&line)
        };
        let all_allowed = identifier_tokens(tree.syntax(), &owner.name).all(|token| {
            let line = snapshot.line(token.text_range().start());
            in_span(&owner.file, owner.start_line, owner.end_line, line)
                || judged
                    .iter()
                    .any(|test| in_span(&test.file, test.start_line, test.end_line, line))
                || is_plain_use_leaf(&token)
        });
        if !all_allowed {
            return false;
        }
    }
    true
}

/// The identifier tokens of `root` (including those inside macro token trees)
/// whose text, without a raw `r#` prefix, is `name`.
fn identifier_tokens<'a>(
    root: &SyntaxNode,
    name: &'a str,
) -> impl Iterator<Item = ra_ap_syntax::SyntaxToken> + use<'a> {
    root.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(move |token| {
            token.kind() == ra_ap_syntax::SyntaxKind::IDENT
                && token.text().trim_start_matches("r#") == name
        })
}

/// `token` is the last path segment of a `use` tree with no rename, so a test
/// that calls through this import still spells the name.
fn is_plain_use_leaf(token: &ra_ap_syntax::SyntaxToken) -> bool {
    let Some(segment) = token
        .parent()
        .and_then(ast::NameRef::cast)
        .and_then(|name| name.syntax().parent())
        .and_then(ast::PathSegment::cast)
    else {
        return false;
    };
    let Some(path) = segment.syntax().parent().and_then(ast::Path::cast) else {
        return false;
    };
    let Some(tree) = path.syntax().parent().and_then(ast::UseTree::cast) else {
        return false;
    };
    tree.path().is_some_and(|own| own == path)
        && path.segment().is_some_and(|last| last == segment)
        && tree.rename().is_none()
        && tree.star_token().is_none()
        && tree.use_tree_list().is_none()
}

/// Whether a bare `drop(..)` call in `test` can only be the prelude's
/// `std::mem::drop`: no free or unknown-context function, test or nested fn
/// named `drop` is indexed, and in the test's file every `drop` identifier is a
/// reference (not a binding, a rename, a macro token or a `use` other than
/// `std::mem::drop`/`core::mem::drop`) or the method of a trait impl.
fn bare_drop_is_prelude(test: &TestSummary, index: &RustIndex) -> bool {
    const DROP: &str = "drop";
    let user_drop_fn = index.functions().iter().any(|function| {
        (function.name == DROP
            && !matches!(function.impl_context, FunctionImplContext::Impl { .. }))
            || function.nested_fn_names.iter().any(|name| name == DROP)
    });
    let user_drop_test = index
        .tests()
        .iter()
        .any(|other| other.name == DROP || other.nested_fn_names.iter().any(|name| name == DROP));
    if user_drop_fn || user_drop_test {
        return false;
    }
    let Some(snapshot) = index.source_syntax.get(index, &test.file) else {
        return false;
    };
    let Some(tree) = snapshot.tree() else {
        return false;
    };
    identifier_tokens(tree.syntax(), DROP).all(|token| {
        let Some(parent) = token.parent() else {
            return false;
        };
        if let Some(name_ref) = ast::NameRef::cast(parent.clone()) {
            // A `use` leaf names `drop` into scope; only the std one is known.
            return match name_ref.syntax().ancestors().find_map(ast::UseTree::cast) {
                Some(tree) => {
                    tree.rename().is_none()
                        && tree.path().is_some_and(|path| {
                            let text = path.syntax().text().to_string();
                            text == "std::mem::drop" || text == "core::mem::drop"
                        })
                }
                None => true,
            };
        }
        // `fn drop(&mut self)` of a trait impl (`impl Drop for T`) cannot be
        // called as a bare `drop(..)`.
        ast::Name::cast(parent)
            .and_then(|name| name.syntax().parent())
            .and_then(ast::Fn::cast)
            .and_then(|function| function.syntax().parent())
            .and_then(ast::AssocItemList::cast)
            .and_then(|list| list.syntax().parent())
            .and_then(ast::Impl::cast)
            .is_some_and(|item| item.trait_().is_some())
    })
}

/// Whether `test` is a plain synchronous `#[test]` whose every mention of
/// `owner` is a call that builds a future the test drops unpolled.
fn test_drops_owner_future_unpolled(test: &TestSummary, owner: &str, index: &RustIndex) -> bool {
    if test.generated.is_some() || is_ignored(test) || !plain_sync_test_attributes(&test.attrs) {
        return false;
    }
    let code = mask_comments_and_strings(&test.body);
    if POLLING_SPELLINGS
        .iter()
        .any(|spelling| names_identifier(&code, spelling))
        || names_identifier(&code, "async")
    {
        return false;
    }
    let Some(parse) = index.source_syntax.parse_fragment(&test.body) else {
        return false;
    };
    let syntax = parse.tree().syntax().clone();
    let Some(function) = syntax.children().find_map(ast::Fn::cast) else {
        return false;
    };
    if function.async_token().is_some() || function.name().is_some_and(|name| name.text() == owner)
    {
        return false;
    }
    let Some(body) = function.body() else {
        return false;
    };
    let calls: Vec<SyntaxNode> = body
        .syntax()
        .descendants()
        .filter(|node| call_name(node).as_deref() == Some(owner))
        .collect();
    if calls.is_empty() {
        return false;
    }
    // Every spelling of the owner in the body must be one of these calls: a
    // mention inside a macro or as a value is not read here.
    let body_code = mask_comments_and_strings(&body.syntax().text().to_string());
    if identifier_count(&body_code, owner) != calls.len() {
        return false;
    }
    let bare_drop_ok = bare_drop_is_prelude(test, index);
    calls
        .iter()
        .all(|call| future_is_dropped(call, &body_code, bare_drop_ok))
}

/// Only `#[test]` plus lint attributes: an async test attribute, a runtime
/// attribute or any macro attribute may poll the body's futures.
fn plain_sync_test_attributes(attrs: &[String]) -> bool {
    let mut has_test = false;
    for attr in attrs {
        let compact: String = attr.chars().filter(|ch| !ch.is_whitespace()).collect();
        if compact == "#[test]" {
            has_test = true;
        } else if !["#[allow(", "#[expect(", "#[warn(", "#[deny(", "#[doc"]
            .iter()
            .any(|prefix| compact.starts_with(prefix))
        {
            return false;
        }
    }
    has_test
}

/// The callee name of a free, path or method call.
fn call_name(node: &SyntaxNode) -> Option<String> {
    if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
        return call.name_ref().map(|name| name.text().to_string());
    }
    let call = ast::CallExpr::cast(node.clone())?;
    let ast::Expr::PathExpr(callee) = call.expr()? else {
        return None;
    };
    callee
        .path()?
        .segment()?
        .name_ref()
        .map(|name| name.text().to_string())
}

/// The call's future is dropped without being polled: it is an expression
/// statement, the sole argument of `std::mem::drop(..)` / `core::mem::drop(..)`
/// (or of a bare `drop(..)` when `bare_drop_ok` says it can only be the
/// prelude's), or the initializer of a `let` binding `_` or a name `body_code`
/// never mentions again. Parentheses are transparent; every other consumer may
/// poll it.
fn future_is_dropped(call: &SyntaxNode, body_code: &str, bare_drop_ok: bool) -> bool {
    let mut current = call.clone();
    while let Some(parent) = current.parent() {
        if ast::ParenExpr::can_cast(parent.kind()) {
            current = parent;
            continue;
        }
        if let Some(statement) = ast::ExprStmt::cast(parent.clone()) {
            return statement.semicolon_token().is_some();
        }
        if let Some(binding) = ast::LetStmt::cast(parent.clone()) {
            if binding.let_else().is_some()
                || binding
                    .initializer()
                    .is_none_or(|init| *init.syntax() != current)
            {
                return false;
            }
            return match binding.pat() {
                Some(ast::Pat::WildcardPat(_)) => true,
                Some(ast::Pat::IdentPat(name)) => {
                    // `r#f` and `f` are one name: count it without the prefix.
                    name.pat().is_none()
                        && name.name().is_some_and(|name| {
                            identifier_count(body_code, name.text().trim_start_matches("r#")) == 1
                        })
                }
                _ => false,
            };
        }
        let Some(arguments) = ast::ArgList::cast(parent) else {
            return false;
        };
        let Some(outer) = arguments.syntax().parent().and_then(ast::CallExpr::cast) else {
            return false;
        };
        // A user-defined `drop` may poll its argument: a bare `drop` counts
        // only when the caller established it is the prelude's.
        let is_drop = matches!(
            outer.expr(),
            Some(ast::Expr::PathExpr(path)) if (bare_drop_ok && path.syntax().text() == "drop")
                || path.syntax().text() == "std::mem::drop"
                || path.syntax().text() == "core::mem::drop"
        );
        let sole = arguments.args().count() == 1;
        if !is_drop || !sole {
            return false;
        }
        return outer
            .syntax()
            .parent()
            .and_then(ast::ExprStmt::cast)
            .is_some_and(|statement| statement.semicolon_token().is_some());
    }
    false
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// How often `name` appears as a whole identifier in `code`.
fn identifier_count(code: &str, name: &str) -> usize {
    if name.is_empty() {
        return 0;
    }
    code.match_indices(name)
        .filter(|(at, _)| {
            code[..*at]
                .chars()
                .next_back()
                .is_none_or(|ch| !is_ident_char(ch))
                && code[at + name.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !is_ident_char(ch))
        })
        .count()
}

fn names_identifier(code: &str, name: &str) -> bool {
    identifier_count(code, name) > 0
}

#[cfg(test)]
mod tests;
