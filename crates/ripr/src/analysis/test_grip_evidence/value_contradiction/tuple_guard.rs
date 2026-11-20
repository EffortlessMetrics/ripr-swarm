//! Inputs for the shared branch index when a guard compares the first field
//! of a newtype (#15770). Only a completed comparison may withhold credit.

use super::{
    branch_index, equality_assertion_arguments, fold_condition, integer_literal_in_type,
    owner_call_arguments, whole_integer_literal,
};
use crate::analysis::classify::{callee_is_unique, macro_names_rebound, single_tuple_field_type};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file_memoized;
use ra_ap_syntax::{AstNode, ast, ast::HasAttrs, ast::HasName};
use std::collections::BTreeMap;

/// All calls in the test must be read. The operator form relies on the caller
/// having established direct dispatch to the changed trait method.
pub(in crate::analysis) fn tuple_branch_reached(
    owner: &FunctionSummary,
    expression: &str,
    line: usize,
    test: &TestSummary,
    index: &RustIndex,
    direct_operator: bool,
) -> Option<bool> {
    if owner.file != test.file || !owner.body.contains(".0") || !owner.body.contains("if") {
        return None;
    }
    let parse = parse_clean_source_file_memoized(owner.body.as_str())?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?;
    let ast::Expr::IfExpr(branch) = body.tail_expr()? else {
        return None;
    };
    if body.statements().next().is_some()
        || branch
            .syntax()
            .descendants()
            .filter_map(ast::IfExpr::cast)
            .count()
            != 1
    {
        return None;
    }
    let condition = branch.condition()?;
    let ast::Expr::BinExpr(comparison) = &condition else {
        return None;
    };
    if !matches!(comparison.op_kind(), Some(ast::BinaryOp::CmpOp(_))) {
        return None;
    }
    let ast::Expr::FieldExpr(field) = comparison.lhs()? else {
        return None;
    };
    if field.name_ref()?.text() != "0" {
        return None;
    }

    let branch_index = branch_index::owner_branch_index(&owner.body);
    let branch_index = branch_index.as_ref().as_ref()?;
    let relative = line.checked_sub(owner.start_line)? + 1;
    let in_then =
        branch_index.outer_arm(expression, relative, &condition.syntax().text().to_string())?;
    let signature = signature(owner, &function, index, direct_operator)?;
    let parameter = field.expr()?.syntax().text().to_string();
    let at = signature.iter().position(|(name, _)| name == &parameter)?;
    let constructor = &signature.get(at)?.1;
    let field_type = single_tuple_field_type(index, constructor)?;
    if !constructor_is_clear(index, owner, constructor, &field_type)
        || macro_names_rebound(index, &["assert_eq"])
    {
        return None;
    }
    let read_field = |text: &str| {
        let args = owner_call_arguments(text, constructor)?;
        let [literal] = args.as_slice() else {
            return None;
        };
        integer_literal_in_type(literal, &field_type)
    };
    // Another call in the sibling arm could re-enter the owner with a
    // different value. A plain literal or a construction of one cannot.
    let sibling = if in_then {
        let ast::ElseBranch::Block(block) = branch.else_branch()? else {
            return None;
        };
        block
    } else {
        branch.then_branch()?
    };
    let tail = sibling.tail_expr()?.syntax().text().to_string();
    if sibling.statements().next().is_some()
        || read_field(&tail)
            .or_else(|| whole_integer_literal(&tail))
            .is_none()
    {
        return None;
    }
    let bound =
        integer_literal_in_type(&comparison.rhs()?.syntax().text().to_string(), &field_type)?;
    let op = comparison.op_token()?;
    let inputs = test_inputs(test, owner, &signature, at, constructor, read_field)?;
    let mut reaches = None;
    for value in inputs {
        let selected = fold_condition(&format!("{value} {op} {bound}"), &BTreeMap::new())?;
        reaches = Some(reaches.unwrap_or(false) || selected == in_then);
    }
    reaches
}

fn signature(
    owner: &FunctionSummary,
    function: &ast::Fn,
    index: &RustIndex,
    direct_operator: bool,
) -> Option<Vec<(String, String)>> {
    let params = function.param_list()?;
    let mut signature = Vec::new();
    match &owner.item.container {
        FunctionContainer::Free if callee_is_unique(&owner.name, index) => {}
        FunctionContainer::TraitImpl {
            trait_path,
            self_ty,
        } if direct_operator
            && matches!(
                trait_path.as_str(),
                "Add" | "std::ops::Add" | "core::ops::Add"
            )
            && owner.name == "add"
            && params.self_param()?.syntax().text() == "self" =>
        {
            signature.push(("self".to_owned(), self_ty.clone()));
        }
        _ => return None,
    }
    for param in params.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        let name = ident.name()?.text().to_string();
        if ident.syntax().text().to_string() != name {
            return None;
        }
        signature.push((name, param.ty()?.syntax().text().to_string()));
    }
    Some(signature)
}

fn constructor_is_clear(index: &RustIndex, owner: &FunctionSummary, name: &str, ty: &str) -> bool {
    let Some(source) = index.files().get(&owner.file) else {
        return false;
    };
    let Some(parse) = parse_clean_source_file_memoized(&source.source) else {
        return false;
    };
    if source.source.contains(&format!("r#{name}")) || source.source.contains(&format!("r#{ty}")) {
        return false;
    }
    let file = parse.tree();
    let local = file
        .syntax()
        .children()
        .filter_map(ast::Struct::cast)
        .any(|item| {
            item.name().is_some_and(|n| n.text() == name)
                && item
                    .attrs()
                    .all(|a| a.simple_name().as_deref() == Some("derive"))
        });
    // An item macro or import may supply an unindexed constructor. A type
    // alias may also give the primitive field a different runtime width.
    local
        && !file.syntax().descendants().any(|node| {
            if let Some(import) = ast::Use::cast(node.clone()) {
                let Some(tree) = import.use_tree() else {
                    return true;
                };
                let path = tree.syntax().text().to_string();
                if path.contains('*') {
                    return !matches!(path.as_str(), "super::*" | "std::ops::*" | "core::ops::*");
                }
                return path
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|word| word == name || word == ty || word == owner.name);
            }
            ast::MacroRules::can_cast(node.kind())
                || ast::MacroDef::can_cast(node.kind())
                || ast::MacroCall::cast(node.clone()).is_some_and(|m| {
                    m.syntax().parent().is_some_and(|p| {
                        ast::SourceFile::can_cast(p.kind()) || ast::ItemList::can_cast(p.kind())
                    })
                })
                || ast::TypeAlias::cast(node)
                    .and_then(|alias| alias.name())
                    .is_some_and(|n| n.text() == ty)
        })
        && !index.functions().iter().any(|f| {
            matches!(&f.item.container, FunctionContainer::TraitImpl { self_ty, .. }
            if self_ty == name && matches!(f.name.as_str(), "drop" | "eq"))
        })
}

fn test_inputs(
    test: &TestSummary,
    owner: &FunctionSummary,
    signature: &[(String, String)],
    position: usize,
    constructor: &str,
    read_field: impl Fn(&str) -> Option<i128>,
) -> Option<Vec<i128>> {
    let parse = parse_clean_source_file_memoized(test.body.as_str())?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?;
    if function.param_list()?.params().next().is_some() || body.tail_expr().is_some() {
        return None;
    }
    let mut locals = BTreeMap::new();
    let mut inputs = Vec::new();
    for stmt in body.statements() {
        if let ast::Stmt::LetStmt(binding) = &stmt {
            let ast::Pat::IdentPat(pat) = binding.pat()? else {
                return None;
            };
            let name = pat.name()?.text().to_string();
            if pat.syntax().text().to_string() != name
                || name == constructor
                || name == owner.name
                || binding.attrs().next().is_some()
                || binding.let_else().is_some()
                || binding.ty().is_some()
            {
                return None;
            }
            let value = read_field(&binding.initializer()?.syntax().text().to_string())?;
            if locals.insert(name, value).is_some() {
                return None;
            }
            continue;
        }
        let mac = match stmt {
            ast::Stmt::Item(ast::Item::MacroCall(mac)) => mac,
            ast::Stmt::ExprStmt(expr) => {
                let ast::Expr::MacroExpr(call) = expr.expr()? else {
                    return None;
                };
                call.macro_call()?
            }
            _ => return None,
        };
        if mac.attrs().next().is_some() || mac.path()?.syntax().text() != "assert_eq" {
            return None;
        }
        let sides = equality_assertion_arguments(&mac.syntax().text().to_string())?;
        let [actual, expected] = sides.as_slice() else {
            return None;
        };
        read_field(expected).or_else(|| whole_integer_literal(expected))?;
        let args = match owner.item.container {
            FunctionContainer::TraitImpl { .. } => {
                let (left, right) = actual.split_once('+')?;
                vec![left.trim().to_owned(), right.trim().to_owned()]
            }
            _ => owner_call_arguments(actual, &owner.name)?,
        };
        if args.len() != signature.len() {
            return None;
        }
        let mut projected = None;
        for (i, (arg, (_, ty))) in args.iter().zip(signature).enumerate() {
            let value = if ty == constructor {
                locals
                    .get(arg.trim())
                    .copied()
                    .or_else(|| read_field(arg))?
            } else {
                integer_literal_in_type(arg, ty)?
            };
            if i == position {
                projected = Some(value);
            }
        }
        inputs.push(projected?);
    }
    Some(inputs)
}
