//! Per-row evaluation of a constant-row table loop's skip guard (#9409,
//! RIPR-SPEC-0240).
//!
//! A harness that lists known-bad rows writes
//! `if SKIP.contains(&scheme) { continue; }` before its assertion. The
//! guard is a bypass only for the rows it names: with the `const` resolved
//! to literals and the binding read from each row's literal cell, ripr can
//! tell which rows the assertion still runs on. Anything the reader cannot
//! establish leaves the guard unrecognized, and the caller keeps refusing
//! the `continue`.

use super::{macro_may_rebind, row_cell};
use crate::analysis::facts::LiteralSliceConst;
use ra_ap_syntax::{
    AstNode, TextRange,
    ast::{self, HasArgList, HasGenericArgs, HasLoopBody, HasName},
};

/// The rows a table loop's leading skip guards remove.
pub(super) struct RowSkips {
    /// One flag per table row: the guards `continue` past it.
    pub skipped: Vec<bool>,
    /// The guard statements, so a scan for `break` or `continue` can set
    /// them aside.
    pub guards: Vec<TextRange>,
}

impl RowSkips {
    pub(super) fn covers(&self, range: TextRange) -> bool {
        self.guards.iter().any(|guard| guard.contains_range(range))
    }
}

/// The module-level `const NAME: &[T] = &[literal, ..]` items beside
/// `function`, with every item a plain literal. A name declared twice is
/// dropped.
pub(crate) fn module_literal_slice_consts(function: &ast::Fn) -> Vec<LiteralSliceConst> {
    let Some(parent) = function.syntax().parent() else {
        return Vec::new();
    };
    let mut consts: Vec<LiteralSliceConst> = Vec::new();
    let mut declared: Vec<String> = Vec::new();
    for item in parent.children().filter_map(ast::Const::cast) {
        let Some(name) = item.name().map(|name| name.text().to_string()) else {
            continue;
        };
        declared.push(name.clone());
        if item
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
        {
            continue;
        }
        let Some(items) = literal_slice_items(&item) else {
            continue;
        };
        consts.push(LiteralSliceConst { name, items });
    }
    consts.retain(|item| declared.iter().filter(|name| **name == item.name).count() == 1);
    consts
}

fn literal_slice_items(item: &ast::Const) -> Option<Vec<String>> {
    let Some(ast::Type::RefType(reference)) = item.ty() else {
        return None;
    };
    if reference.mut_token().is_some()
        || !matches!(
            reference.ty(),
            Some(ast::Type::SliceType(_) | ast::Type::ArrayType(_))
        )
    {
        return None;
    }
    let mut body = item.body()?;
    if let ast::Expr::RefExpr(reference) = &body {
        if reference.mut_token().is_some() || reference.raw_token().is_some() {
            return None;
        }
        body = reference.expr()?;
    }
    let ast::Expr::ArrayExpr(array) = body else {
        return None;
    };
    if array.semicolon_token().is_some() {
        return None;
    }
    array
        .exprs()
        .map(|expr| match expr {
            ast::Expr::Literal(literal) => Some(literal.syntax().text().to_string()),
            _ => None,
        })
        .collect()
}

/// The skip guards at the top of `table`'s loop body and the rows they
/// remove. `None` unless at least one guard is recognized, every guard is
/// decided for every row, and some row survives; the caller then treats
/// any `continue` as it did before.
pub(super) fn row_skips(
    table: &ast::ForExpr,
    function: &ast::Fn,
    rows: &ast::ArrayExpr,
    consts: &[LiteralSliceConst],
) -> Option<RowSkips> {
    let body = table.loop_body()?;
    let statements = body.stmt_list()?;
    let mut skipped = vec![false; rows.exprs().count()];
    let mut guards = Vec::new();
    for statement in statements.statements() {
        let Some(guard) = skip_guard(&statement) else {
            break;
        };
        let mask = guard_mask(&guard, table, function, rows, consts)?;
        for (row, hit) in skipped.iter_mut().zip(mask) {
            *row |= hit;
        }
        guards.push(statement.syntax().text_range());
    }
    (!guards.is_empty() && skipped.iter().any(|skip| !skip)).then_some(RowSkips { skipped, guards })
}

/// What a recognized guard asks of a row's cell.
enum Guard {
    /// `LIST.contains(&binding)`
    Listed { list: String, binding: String },
    /// `binding == literal`
    Equals { binding: String, literal: String },
}

/// `if <guard> { continue; }` with no `else`, as a statement.
fn skip_guard(statement: &ast::Stmt) -> Option<Guard> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    let ast::Expr::IfExpr(branch) = statement.expr()? else {
        return None;
    };
    if branch.else_branch().is_some()
        || branch
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let then = branch.then_branch()?.stmt_list()?;
    let only = match (
        then.statements().collect::<Vec<_>>().as_slice(),
        then.tail_expr(),
    ) {
        ([ast::Stmt::ExprStmt(single)], None) => single.expr()?,
        ([], Some(tail)) => tail,
        _ => return None,
    };
    let ast::Expr::ContinueExpr(skip) = only else {
        return None;
    };
    if skip.lifetime().is_some() {
        return None;
    }
    match branch.condition()? {
        ast::Expr::MethodCallExpr(call) => {
            let name = call.name_ref()?;
            let args: Vec<_> = call.arg_list()?.args().collect();
            let [ast::Expr::RefExpr(argument)] = args.as_slice() else {
                return None;
            };
            if name.text() != "contains"
                || call.generic_arg_list().is_some()
                || argument.mut_token().is_some()
                || argument.raw_token().is_some()
            {
                return None;
            }
            Some(Guard::Listed {
                list: plain_name(&call.receiver()?)?,
                binding: plain_name(&argument.expr()?)?,
            })
        }
        ast::Expr::BinExpr(comparison)
            if comparison.op_kind()
                == Some(ast::BinaryOp::CmpOp(ast::CmpOp::Eq { negated: false })) =>
        {
            let (lhs, rhs) = (comparison.lhs()?, comparison.rhs()?);
            match (&lhs, &rhs) {
                (ast::Expr::PathExpr(_), ast::Expr::Literal(literal))
                | (ast::Expr::Literal(literal), ast::Expr::PathExpr(_)) => {
                    let path = if matches!(lhs, ast::Expr::PathExpr(_)) {
                        &lhs
                    } else {
                        &rhs
                    };
                    Some(Guard::Equals {
                        binding: plain_name(path)?,
                        literal: literal.syntax().text().to_string(),
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The single-segment, unqualified, non-generic name `expr` spells.
fn plain_name(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expr else {
        return None;
    };
    let path = path.path().filter(|path| path.qualifier().is_none())?;
    let segment = path.segment()?;
    if segment.generic_arg_list().is_some() {
        return None;
    }
    Some(segment.name_ref()?.text().to_string())
}

/// Which rows `guard` skips: `None` when any row's outcome is not decided.
fn guard_mask(
    guard: &Guard,
    table: &ast::ForExpr,
    function: &ast::Fn,
    rows: &ast::ArrayExpr,
    consts: &[LiteralSliceConst],
) -> Option<Vec<bool>> {
    let (binding, listed) = match guard {
        Guard::Listed { list, binding } => {
            // A second spelling of the list's name could be a local that
            // shadows the module `const`.
            if tokens_named(function, list) != 1 {
                return None;
            }
            let mut matching = consts.iter().filter(|item| item.name == *list);
            let item = matching.next()?;
            if matching.next().is_some() {
                return None;
            }
            let keys: Option<Vec<String>> = item.items.iter().map(|item| plain_key(item)).collect();
            (binding, keys?)
        }
        Guard::Equals { binding, literal } => (binding, vec![plain_key(literal)?]),
    };
    // The binding must be this loop's own pattern variable, bound nowhere
    // else in the test.
    let position = pattern_position(table, binding)?;
    if function
        .syntax()
        .descendants()
        .filter_map(ast::Name::cast)
        .filter(|name| name.text() == binding.as_str())
        .count()
        != 1
        || macro_may_rebind(function, binding)
    {
        return None;
    }
    rows.exprs()
        .map(|row| {
            let key = plain_key(row_cell(row, position)?.trim())?;
            Some(listed.contains(&key))
        })
        .collect()
}

/// `Some(None)` when `name` is the whole `for` pattern, `Some(Some((i, n)))`
/// when it is field `i` of a flat `n`-field tuple pattern.
fn pattern_position(table: &ast::ForExpr, name: &str) -> Option<Option<(usize, usize)>> {
    match table.pat()? {
        ast::Pat::IdentPat(pattern) => (plain_ident(&pattern, name)).then_some(None),
        ast::Pat::TuplePat(tuple) => {
            let fields: Vec<_> = tuple.fields().collect();
            if fields
                .iter()
                .any(|field| !matches!(field, ast::Pat::IdentPat(_) | ast::Pat::WildcardPat(_)))
            {
                return None;
            }
            let index = fields.iter().position(
                |field| matches!(field, ast::Pat::IdentPat(pattern) if plain_ident(pattern, name)),
            )?;
            Some(Some((index, fields.len())))
        }
        _ => None,
    }
}

fn plain_ident(pattern: &ast::IdentPat, name: &str) -> bool {
    pattern.mut_token().is_none()
        && pattern.ref_token().is_none()
        && pattern.at_token().is_none()
        && pattern.name().is_some_and(|bound| bound.text() == name)
}

fn tokens_named(function: &ast::Fn, name: &str) -> usize {
    function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text().trim_start_matches("r#") == name)
        .count()
}

/// A literal two spellings of which cannot differ in value: a string
/// without escapes or a raw prefix, or a decimal integer without suffix,
/// separator or leading zero. Text equality then decides value equality.
fn plain_key(text: &str) -> Option<String> {
    let text = text.trim();
    let string = text.len() >= 2
        && text.starts_with('"')
        && text.ends_with('"')
        && !text[1..text.len() - 1].contains(['"', '\\']);
    let integer = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    (string || integer).then(|| text.to_string())
}

#[cfg(test)]
mod tests;
