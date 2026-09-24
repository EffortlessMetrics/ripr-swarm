//! Fail-closed admission for the first derived-local tuple slice.
//!
//! The underlying discriminator understands one bounded closure shape, but its
//! text-shaped initializer and descendant-oracle helpers are intentionally not
//! authority on their own. This layer binds the admitted owner roles and one
//! executed top-level projection oracle before that discriminator may refine
//! evidence. Broader outer-local derivation remains unsupported.

use crate::analysis::classify::ProbeContext;
use crate::analysis::rust_index::find_file_facts;
use crate::domain::{Probe, RelationReason};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile, ast};

pub(super) fn admits(context: &ProbeContext<'_>) -> bool {
    admits_inner(context).unwrap_or(false)
}

fn admits_inner(context: &ProbeContext<'_>) -> Option<bool> {
    let owner = context.owner_fn?;
    let facts = find_file_facts(context.index, &owner.file)?;
    let root = parsed(&facts.source)?;
    let function = named_function(&root, &owner.name)?;
    if !canonical_owner_roles(&function)? {
        return Some(false);
    }
    let result = current_result(&facts.source, &function, context.probe)?;

    let related = context
        .related_tests
        .iter()
        .filter(|(_, reason)| *reason == RelationReason::DirectOwnerCall)
        .collect::<Vec<_>>();
    let [(test, _)] = related.as_slice() else {
        return Some(false);
    };
    let test_facts = find_file_facts(context.index, &test.file)?;
    let test_root = parsed(&test_facts.source)?;
    let test_function = named_function(&test_root, &test.name)?;
    top_level_projection_observes(&test_function, &owner.name, &result)
}

/// First-slice authority is the canonical helper boundary only. Merely finding
/// any owner parameter or any `.contains` call cannot assign semantic roles.
fn canonical_owner_roles(function: &ast::Fn) -> Option<bool> {
    if !canonical_receipt_iteration(function)?
        || !has_exact_parameter(
            function,
            "receipt_request_ids",
            "&BTreeMap<String,Vec<String>>",
        )?
        || !has_exact_parameter(function, "request_set", "&BTreeSet<String>")?
        || !has_exact_parameter(function, "task_id", "&str")?
    {
        return Some(false);
    }

    let source = compact(&function.syntax().text().to_string());
    Some(
        source
            .matches("receipt_request_ids.get(&receipt.id).is_some_and(")
            .count()
            == 1
            && source
                .matches("request_set.contains(request_id.as_str())")
                .count()
                == 1
            && source
                .matches("lettask_identity_matches=receipt.id==task_id;")
                .count()
                == 1,
    )
}

/// Bind the closure's source to the owner parameter, not a nearby spelling or
/// an unrelated collection with the same element shape.
fn canonical_receipt_iteration(function: &ast::Fn) -> Option<bool> {
    let mut parameters = function
        .param_list()?
        .params()
        .filter(|parameter| immutable_parameter_name(parameter).as_deref() == Some("receipts"));
    if parameters.next().is_none() || parameters.next().is_some() {
        return Some(false);
    }
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return Some(false);
    }
    let filtered = method_receiver(&body.tail_expr()?, "collect", 0)?;
    let iterated = method_receiver(&filtered, "filter_map", 1)?;
    let filter_call = ast::MethodCallExpr::cast(filtered.syntax().clone())?;
    let closure = filter_call.arg_list()?.args().next()?;
    if ast::ClosureExpr::cast(closure.syntax().clone()).is_none() {
        return Some(false);
    }
    let receiver = method_receiver(&iterated, "iter", 0)?;
    Some(direct_name(&receiver).as_deref() == Some("receipts"))
}

fn method_receiver(expression: &ast::Expr, name: &str, arity: usize) -> Option<ast::Expr> {
    let method = ast::MethodCallExpr::cast(expression.syntax().clone())?;
    if method.name_ref()?.text() != name || method.arg_list()?.args().count() != arity {
        return None;
    }
    method.receiver()
}

fn has_exact_parameter(
    function: &ast::Fn,
    expected_name: &str,
    expected_type: &str,
) -> Option<bool> {
    let mut matches = function.param_list()?.params().filter(|parameter| {
        immutable_parameter_name(parameter).as_deref() == Some(expected_name)
            && parameter
                .ty()
                .is_some_and(|value| compact(&value.syntax().text().to_string()) == expected_type)
    });
    let present = matches.next().is_some();
    Some(present && matches.next().is_none())
}

fn immutable_parameter_name(parameter: &ast::Param) -> Option<String> {
    let pattern = ast::IdentPat::cast(parameter.pat()?.syntax().clone())?;
    let name = pattern.name()?.text().to_string();
    (pattern.syntax().text().to_string().trim() == name).then_some(name)
}

fn current_result(source: &str, function: &ast::Fn, probe: &Probe) -> Option<String> {
    let after = probe.after.as_deref()?;
    let mut selected = None;
    for arm in function
        .syntax()
        .descendants()
        .filter_map(ast::MatchArm::cast)
    {
        if arm.guard().is_some() || arm.attrs().next().is_some() {
            continue;
        }
        let start = u32::from(arm.syntax().text_range().start()) as usize;
        let line = source
            .get(..start)?
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        if line != probe.location.line
            || !arm_source_matches(&arm, after)?
            || !arm_source_matches(&arm, &probe.expression)?
        {
            continue;
        }
        let result = plain_string(&arm.expr()?)?;
        if selected.replace(result).is_some() {
            return None;
        }
    }
    selected
}

fn top_level_projection_observes(
    function: &ast::Fn,
    owner: &str,
    expected_result: &str,
) -> Option<bool> {
    let attributes = function.attrs().collect::<Vec<_>>();
    let [attribute] = attributes.as_slice() else {
        return None;
    };
    if attribute.syntax().text() != "#[test]"
        || function.async_token().is_some()
        || function.param_list()?.params().next().is_some()
    {
        return None;
    }

    let body = function.body()?.stmt_list()?;
    if body.tail_expr().is_some() {
        return None;
    }
    let statements = body.statements().collect::<Vec<_>>();
    let mut result_binding = None;
    for (position, statement) in statements.iter().enumerate() {
        let ast::Stmt::LetStmt(binding) = statement else {
            continue;
        };
        let Some(initializer) = binding.initializer() else {
            continue;
        };
        let Some(call) = ast::CallExpr::cast(initializer.syntax().clone()) else {
            continue;
        };
        let Some(callee) = call.expr() else {
            continue;
        };
        if direct_name(&callee).as_deref() != Some(owner) {
            continue;
        }
        if result_binding.is_some() {
            return None;
        }
        result_binding = Some((immutable_binding_name(binding)?, position));
    }
    let (result_binding, owner_position) = result_binding?;
    // Equal spelling is not binding identity. Reject shadowing, including
    // destructuring and nested patterns, rather than borrowing their assertions.
    let binding_count = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| {
            pattern
                .name()
                .is_some_and(|name| name.text() == result_binding.as_str())
        })
        .count();
    if binding_count != 1 {
        return Some(false);
    }

    let mut length_observed = false;
    let mut result_observed = false;
    for statement in statements.into_iter().skip(owner_position + 1) {
        let ast::Stmt::ExprStmt(statement) = statement else {
            continue;
        };
        let Some(expression) = statement.expr() else {
            continue;
        };
        let Some(macro_expression) = ast::MacroExpr::cast(expression.syntax().clone()) else {
            continue;
        };
        let Some(call) = macro_expression.macro_call() else {
            continue;
        };
        if call.path()?.syntax().text() != "assert_eq" {
            continue;
        }
        let Some((left, right)) = assertion_operands(&call) else {
            continue;
        };
        let left_text = compact(&left.syntax().text().to_string());
        let right_text = compact(&right.syntax().text().to_string());
        let length = format!("{result_binding}.len()");
        let result = format!("{result_binding}[0].1");
        if (left_text == length && right_text == "1") || (right_text == length && left_text == "1")
        {
            length_observed = true;
        }
        if (left_text == result && plain_string(&right).as_deref() == Some(expected_result))
            || (right_text == result && plain_string(&left).as_deref() == Some(expected_result))
        {
            result_observed = true;
        }
    }

    Some(length_observed && result_observed)
}

fn assertion_operands(call: &ast::MacroCall) -> Option<(ast::Expr, ast::Expr)> {
    let tokens = call.token_tree()?.syntax().text().to_string();
    let inner = tokens.strip_prefix('(')?.strip_suffix(')')?;
    let source = format!("fn __operands() {{ ({inner}) }}");
    let root = parsed(&source)?;
    let function = named_function(&root, "__operands")?;
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let tuple = ast::TupleExpr::cast(body.tail_expr()?.syntax().clone())?;
    let operands = tuple
        .syntax()
        .children()
        .filter_map(ast::Expr::cast)
        .collect::<Vec<_>>();
    let [left, right, ..] = operands.as_slice() else {
        return None;
    };
    Some((left.clone(), right.clone()))
}

fn immutable_binding_name(statement: &ast::LetStmt) -> Option<String> {
    let pattern = ast::IdentPat::cast(statement.pat()?.syntax().clone())?;
    let name = pattern.name()?.text().to_string();
    (pattern.syntax().text().to_string().trim() == name).then_some(name)
}

fn direct_name(expression: &ast::Expr) -> Option<String> {
    let path = ast::PathExpr::cast(expression.syntax().clone())?.path()?;
    let text = path.syntax().text().to_string();
    valid_identifier(&text).then_some(text)
}

fn valid_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn arm_source_matches(arm: &ast::MatchArm, claimed: &str) -> Option<bool> {
    let full = arm.syntax().text().to_string();
    let start = u32::from(arm.syntax().text_range().start());
    let end = u32::from(arm.fat_arrow_token()?.text_range().end());
    let boundary = end.checked_sub(start)? as usize;
    let pattern = full.get(..boundary)?;
    Some(arm_text(&full) == arm_text(claimed) || pattern.trim() == claimed.trim())
}

fn arm_text(text: &str) -> &str {
    text.trim().trim_end_matches(',').trim_end()
}

fn plain_string(expression: &ast::Expr) -> Option<String> {
    let literal = ast::Literal::cast(expression.syntax().clone())?;
    let text = literal.syntax().text().to_string();
    let value = text.strip_prefix('"')?.strip_suffix('"')?;
    (!value.contains(['\\', '"', '\n', '\r'])).then(|| value.to_string())
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn parsed(source: &str) -> Option<ast::SourceFile> {
    let parse = SourceFile::parse(source, Edition::CURRENT);
    parse.errors().is_empty().then(|| parse.tree())
}

fn named_function(root: &ast::SourceFile, name: &str) -> Option<ast::Fn> {
    let mut functions = root
        .syntax()
        .children()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|value| value.text() == name));
    let function = functions.next()?;
    functions.next().is_none().then_some(function)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iteration_is_admitted(source: &str) -> bool {
        parsed(source)
            .and_then(|root| named_function(&root, "terminalize_proof"))
            .and_then(|function| canonical_receipt_iteration(&function))
            .unwrap_or(false)
    }

    fn projection_is_admitted(body: &str) -> bool {
        let source = format!("#[test] fn projection() {{ {body} }}");
        parsed(&source)
            .and_then(|root| named_function(&root, "projection"))
            .and_then(|function| {
                top_level_projection_observes(&function, "terminalize_proof", "request_identity_v2")
            })
            .unwrap_or(false)
    }

    #[test]
    fn canonical_receipts_parameter_supplies_the_iterator() {
        assert!(iteration_is_admitted(
            "fn terminalize_proof(receipts: &[Receipt]) { receipts.iter().filter_map(|receipt| Some(receipt)).collect() }"
        ));
    }

    #[test]
    fn unrelated_receipts_cannot_supply_the_iterator() {
        assert!(!iteration_is_admitted(
            "fn terminalize_proof(receipts: &[Receipt], unrelated_receipts: &[Receipt]) { unrelated_receipts.iter().filter_map(|receipt| Some(receipt)).collect() }"
        ));
    }

    #[test]
    fn local_or_transformed_receipts_remain_unverified() {
        for source in [
            "fn terminalize_proof(input: &[Receipt]) { let receipts = input; receipts.iter().filter_map(|receipt| Some(receipt)).collect() }",
            "fn terminalize_proof(receipts: &[Receipt]) { receipts.iter().skip(1).filter_map(|receipt| Some(receipt)).collect() }",
            "fn terminalize_proof(receipts: &[Receipt]) { receipts.iter().filter_map(adapter(|receipt| Some(receipt))).collect() }",
        ] {
            assert!(!iteration_is_admitted(source), "{source}");
        }
    }

    #[test]
    fn assertions_after_the_unique_owner_result_are_admitted() {
        assert!(projection_is_admitted(
            r#"let terminal = terminalize_proof();
            assert_eq!(terminal.len(), 1);
            assert_eq!(terminal[0].1, "request_identity_v2");"#
        ));
    }

    #[test]
    fn shadowed_owner_result_cannot_borrow_assertions() {
        for shadow in [
            r#"let terminal = [(0, "request_identity_v2")];"#,
            r#"let (terminal,) = ([(0, "request_identity_v2")],);"#,
        ] {
            let body = format!(
                r#"let terminal = terminalize_proof();
                {shadow}
                assert_eq!(terminal.len(), 1);
                assert_eq!(terminal[0].1, "request_identity_v2");"#
            );
            assert!(!projection_is_admitted(&body), "{shadow}");
        }
    }

    #[test]
    fn assertions_before_the_owner_call_are_not_its_observation() {
        assert!(!projection_is_admitted(
            r#"assert_eq!(terminal.len(), 1);
            assert_eq!(terminal[0].1, "request_identity_v2");
            let terminal = terminalize_proof();"#
        ));
    }
}
