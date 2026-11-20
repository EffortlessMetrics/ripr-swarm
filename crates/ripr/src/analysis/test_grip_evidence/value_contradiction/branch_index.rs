//! The argument-independent half of [`super::seam_branch_reached_by_call`]
//! (#11738).
//!
//! Which `if` branch holds a seam's text on its line, the enclosing branches
//! on the path to it, and whether the owner rebinds a parameter depend only
//! on the owner's text, so they are read once per owner text in a run scope.
//! Before, each (seam, oracle) pair parsed the owner and walked every `if` of
//! it, so one owner with many changed branches paid its length squared.

use super::{fold_condition, normalized_expression, parameter_is_rebound_or_mutated};
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::syntax::run_memo::{self, Product};
use ra_ap_syntax::{AstNode, ast, ast::HasName};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// The owner's branch structure, or `None` when the owner cannot be read
/// (no clean parse, no function or body, a parameter that is not a plain
/// name, an `if` without a then block) or rebinds or mutates a parameter.
/// Each of those made the per-pair answer `None` whatever the call.
pub(super) fn owner_branch_index(body: &str) -> Rc<Option<BranchIndex>> {
    run_memo::memoize(Product::OwnerBranchIndex, 0, body, || {
        Rc::new(BranchIndex::read(body))
    })
}

pub(super) struct BranchIndex {
    names: Vec<String>,
    /// Texts some `if` holds in both its then block and its `else` block.
    both_arms: HashSet<String>,
    /// For a (text, body line), each `if` whose then or `else` block holds
    /// that text starting on that line, with whether the then block does.
    at_line: HashMap<(String, usize), Vec<(usize, bool)>>,
    ifs: Vec<IndexedIf>,
}

struct IndexedIf {
    condition: Option<String>,
    /// Every `if` above this one, innermost first.
    enclosing: Vec<Enclosing>,
}

struct Enclosing {
    in_then: bool,
    in_else: bool,
    condition: Option<String>,
}

impl BranchIndex {
    fn read(body: &str) -> Option<Self> {
        #[cfg(test)]
        BRANCH_INDEX_READS.with(|reads| reads.set(reads.get() + 1));
        let parse = parse_clean_source_file(body)?;
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?;
        let block = function.body()?;
        let mut names = Vec::new();
        for param in function.param_list()?.params() {
            let ast::Pat::IdentPat(ident) = param.pat()? else {
                return None;
            };
            names.push(ident.name()?.text().to_string());
        }
        if parameter_is_rebound_or_mutated(&block, &names) {
            return None;
        }
        let newlines: Vec<usize> = body
            .bytes()
            .enumerate()
            .filter_map(|(at, byte)| (byte == b'\n').then_some(at))
            .collect();
        let line_of = |node: &ra_ap_syntax::SyntaxNode| {
            let offset = usize::from(node.text_range().start());
            newlines.partition_point(|newline| *newline < offset) + 1
        };
        let mut both_arms = HashSet::new();
        let mut at_line: HashMap<(String, usize), Vec<(usize, bool)>> = HashMap::new();
        let mut ifs = Vec::new();
        for if_expr in block.syntax().descendants().filter_map(ast::IfExpr::cast) {
            let then_block = if_expr.then_branch()?;
            let else_block = match if_expr.else_branch() {
                Some(ast::ElseBranch::Block(else_block)) => Some(else_block),
                _ => None,
            };
            let then_entries = block_entries(&then_block, &line_of);
            let else_entries = else_block
                .as_ref()
                .map(|else_block| block_entries(else_block, &line_of))
                .unwrap_or_default();
            let then_texts: HashSet<&str> =
                then_entries.iter().map(|(text, _)| text.as_str()).collect();
            for (text, _) in &else_entries {
                if then_texts.contains(text.as_str()) {
                    both_arms.insert(text.clone());
                }
            }
            let then_keys: HashSet<&(String, usize)> = then_entries.iter().collect();
            let mut keys: Vec<&(String, usize)> = then_entries.iter().collect();
            keys.extend(else_entries.iter());
            let mut seen = HashSet::new();
            let position = ifs.len();
            for key in keys {
                if seen.insert(key) {
                    at_line
                        .entry(key.clone())
                        .or_default()
                        .push((position, then_keys.contains(key)));
                }
            }
            ifs.push(IndexedIf {
                condition: if_expr
                    .condition()
                    .map(|condition| condition.syntax().text().to_string()),
                enclosing: enclosing_ifs(&if_expr),
            });
        }
        Some(Self {
            names,
            both_arms,
            at_line,
            ifs,
        })
    }

    /// The per-call half: `values` bound to the parameters, `target` the
    /// seam's normalized text and `line` its body-relative line.
    pub(super) fn reached(
        &self,
        values: Vec<i128>,
        seam_expression: &str,
        line: usize,
    ) -> Option<bool> {
        if self.names.len() != values.len() {
            return None;
        }
        let environment: BTreeMap<String, i128> = self.names.iter().cloned().zip(values).collect();
        let target = normalized_expression(seam_expression);
        if self.both_arms.contains(&target) {
            return None;
        }
        let matches = self
            .at_line
            .get(&(target, line))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let [(position, in_then)] = matches else {
            return None;
        };
        let indexed = self.ifs.get(*position)?;
        let condition = fold_condition(indexed.condition.as_ref()?, &environment)?;
        let reached = if *in_then { condition } else { !condition };
        if !reached {
            return Some(false);
        }
        for enclosing in &indexed.enclosing {
            if !enclosing.in_then && !enclosing.in_else {
                return None;
            }
            let condition = fold_condition(enclosing.condition.as_ref()?, &environment)?;
            let reached = if enclosing.in_then {
                condition
            } else {
                !condition
            };
            if !reached {
                return Some(false);
            }
        }
        Some(true)
    }
}

/// The normalized text and start line of `block`'s tail expression and of
/// each of its statements.
fn block_entries(
    block: &ast::BlockExpr,
    line_of: &impl Fn(&ra_ap_syntax::SyntaxNode) -> usize,
) -> Vec<(String, usize)> {
    let mut entries = Vec::new();
    if let Some(tail) = block.tail_expr() {
        entries.push((
            normalized_expression(&tail.syntax().text().to_string()),
            line_of(tail.syntax()),
        ));
    }
    for statement in block.statements() {
        entries.push((
            normalized_expression(&statement.syntax().text().to_string()),
            line_of(statement.syntax()),
        ));
    }
    entries
}

fn enclosing_ifs(if_expr: &ast::IfExpr) -> Vec<Enclosing> {
    let range = if_expr.syntax().text_range();
    let mut enclosing = Vec::new();
    let mut current = if_expr.syntax().parent();
    while let Some(node) = current {
        if let Some(ancestor) = ast::IfExpr::cast(node.clone()) {
            let in_then = ancestor
                .then_branch()
                .is_some_and(|block| block.syntax().text_range().contains_range(range));
            let in_else = match ancestor.else_branch() {
                Some(ast::ElseBranch::Block(block)) => {
                    block.syntax().text_range().contains_range(range)
                }
                Some(ast::ElseBranch::IfExpr(inner)) => {
                    inner.syntax().text_range().contains_range(range)
                }
                None => false,
            };
            enclosing.push(Enclosing {
                in_then,
                in_else,
                condition: ancestor
                    .condition()
                    .map(|condition| condition.syntax().text().to_string()),
            });
        }
        current = node.parent();
    }
    enclosing
}

#[cfg(test)]
thread_local! {
    /// How many owner texts this thread has indexed.
    pub(super) static BRANCH_INDEX_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
