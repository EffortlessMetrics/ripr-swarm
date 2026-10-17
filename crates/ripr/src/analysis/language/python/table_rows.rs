//! Table rows from unittest `subTest` loops, `for` tables, and pytest
//! parametrize cases.
//!
//! A strong exact oracle whose expected value is a row column cannot
//! discriminate a changed operand that no row varies. The more-itertools
//! `last(..., default=None)` empty path is the motivating case: every
//! `subTest` row passes `default=None` and asserts equality to that same
//! column, so `return default` -> `return None` still passes (#6651).
//!
//! Fail closed toward not-`exposed` only when static evidence sees the table
//! and can prove the operand is unvaried. An unreadable table, a non-table
//! strong test, or a row that varies the operand keeps today's verdict.

use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::boundary::{literal_value, literals_equal};
use super::parametrize::PythonParametrizeCases;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use super::source_utils::text_for_range;
use super::static_limits::is_simple_python_identifier;
use super::{PythonAssertion, PythonOwner, PythonTest, expr_full_name};
use crate::domain::{OracleStrength, OwnerKind};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Rows beyond this bound leave the table unread.
const MAX_TABLE_ROWS: usize = 64;

/// A changed operand that no readable table row varies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UnvariedTableOperand {
    pub(super) operand: String,
    pub(super) reason: String,
}

/// When every strong related table-driven oracle takes its expected value from
/// a row column and no row varies the changed returned operand, name that
/// operand. None when a non-table strong test exists, a table varies the
/// operand, or the table cannot be read.
pub(super) fn python_unvaried_table_operand(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<UnvariedTableOperand> {
    let operand = changed_returned_parameter(old_line_text, line_text, owner)?;
    let mut saw_table = false;
    for candidate in related_candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        let Some(assertion) =
            select_relevant_assertion(&candidate.test.assertions, Some(focus)).assertion()
        else {
            continue;
        };
        if assertion.oracle_strength.rank() < OracleStrength::Strong.rank() {
            continue;
        }
        let Some(table) = table_cases(candidate.test) else {
            // A strong oracle that is not table-driven may still discriminate.
            return None;
        };
        if !table_does_not_vary(candidate.test, owner, &operand, &table, assertion) {
            return None;
        }
        saw_table = true;
    }
    saw_table.then(|| UnvariedTableOperand {
        operand: operand.clone(),
        reason: format!("table rows do not vary {operand}"),
    })
}

fn changed_returned_parameter(
    old_line_text: Option<&str>,
    new_line_text: &str,
    owner: &PythonOwner,
) -> Option<String> {
    // `return default` -> `return None` names the operand on the old line.
    let old = old_line_text.and_then(|old| returned_parameter(old, owner));
    let new = returned_parameter(new_line_text, owner);
    match (old, new) {
        (Some(old), Some(new)) if old == new => None,
        (Some(old), _) => Some(old),
        (None, Some(new)) => Some(new),
        (None, None) => None,
    }
}

fn returned_parameter(line_text: &str, owner: &PythonOwner) -> Option<String> {
    let expr = line_text.trim().strip_prefix("return ")?.trim();
    let expr = expr.split('#').next()?.trim();
    if !is_simple_python_identifier(expr) {
        return None;
    }
    owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == expr)
        .then(|| expr.to_string())
}

fn table_cases(test: &PythonTest) -> Option<PythonParametrizeCases> {
    if let Some(parametrize) = &test.parametrize
        && !parametrize.cases.is_empty()
    {
        return Some(parametrize.clone());
    }
    loop_table_cases(&test.body_text)
}

fn table_does_not_vary(
    test: &PythonTest,
    owner: &PythonOwner,
    operand: &str,
    table: &PythonParametrizeCases,
    assertion: &PythonAssertion,
) -> bool {
    let columns = table.argnames();
    if columns.is_empty() {
        return false;
    }
    let names = owner_names(owner, test);
    let Some(stmts) = assertion_stmts(&assertion.gate_text) else {
        return false;
    };
    let Some((owner_call, expected)) = owner_call_and_expected(&stmts, &names) else {
        return false;
    };
    let expected_from_table = expr_plain_name(expected).is_some_and(|name| columns.contains(&name));
    if !expected_from_table {
        return false;
    }
    let Some(column) = operand_column(owner_call, operand, owner, &columns) else {
        return false;
    };
    unvaried_column_value(&table.cases, &column).is_some()
}

fn owner_names(owner: &PythonOwner, test: &PythonTest) -> Vec<String> {
    let mut names = vec![owner.name.clone()];
    names.extend(
        test.imports
            .iter()
            .filter(|import| import.imported == owner.name && import.alias != owner.name)
            .map(|import| import.alias.clone()),
    );
    names
}

fn assertion_stmts(gate_text: &str) -> Option<Vec<Stmt>> {
    let body = gate_text
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!("def _():\n{body}\n");
    let module = parse_module_result(Path::new("t.py"), &source).ok()?;
    match module {
        Mod::Module(module) => match module.body.into_iter().next()? {
            Stmt::FunctionDef(function) => Some(function.body),
            _ => None,
        },
        _ => None,
    }
}

fn owner_call_and_expected<'a>(
    stmts: &'a [Stmt],
    owner_names: &[String],
) -> Option<(&'a Expr, &'a Expr)> {
    stmts
        .iter()
        .find_map(|stmt| owner_call_and_expected_from_stmt(stmt, owner_names))
}

fn owner_call_and_expected_from_stmt<'a>(
    stmt: &'a Stmt,
    owner_names: &[String],
) -> Option<(&'a Expr, &'a Expr)> {
    match stmt {
        Stmt::Assert(assert_stmt) => match assert_stmt.test.as_ref() {
            Expr::Compare(compare)
                if compare.ops.len() == 1
                    && matches!(compare.ops[0], ast::CmpOp::Eq | ast::CmpOp::Is)
                    && compare.comparators.len() == 1 =>
            {
                sides(compare.left.as_ref(), &compare.comparators[0], owner_names)
            }
            _ => None,
        },
        Stmt::Expr(expr_stmt) => match expr_stmt.value.as_ref() {
            Expr::Call(call) if is_equality_assertion(call) => {
                let (first, second) = compared_operands(call)?;
                sides(first, second, owner_names)
            }
            _ => None,
        },
        _ => None,
    }
}

fn sides<'a>(
    left: &'a Expr,
    right: &'a Expr,
    owner_names: &[String],
) -> Option<(&'a Expr, &'a Expr)> {
    match (
        is_owner_call(left, owner_names),
        is_owner_call(right, owner_names),
    ) {
        (true, false) => Some((left, right)),
        (false, true) => Some((right, left)),
        _ => None,
    }
}

fn is_owner_call(expr: &Expr, owner_names: &[String]) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    expr_full_name(call.func.as_ref()).is_some_and(|name| {
        owner_names
            .iter()
            .any(|owner| name == *owner || name.ends_with(&format!(".{owner}")))
    })
}

fn is_equality_assertion(call: &ast::ExprCall) -> bool {
    expr_full_name(&call.func).is_some_and(|name| {
        matches!(
            name.rsplit('.').next().unwrap_or(name.as_str()),
            "assertEqual" | "assertIs"
        )
    })
}

fn compared_operands(call: &ast::ExprCall) -> Option<(&Expr, &Expr)> {
    let keyword = |arg: &str| {
        call.keywords
            .iter()
            .find(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|name| name.as_str() == arg)
            })
            .map(|keyword| &keyword.value)
    };
    let mut positional = call.args.iter();
    let first = positional.next().or_else(|| keyword("first"))?;
    let second = positional.next().or_else(|| keyword("second"))?;
    Some((first, second))
}

fn expr_plain_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        _ => None,
    }
}

fn operand_column(
    owner_call: &Expr,
    operand: &str,
    owner: &PythonOwner,
    columns: &BTreeSet<String>,
) -> Option<String> {
    let Expr::Call(call) = owner_call else {
        return None;
    };
    if call
        .args
        .iter()
        .any(|argument| matches!(argument, Expr::Starred(_)))
        || call.keywords.iter().any(|keyword| keyword.arg.is_none())
    {
        return None;
    }
    let skip = receiver_skip(owner)?;
    for keyword in &call.keywords {
        if keyword
            .arg
            .as_ref()
            .is_some_and(|name| name.as_str() == operand)
            && let Some(name) = expr_plain_name(&keyword.value)
            && columns.contains(&name)
        {
            return Some(name);
        }
    }
    let positional: Vec<&str> = owner
        .parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .skip(skip)
        .map(|parameter| parameter.name.as_str())
        .collect();
    for (parameter, argument) in positional.iter().zip(call.args.iter()) {
        if *parameter == operand
            && let Some(name) = expr_plain_name(argument)
            && columns.contains(&name)
        {
            return Some(name);
        }
    }
    None
}

fn receiver_skip(owner: &PythonOwner) -> Option<usize> {
    match owner.owner_kind {
        Some(OwnerKind::Method) => Some(1),
        Some(OwnerKind::ClassMethod) => Some(usize::from(
            !owner
                .decorators
                .iter()
                .any(|decorator| decorator.ends_with("staticmethod")),
        )),
        Some(OwnerKind::Function) => Some(0),
        _ => None,
    }
}

fn unvaried_column_value(cases: &[BTreeMap<String, String>], column: &str) -> Option<String> {
    if cases.is_empty() {
        return None;
    }
    let mut values = Vec::with_capacity(cases.len());
    for case in cases {
        let raw = case.get(column)?;
        values.push(literal_value(raw)?);
    }
    let first = values.first()?.clone();
    values
        .iter()
        .all(|value| literals_equal(value, &first))
        .then_some(first)
}

fn loop_table_cases(source: &str) -> Option<PythonParametrizeCases> {
    let module = parse_module_result(Path::new("t.py"), source).ok()?;
    let stmts = match module {
        Mod::Module(module) => module.body,
        _ => return None,
    };
    let mut tables = Vec::new();
    match stmts.as_slice() {
        [Stmt::FunctionDef(function)] => walk_tables(&function.body, source, &mut tables),
        [Stmt::AsyncFunctionDef(function)] => walk_tables(&function.body, source, &mut tables),
        _ => walk_tables(&stmts, source, &mut tables),
    }
    (tables.len() == 1).then(|| tables.remove(0))
}

fn walk_tables(stmts: &[Stmt], source: &str, out: &mut Vec<PythonParametrizeCases>) {
    for stmt in stmts {
        match stmt {
            Stmt::For(for_stmt) => {
                if let Some(table) = table_from_for(&for_stmt.target, &for_stmt.iter, source) {
                    out.push(table);
                }
                walk_tables(&for_stmt.body, source, out);
                walk_tables(&for_stmt.orelse, source, out);
            }
            Stmt::AsyncFor(for_stmt) => {
                if let Some(table) = table_from_for(&for_stmt.target, &for_stmt.iter, source) {
                    out.push(table);
                }
                walk_tables(&for_stmt.body, source, out);
                walk_tables(&for_stmt.orelse, source, out);
            }
            Stmt::With(with_stmt) => walk_tables(&with_stmt.body, source, out),
            Stmt::AsyncWith(with_stmt) => walk_tables(&with_stmt.body, source, out),
            Stmt::If(if_stmt) => {
                walk_tables(&if_stmt.body, source, out);
                walk_tables(&if_stmt.orelse, source, out);
            }
            Stmt::While(while_stmt) => {
                walk_tables(&while_stmt.body, source, out);
                walk_tables(&while_stmt.orelse, source, out);
            }
            Stmt::Try(try_stmt) => {
                walk_tables(&try_stmt.body, source, out);
                walk_tables(&try_stmt.orelse, source, out);
                walk_tables(&try_stmt.finalbody, source, out);
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    walk_tables(&handler.body, source, out);
                }
            }
            Stmt::TryStar(try_stmt) => {
                walk_tables(&try_stmt.body, source, out);
                walk_tables(&try_stmt.orelse, source, out);
                walk_tables(&try_stmt.finalbody, source, out);
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    walk_tables(&handler.body, source, out);
                }
            }
            Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    walk_tables(&case.body, source, out);
                }
            }
            _ => {}
        }
    }
}

fn table_from_for(target: &Expr, iter: &Expr, source: &str) -> Option<PythonParametrizeCases> {
    let names = for_target_names(target)?;
    if names.is_empty() || names.len() > 8 {
        return None;
    }
    let rows = match iter {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    if rows.is_empty() || rows.len() > MAX_TABLE_ROWS {
        return None;
    }
    let bare_single = names.len() == 1;
    let mut cases = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row_cells(row, names.len(), bare_single)?;
        let mut case = BTreeMap::new();
        for (name, value) in names.iter().zip(values) {
            let text = text_for_range(source, value.range()).trim().to_string();
            if text.is_empty() {
                return None;
            }
            case.insert(name.clone(), text);
        }
        cases.push(case);
    }
    Some(PythonParametrizeCases { cases })
}

fn for_target_names(target: &Expr) -> Option<Vec<String>> {
    match target {
        Expr::Name(name) => Some(vec![name.id.to_string()]),
        Expr::Tuple(tuple) => names_from_elements(&tuple.elts),
        Expr::List(list) => names_from_elements(&list.elts),
        _ => None,
    }
}

fn names_from_elements(elements: &[Expr]) -> Option<Vec<String>> {
    elements
        .iter()
        .map(|element| match element {
            Expr::Name(name) => Some(name.id.to_string()),
            _ => None,
        })
        .collect()
}

fn row_cells(row: &Expr, arity: usize, bare_single: bool) -> Option<Vec<&Expr>> {
    if bare_single {
        return (!matches!(row, Expr::Starred(_))).then(|| vec![row]);
    }
    let elements = match row {
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::List(list) => &list.elts,
        _ => return None,
    };
    if elements
        .iter()
        .any(|element| matches!(element, Expr::Starred(_)))
    {
        return None;
    }
    (elements.len() == arity).then(|| elements.iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(source: &str) -> Option<Vec<BTreeMap<String, String>>> {
        loop_table_cases(source).map(|table| table.cases)
    }

    #[test]
    fn subtest_table_records_unvaried_default_column() {
        let source = "def test_default(self):\n    for iterable, default in [\n        ([], None),\n        (iter([]), None),\n    ]:\n        with self.subTest(iterable=iterable, default=default):\n            self.assertEqual(last(iterable, default=default), default)\n";
        let table = cases(source).expect("subTest table");
        assert_eq!(table.len(), 2);
        assert_eq!(table[0].get("default").map(String::as_str), Some("None"));
        assert_eq!(table[1].get("default").map(String::as_str), Some("None"));
        assert_eq!(
            unvaried_column_value(&table, "default").as_deref(),
            Some("None")
        );
        assert!(unvaried_column_value(&table, "iterable").is_none());
    }

    #[test]
    fn a_row_that_varies_default_is_not_unvaried() {
        let source = "def test_default(self):\n    for iterable, default in [([], None), (iter([]), 0)]:\n        with self.subTest(iterable=iterable):\n            self.assertEqual(last(iterable, default=default), default)\n";
        let table = cases(source).expect("varied table");
        assert!(unvaried_column_value(&table, "default").is_none());
    }

    #[test]
    fn a_loop_over_a_variable_records_nothing() {
        let source = "def test_default(self):\n    for iterable, default in rows:\n        self.assertEqual(last(iterable, default), default)\n";
        assert!(cases(source).is_none());
    }
}
