//! Polling-aware helper-chain admission above an owner (#7415, #11065, #11151).
//!
//! Calling an `async fn` constructs a future; its body, and so the owner it
//! calls, runs only when something polls it. The helper chain is resolved per
//! probe, so the first non-synchronous hop cuts the chain by default and a test
//! that merely constructs or drops the future earns no helper credit. A test
//! that provably drives the hop earns it back:
//!
//! - the call is the operand of `.await` in code the test function itself
//!   executes (an `async` test body such as `#[tokio::test]`; an `async` block
//!   or closure only declares a future and is refused, the same boundary
//!   `node_executes_in` draws for assertion operands); or
//! - the call is the single argument of a test-local helper that the
//!   RIPR-SPEC-0197 poll-helper reader verified polls one future to
//!   completion, bound to the actual verified definition. A third-party
//!   `block_on` (`futures::executor`, `tokio::runtime`) is a name, not a
//!   proof, and is refused.
//!
//! Every async hop between the test and the owner must be driven too: the
//! test polls the entry hop it calls, and each `async` hop's body must
//! `.await` its callee. A declaration that cannot be read as either `async` or
//! synchronous, a call that is held in a binding, spawned, joined or passed on,
//! and any same-named call the test leaves unpolled all fail closed.
//!
//! An owner that may return a future is driven like an `async fn` owner: the
//! lowest hop must `.await` its call. That is an `async fn`, an unreadable
//! declaration, and a plain `fn` whose return type names `Future` or `Pin` or
//! whose body holds an `async` block or closure (`fn f() -> impl Future { async
//! move { .. } }` runs its changed line only when polled). A return type behind
//! a type alias is not seen.
//!
//! The hop-side shadow guard is name-based and fail closed. It refuses local
//! bindings, nested `fn`s, any `use` that binds the callee's name (by final
//! segment or `as` rename, so a hop that imports its callee at all, such as
//! `use ext::inner;` or `use crate::inner;`, is refused by design) and ANY
//! glob `use` in the hop's file. Recall cost: a hop file with a glob import,
//! including a `use crate::*;` prelude, is not credited. A glob inside a
//! `#[cfg(test)]` module that does not contain the hop (the in-file
//! `mod tests { use super::*; }`) is exempt. Not detected: imports in other
//! files, a qualified path to an external item of the same name, name binders
//! other than `use` (a const, static, tuple-struct constructor,
//! `extern crate .. as inner`, macro-generated items) and a `use` inside a
//! `macro_rules!` body.

use super::helper_transfer::HelperChain;
use super::owner_pin::{OwnerPinSyntax, function_declaration_is_async};
use super::related_tests::{node_executes_in, order_related};
use super::{DependencyEdgeContext, RelatedTestCandidateIndex};
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::domain::{Probe, RelationReason};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasName},
};
use std::cell::OnceCell;
use std::collections::BTreeSet;

/// The related tests and the chain activation should read.
pub(in crate::analysis) struct PolledHops<'a> {
    pub(in crate::analysis) related: Vec<(&'a TestSummary, RelationReason)>,
    pub(in crate::analysis) chain: Option<HelperChain>,
}

/// Position of the first hop that is not provably synchronous. Hops run
/// owner-upward, so everything below it is a synchronous call path.
fn first_non_sync_hop(chain: &HelperChain, index: &RustIndex) -> Option<usize> {
    chain
        .hops
        .iter()
        .position(|hop| function_declaration_is_async(&hop.caller, index) != Some(false))
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the related-test finder's explicit inputs"
)]
pub(in crate::analysis) fn related_through_polled_hops<'a>(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    index: &'a RustIndex,
    workspace_complete: bool,
    resolved: Option<HelperChain>,
    dependency_edges: Option<&DependencyEdgeContext<'_>>,
    candidate_index: &RelatedTestCandidateIndex,
) -> PolledHops<'a> {
    let cut = resolved
        .as_ref()
        .and_then(|chain| first_non_sync_hop(chain, index));
    // The default: truncate at the first non-synchronous hop. Empty-hop chains
    // stay off the relation/activation path (fail closed).
    let base = resolved
        .clone()
        .map(|mut chain| {
            if let Some(cut) = cut {
                chain.hops.truncate(cut);
            }
            chain
        })
        .filter(|chain| !chain.hops.is_empty());
    let mut related = super::find_related_tests_with_candidate_index(
        probe,
        owner_fn,
        index,
        workspace_complete,
        base.as_ref(),
        dependency_edges,
        candidate_index,
    );
    let (Some(full), Some(cut)) = (resolved.as_ref(), cut) else {
        return PolledHops {
            related,
            chain: base,
        };
    };
    let through = super::find_related_tests_with_candidate_index(
        probe,
        owner_fn,
        index,
        workspace_complete,
        Some(full),
        dependency_edges,
        candidate_index,
    );
    let Some(owner) = owner_fn else {
        return PolledHops {
            related,
            chain: base,
        };
    };
    let oracle = PollOracle {
        index,
        syntax: candidate_index.owner_pin_syntax(),
        verified: OnceCell::new(),
    };
    let mut top: Option<usize> = None;
    let mut changed = false;
    for (test, reason) in through {
        if reason != RelationReason::HelperOwnerCall
            || related.iter().any(|(seen, kept)| {
                std::ptr::eq(*seen, test) && *kept == RelationReason::HelperOwnerCall
            })
        {
            continue;
        }
        let Some(entry) = polled_entry(full, cut, owner, test, &oracle) else {
            continue;
        };
        top = top.max(Some(entry));
        match related
            .iter_mut()
            .find(|(seen, _)| std::ptr::eq(*seen, test))
        {
            Some(slot) => slot.1 = reason,
            None => related.push((test, reason)),
        }
        changed = true;
    }
    if changed {
        order_related(&mut related, candidate_index);
    }
    let chain = match top {
        Some(top) => Some(HelperChain {
            hops: full.hops[..=top].to_vec(),
            stop_above: full.stop_above.clone(),
        }),
        None => base,
    };
    PolledHops { related, chain }
}

struct PollOracle<'a> {
    index: &'a RustIndex,
    syntax: &'a OwnerPinSyntax,
    verified: OnceCell<BTreeSet<String>>,
}

impl PollOracle<'_> {
    /// `name` is a test-local poll-to-completion helper bound to its verified
    /// definition (RIPR-SPEC-0197), not merely spelled like one.
    fn drives(&self, test: &TestSummary, name: &str) -> bool {
        self.verified
            .get_or_init(|| super::poll_helper::verified_helpers(self.index))
            .contains(name)
            && super::poll_helper::binding_is_verified(test, name, self.index, self.syntax)
    }
}

/// The highest async-region hop `test` enters, provided every call the test
/// makes to an async-region hop is provably polled and every async hop under
/// the entry polls its own callee. `None` when the test enters none or any
/// driven call is unproven.
fn polled_entry(
    chain: &HelperChain,
    cut: usize,
    owner: &FunctionSummary,
    test: &TestSummary,
    oracle: &PollOracle<'_>,
) -> Option<usize> {
    let parse = oracle.index.source_syntax.parse_fragment(&test.body)?;
    let syntax = parse.tree().syntax().clone();
    let function = syntax.children().find_map(ast::Fn::cast)?;
    let mut entry = None;
    for (at, hop) in chain.hops.iter().enumerate().skip(cut) {
        match test_call_sites(&function, test, &hop.caller.name, oracle) {
            Sites::None => continue,
            Sites::Unpolled => return None,
            Sites::Polled => {}
        }
        if !owner_call_is_driven(chain, owner, oracle.index)
            || !hops_below_poll_their_callees(chain, cut, at, oracle.index)
        {
            return None;
        }
        entry = Some(at);
    }
    entry
}

/// An owner whose call yields a future runs its body only when polled: the
/// lowest hop must drive its call into the owner. That covers an `async fn`,
/// an unreadable declaration (fail closed), and a plain `fn` that may return a
/// future: a return type naming `Future`/`Pin`, or a body holding an `async`
/// block or a closure. Only a plain `fn` that is clearly not future-shaped
/// needs nothing. A return type hidden behind a type alias is not seen.
fn owner_call_is_driven(chain: &HelperChain, owner: &FunctionSummary, index: &RustIndex) -> bool {
    if function_declaration_is_async(owner, index) == Some(false)
        && !owner_may_return_future(owner, index)
    {
        return true;
    }
    chain
        .hops
        .first()
        .is_some_and(|lowest| hop_call_sites(&lowest.caller, &owner.name, index) == Sites::Polled)
}

fn owner_may_return_future(owner: &FunctionSummary, index: &RustIndex) -> bool {
    let Some(declaration) = index
        .source_syntax
        .get(index, &owner.file)
        .and_then(|snapshot| snapshot.declaration(owner))
    else {
        return true;
    };
    declaration.ret_type().is_some_and(|ret| {
        let text = ret.syntax().text().to_string();
        text.contains("Future") || text.contains("Pin")
    }) || declaration.syntax().descendants().any(|node| {
        ast::ClosureExpr::can_cast(node.kind())
            || ast::BlockExpr::cast(node).is_some_and(|block| block.async_token().is_some())
    })
}

/// Every async hop from the first non-synchronous one up to the entry has its
/// call driven by the hop above it (the entry's own call is the test's).
fn hops_below_poll_their_callees(
    chain: &HelperChain,
    cut: usize,
    entry: usize,
    index: &RustIndex,
) -> bool {
    (cut..=entry).all(
        |at| match function_declaration_is_async(&chain.hops[at].caller, index) {
            Some(false) => true,
            Some(true) if at == entry => true,
            Some(true) => {
                hop_call_sites(
                    &chain.hops[at + 1].caller,
                    &chain.hops[at].caller.name,
                    index,
                ) == Sites::Polled
            }
            None => false,
        },
    )
}

#[derive(Debug, PartialEq, Eq)]
enum Sites {
    /// No call of the name.
    None,
    /// At least one call is held, spawned, passed on or not executed.
    Unpolled,
    /// At least one call, and every one is driven by `.await` or a verified
    /// polling helper in code the function executes.
    Polled,
}

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

/// The call's value is awaited directly, or is the only argument of a
/// verified polling helper. Parentheses are transparent; any other consumer
/// (a binding, a combinator, an argument of another call, `spawn`, `join!`)
/// is not a proof.
fn call_is_driven(node: &SyntaxNode, helper: &dyn Fn(&str) -> bool) -> bool {
    let mut current = node.clone();
    while let Some(parent) = current.parent() {
        if ast::ParenExpr::can_cast(parent.kind()) {
            current = parent;
            continue;
        }
        if let Some(awaited) = ast::AwaitExpr::cast(parent.clone()) {
            return awaited
                .expr()
                .is_some_and(|operand| *operand.syntax() == current);
        }
        let Some(arguments) = ast::ArgList::cast(parent) else {
            return false;
        };
        let Some(call) = arguments.syntax().parent().and_then(ast::CallExpr::cast) else {
            return false;
        };
        let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
            return false;
        };
        let mut args = arguments.args();
        let sole = args
            .next()
            .is_some_and(|argument| *argument.syntax() == current)
            && args.next().is_none();
        let name = callee
            .path()
            .filter(|path| path.qualifier().is_none())
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string());
        return sole && name.is_some_and(|name| helper(&name));
    }
    false
}

fn hop_call_sites(caller: &FunctionSummary, callee: &str, index: &RustIndex) -> Sites {
    let Some(function) = index
        .source_syntax
        .get(index, &caller.file)
        .and_then(|snapshot| snapshot.declaration(caller))
    else {
        return Sites::Unpolled;
    };
    // A local binding, closure parameter or nested `fn` with the callee's name
    // can be what the call resolves to; the hop-side scan matches by name, so
    // any such binding refuses (fail closed).
    let rebinds_callee = function.syntax().descendants().any(|node| {
        ast::IdentPat::cast(node.clone())
            .and_then(|pat| pat.name())
            .is_some_and(|name| name.text() == callee)
            || ast::Fn::cast(node.clone()).is_some_and(|item| {
                item.syntax() != function.syntax()
                    && item.name().is_some_and(|name| name.text() == callee)
            })
    });
    // Import identity is not preserved, so any import that may bind the
    // callee's name refuses (fail closed, by design): a `use` tree whose bound
    // name is the callee, by final segment (`use ext::inner;`, which includes
    // the callee's own `use crate::inner;`) or by `as` rename; and ANY glob
    // `use` in the hop's file, wherever the callee is defined. The one
    // exemption is a glob inside a `#[cfg(test)]` module that does not contain
    // the hop (the in-file `mod tests { use super::*; }`), which is not in
    // scope of the hop. Not seen: imports in other files, a qualified path to
    // an external item of the same name, name binders other than `use` (a
    // const, static, tuple-struct constructor, `extern crate .. as inner`,
    // macro-generated items) and a `use` inside a `macro_rules!` body.
    let imports_may_bind_callee = index
        .source_syntax
        .get(index, &caller.file)
        .and_then(|snapshot| snapshot.tree())
        .is_none_or(|tree| {
            tree.syntax()
                .descendants()
                .filter_map(ast::UseTree::cast)
                .any(|use_tree| {
                    if use_tree.star_token().is_some() {
                        return !in_unrelated_test_module(&use_tree, function.syntax());
                    }
                    use_tree.use_tree_list().is_none()
                        && match use_tree.rename() {
                            Some(rename) => rename.name().is_some_and(|name| name.text() == callee),
                            None => use_tree
                                .path()
                                .and_then(|path| path.segment())
                                .and_then(|segment| segment.name_ref())
                                .is_some_and(|name| name.text() == callee),
                        }
                })
        });
    let renamed_to_callee = imports_may_bind_callee;
    if rebinds_callee || renamed_to_callee {
        return Sites::Unpolled;
    }
    // A synchronous caller cannot `.await`, and ripr verifies no polling
    // helper from a production body.
    scan(function.syntax(), callee, &|_| false, &mut |_| Vec::new())
}

/// A glob inside a `#[cfg(test)]` module that does not contain the hop.
fn in_unrelated_test_module(glob: &ast::UseTree, hop: &SyntaxNode) -> bool {
    glob.syntax()
        .ancestors()
        .filter_map(ast::Module::cast)
        .any(|module| {
            module.attrs().any(|attr| {
                attr.syntax()
                    .text()
                    .to_string()
                    .replace(' ', "")
                    .contains("cfg(test)")
            }) && !module
                .syntax()
                .text_range()
                .contains_range(hop.text_range())
        })
}

fn test_call_sites(
    function: &ast::Fn,
    test: &TestSummary,
    callee: &str,
    oracle: &PollOracle<'_>,
) -> Sites {
    let helper = |name: &str| oracle.drives(test, name);
    scan(function.syntax(), callee, &helper, &mut |call| {
        assertion_operands(call, test, oracle)
    })
}

fn scan(
    scope: &SyntaxNode,
    callee: &str,
    helper: &dyn Fn(&str) -> bool,
    macro_operands: &mut dyn FnMut(&ast::MacroCall) -> Vec<String>,
) -> Sites {
    let mut found = false;
    for node in scope.descendants() {
        if call_name(&node).as_deref() == Some(callee) {
            found = true;
            if !node_executes_in(&node, scope) || !call_is_driven(&node, helper) {
                return Sites::Unpolled;
            }
        } else if let Some(call) = ast::MacroCall::cast(node.clone())
            && node_executes_in(&node, scope)
        {
            for operand in macro_operands(&call) {
                let text = format!("fn ripr_hop_operand() {{ let _ = {operand}; }}");
                let Some(parse) = crate::analysis::syntax::parse_clean_source_file(&text) else {
                    return Sites::Unpolled;
                };
                let syntax = parse.tree().syntax().clone();
                let Some(wrapper) = syntax.children().find_map(ast::Fn::cast) else {
                    return Sites::Unpolled;
                };
                for inner in wrapper.syntax().descendants() {
                    if call_name(&inner).as_deref() == Some(callee) {
                        found = true;
                        if !node_executes_in(&inner, wrapper.syntax())
                            || !call_is_driven(&inner, helper)
                        {
                            return Sites::Unpolled;
                        }
                    }
                }
            }
        }
    }
    if found { Sites::Polled } else { Sites::None }
}

/// The operands of a standard assertion macro whose name the test does not
/// rebind, the same boundary the executable-call reader applies.
fn assertion_operands(
    call: &ast::MacroCall,
    test: &TestSummary,
    oracle: &PollOracle<'_>,
) -> Vec<String> {
    let Some(path) = call.path() else {
        return Vec::new();
    };
    let name = path.syntax().text().to_string();
    let name = name.trim();
    let operands = match name {
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => 2,
        "assert" | "debug_assert" => 1,
        _ => return Vec::new(),
    };
    let rebound_locally = call.syntax().ancestors().any(|scope| {
        ast::StmtList::cast(scope).is_some_and(|list| {
            list.syntax().children().any(|node| {
                ast::MacroRules::cast(node.clone())
                    .and_then(|item| ast::HasName::name(&item))
                    .or_else(|| {
                        ast::MacroDef::cast(node).and_then(|item| ast::HasName::name(&item))
                    })
                    .is_some_and(|bound| bound.text() == name)
            })
        })
    });
    let offset = u32::from(call.syntax().text_range().start()) as usize;
    let line = test.start_line
        + test.body[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
    if rebound_locally
        || !oracle
            .syntax
            .targeted_macro_binding_is_unambiguous(test, line, name, oracle.index)
    {
        return Vec::new();
    }
    let Some(tree) = call.token_tree() else {
        return Vec::new();
    };
    let text = tree.syntax().text().to_string();
    let Some(arguments) = text.get(1..text.len().saturating_sub(1)) else {
        return Vec::new();
    };
    super::reveal::split_top_level_arguments(arguments)
        .map(|arguments| {
            arguments
                .into_iter()
                .take(operands)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
