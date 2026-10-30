//! Scope-end carrier authority for a standard `Drop::drop` (#10814).
//!
//! A destructor runs at the end of a scope no test can name, so the ordinary
//! sibling-effect read credits nothing about it (#10795). One shape does tie an
//! assertion to that scope end, and this is the only one admitted: the test
//! creates one local shared handle, builds one record-literal guard of the
//! owner's type whose field is `handle.clone()`, ends the guard's scope with a
//! top-level `{ let g = Guard { .. }; }` block or a top-level `drop(g);`, and
//! then asserts, at the top level and in order, on a wrapper of that same
//! handle. The destructor's changed write is the only thing that can move the
//! handle between its fresh state and the asserted one.
//!
//! Everything else keeps the destructor veto: a different or re-created cell,
//! a guard that is aliased, moved, leaked or held in a closure, a write or
//! rebind of either handle after the scope, an assertion before the scope, a
//! shadowed `drop`, a second `Drop` impl of the type, a macro that could add
//! one, and any earlier top-level statement that can end the test before the
//! scope (`return`, `?`, `break`, `process::exit`).
//!
//! The decision is shared: the classifier asks it to confirm an assertion
//! ([`assertion_observes_scope_end`]) and the destructor gate asks it again
//! before it lets an `exposed` verdict stand ([`destructor_credit_established`]).

use super::{
    AssertionSite, ImplicitExpansionContext, assertion_site, compact, guard_literal_handle,
    handle_is_fresh_and_pinned, mentions, plain_receiver_type, shared_field,
};
use crate::analysis::classify::related_tests::{
    standard_drop_call_unshadowed, test_drops_destructor_value,
};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasAttrs, HasName},
};

/// Whether `assertion` in `test` reads the shared handle a guard of the
/// owner's `Drop` type wrote when the test ended its scope.
pub(in crate::analysis) fn assertion_observes_scope_end(
    owner: &FunctionSummary,
    expression: &str,
    test: &TestSummary,
    assertion: &OracleFact,
    source: &str,
    expansion: &ImplicitExpansionContext<'_>,
) -> bool {
    let index = expansion.index;
    let FunctionContainer::TraitImpl { self_ty, .. } = &owner.item.container else {
        return false;
    };
    // The relation owns the owned-value, leak, shadow and scope-clearing
    // refusals; this authority adds the assertion-local carrier on top.
    if !test_drops_destructor_value(test, owner, Some(source), index)
        || !plain_receiver_type(source, self_ty, |ty| {
            expansion.memo.admits_drop_owner(index, ty)
        })
        || !plain_test_attributes(test)
    {
        return false;
    }
    let Some(field) = shared_field(owner, owner, expression) else {
        return false;
    };
    let Some(site) = assertion_site(test, assertion, source) else {
        return false;
    };
    if site.visible.iter().any(can_end_the_test_early) {
        return false;
    }
    let drop_call_is_std = standard_drop_call_unshadowed(source, &test.file, index);
    scope_ends_over_shared_handle(&site, self_ty, &field, expression, drop_call_is_std)
}

/// Whether an `exposed` verdict on a standard destructor stands: the probe is
/// a write the carrier reads, and every contributing strong related test is one
/// whose assertion the carrier confirms. Any row the carrier does not prove
/// keeps the veto (#10553).
pub(in crate::analysis) fn destructor_credit_established(
    finding: &crate::domain::Finding,
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    use crate::domain::{OracleStrength, ProbeFamily, RelatedTestRole};
    if !matches!(
        finding.probe.family,
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion
    ) {
        return false;
    }
    // A row cut from the display cannot be checked, so "every strong row is
    // confirmed" does not hold (the cut check `reached_only_through_syntax`
    // makes, #10551).
    if finding
        .related_tests_matched_total
        .is_some_and(|total| finding.related_tests.len() < total)
    {
        return false;
    }
    let memo = super::ImplicitExpansionMemo::default();
    let expansion = ImplicitExpansionContext::new(index, &memo);
    let mut credited = false;
    for row in &finding.related_tests {
        if row.role != RelatedTestRole::Contributing
            || row.miss.is_some()
            || row.oracle_strength != OracleStrength::Strong
        {
            continue;
        }
        let carried = row.oracle.as_deref().is_some_and(|oracle| {
            index
                .tests()
                .iter()
                .filter(|test| {
                    test.file == row.file && test.name == row.name && test.start_line == row.line
                })
                .any(|test| {
                    let Some(facts) = index.files().get(&test.file) else {
                        return false;
                    };
                    test.assertions
                        .iter()
                        .filter(|assertion| assertion.text == oracle)
                        .all(|assertion| {
                            assertion_observes_scope_end(
                                owner,
                                &finding.probe.expression,
                                test,
                                assertion,
                                &facts.source,
                                &expansion,
                            )
                        })
                        && test
                            .assertions
                            .iter()
                            .any(|assertion| assertion.text == oracle)
                })
        });
        if !carried {
            return false;
        }
        credited = true;
    }
    credited
}

/// A test the harness runs exactly once and reports as it ends: no
/// `should_panic`, `ignore`, async runtime or `cfg` attribute.
fn plain_test_attributes(test: &TestSummary) -> bool {
    test.attrs.iter().all(|attr| compact(attr) == "#[test]")
}

/// Whether the statement can leave the test before its later statements run,
/// so the scope and the assertion after it may never be reached. Token-level so
/// a `?` inside a macro argument counts.
fn can_end_the_test_early(statement: &ast::Stmt) -> bool {
    statement
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| match token.kind() {
            SyntaxKind::RETURN_KW
            | SyntaxKind::QUESTION
            | SyntaxKind::BREAK_KW
            | SyntaxKind::CONTINUE_KW
            | SyntaxKind::YIELD_KW => true,
            SyntaxKind::IDENT => matches!(token.text(), "exit" | "abort"),
            _ => false,
        })
}

fn scope_ends_over_shared_handle(
    site: &AssertionSite,
    self_ty: &str,
    field: &str,
    expression: &str,
    drop_call_is_std: bool,
) -> bool {
    let visible = &site.visible;
    for (at, statement) in visible.iter().enumerate() {
        // `{ let g = Guard { .. }; }`: the guard lives only inside the block.
        if let Some(inner) = plain_guard_block(statement)
            && guard_binding(&inner, self_ty, field).is_some_and(|(receiver, local)| {
                guard_scope_carries_the_assertion(site, at, &receiver, &local, expression, 0)
            })
        {
            return true;
        }
        // `let g = Guard { .. };` then a top-level `drop(g);`.
        if drop_call_is_std
            && let Some((receiver, local)) = guard_binding(statement, self_ty, field)
            && guard_scope_carries_the_assertion(site, at, &receiver, &local, expression, 1)
        {
            return true;
        }
    }
    false
}

/// The block's only statement when the statement is a plain `{ .. }` holding
/// exactly one `let` and no tail expression.
fn plain_guard_block(statement: &ast::Stmt) -> Option<ast::Stmt> {
    let ast::Stmt::ExprStmt(expr) = statement else {
        return None;
    };
    let ast::Expr::BlockExpr(block) = expr.expr()? else {
        return None;
    };
    // An attribute (`#[cfg(any())]`) can remove the block from the test.
    if block.label().is_some() || block.modifier().is_some() || block.attrs().next().is_some() {
        return None;
    }
    let list = block.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [only] = statements.as_slice() else {
        return None;
    };
    (list.tail_expr().is_none() && matches!(only, ast::Stmt::LetStmt(_))).then(|| only.clone())
}

/// The guard's name and the handle it clones, from `let [mut] g = Guard { f:
/// handle.clone() };`.
fn guard_binding(statement: &ast::Stmt, self_ty: &str, field: &str) -> Option<(String, String)> {
    let ast::Stmt::LetStmt(binding) = statement else {
        return None;
    };
    let Some(ast::Pat::IdentPat(pattern)) = binding.pat() else {
        return None;
    };
    if pattern.ref_token().is_some() || binding.let_else().is_some() {
        return None;
    }
    let receiver = pattern.name()?.text().to_string();
    let local = guard_literal_handle(&binding.initializer()?, self_ty, field)?;
    Some((receiver, local))
}

/// The statement at `at` introduces the guard (`explicit_drop` is `1`: a
/// top-level `drop(g);` must follow; `0`: the statement is the whole block).
/// The handle is fresh before it, and nothing but that scope end touches the
/// handle or the guard before the asserted wrapper reads it.
fn guard_scope_carries_the_assertion(
    site: &AssertionSite,
    at: usize,
    receiver: &str,
    local: &str,
    expression: &str,
    explicit_drop: usize,
) -> bool {
    let visible = &site.visible;
    if local != site.observed_local
        || visible[..at]
            .iter()
            .any(|statement| mentions(statement.syntax(), receiver))
        || !handle_is_fresh_and_pinned(&visible[..at], local, expression, &site.expected_value)
    {
        return false;
    }
    let mut dropped = explicit_drop == 0;
    for next in &visible[at + 1..] {
        if mentions(next.syntax(), local) {
            return false;
        }
        if mentions(next.syntax(), receiver) {
            if !dropped && is_drop_of(next, receiver) {
                dropped = true;
            } else {
                return false;
            }
        }
    }
    dropped
}

/// `drop(g);` / `mem::drop(g);` / `std::mem::drop(g);` as a whole statement.
fn is_drop_of(statement: &ast::Stmt, receiver: &str) -> bool {
    let text = compact(&statement.syntax().text().to_string());
    ["", "mem::", "std::mem::", "core::mem::"]
        .iter()
        .any(|path| text == format!("{path}drop({receiver});"))
}
