//! Exact terminal projection grammar for the real #1728 consumer remainder.
//!
//! This is a bounded parsed-source contract, not evaluation of arbitrary Rust.
//! A single parser-owned match is the only hole in the frozen consumer body.
//! Token identity retains string whitespace, operators and literal types; only
//! lexer trivia is ignored. Unknown statements, writes or result routes refuse.
use super::{
    assertion_operands, direct_name, has_exact_parameter, immutable_binding_name, named_function,
    parsed, plain_string,
};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};
use std::collections::BTreeMap;

const TERMINAL_BODY: &str = r#"{
    let mut request_ids = proof_request_ids.get(task_id).cloned().unwrap_or_default();
    request_ids.sort();
    request_ids.dedup();
    let request_set = request_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut matching = receipts
        .iter()
        .filter_map(|receipt| {
            let request_identity_matches =
                receipt_request_ids
                    .get(&receipt.id)
                    .is_some_and(|receipt_requests| {
                        receipt_requests
                            .iter()
                            .any(|request_id| request_set.contains(request_id.as_str()))
                    });
            let task_identity_matches = receipt.id == task_id;
            let relation = __tuple_relation;
            Some((receipt, relation))
        })
        .collect::<Vec<_>>();
    matching.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));
    let receipt_ids = matching
        .iter()
        .map(|(receipt, _)| receipt.id.clone())
        .collect::<Vec<_>>();
    if matching.is_empty() {
        let (status, reason) = if plan_status == "planned" {
            (
                "not_executed".to_owned(),
                "planned proof task has no terminal receipt joined by request or task identity"
                    .to_owned(),
            )
        } else {
            (
                plan_status.to_owned(),
                "proof task retained its terminal planner disposition".to_owned(),
            )
        };
        return Ok((status, reason, request_ids, receipt_ids));
    }
    let mut results = matching
        .iter()
        .map(|(receipt, _)| terminal_proof_receipt_status(receipt))
        .collect::<Result<Vec<_>>>()?;
    results.sort();
    results.dedup();
    let status = if results.len() == 1 {
        results[0].clone()
    } else {
        "multiple_terminal_receipts".to_owned()
    };
    let reason = matching
        .iter()
        .map(|(receipt, relation)| format!("{}={} join={relation}", receipt.id, receipt.result))
        .collect::<Vec<_>>()
        .join("; ");
    Ok((status, reason, request_ids, receipt_ids))
}
"#;

fn tokens(node: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
        .map(|token| (token.kind(), token.text().to_string()))
        .collect()
}

/// Bind the one closure match to the outer request derivation and all four
/// outward fields. Exact grammar refuses extra/shadowing/mutating statements.
/// The sibling derived witness separately proves arm and initializer identity.
pub(super) fn owner_match(function: &ast::Fn) -> Option<ast::MatchExpr> {
    let root = function
        .syntax()
        .ancestors()
        .find_map(ast::SourceFile::cast)?;
    // A local formatter/constructor replacement can return a diagnostic
    // constant without using the relation. Exact body spelling alone would
    // then cease to establish the outward value boundary.
    if root.syntax().descendants().any(|node| {
        ast::MacroRules::can_cast(node.kind())
            || ast::MacroDef::can_cast(node.kind())
            || ast::ExternCrate::can_cast(node.kind())
            || ast::Fn::cast(node.clone()).is_some_and(|function| {
                function
                    .name()
                    .is_some_and(|name| matches!(name.text(), "Ok" | "Some"))
            })
            || ast::Use::cast(node).is_some_and(|import| {
                import
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|node| node.into_token())
                    .any(|token| matches!(token.text(), "format" | "Ok" | "Some"))
                    || tokens(import.syntax())
                        .iter()
                        .any(|(_, token)| token == "*")
                        && tokens(import.syntax())
                            .into_iter()
                            .map(|(_, token)| token)
                            .collect::<String>()
                            != "usecrate::*;"
            })
    }) {
        return None;
    }
    if function.attrs().next().is_some()
        || function.async_token().is_some()
        || function.param_list()?.params().count() != 5
    {
        return None;
    }
    let parameters = function.param_list()?.params().collect::<Vec<_>>();
    for (position, (name, ty)) in [
        ("task_id", "&str"),
        ("plan_status", "&str"),
        ("receipts", "&[ProofReceipt]"),
        ("proof_request_ids", "&BTreeMap<String,Vec<String>>"),
        ("receipt_request_ids", "&BTreeMap<String,Vec<String>>"),
    ]
    .into_iter()
    .enumerate()
    {
        if super::immutable_parameter_name(&parameters[position]).as_deref() != Some(name)
            || !has_exact_parameter(function, name, ty)?
        {
            return None;
        }
    }
    let mut matches = function
        .syntax()
        .descendants()
        .filter_map(ast::MatchExpr::cast);
    let selected = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let body = function.body()?;
    let start = u32::from(body.syntax().text_range().start());
    let from = u32::from(selected.syntax().text_range().start()).checked_sub(start)? as usize;
    let to = u32::from(selected.syntax().text_range().end()).checked_sub(start)? as usize;
    let body_text = body.syntax().text().to_string();
    let masked = format!(
        "fn __terminal() {}__tuple_relation{}",
        body_text.get(..from)?,
        body_text.get(to..)?
    );
    let root = parsed(&masked)?;
    let masked_body = named_function(&root, "__terminal")?.body()?;
    let canonical = parsed(&format!("fn __terminal() {TERMINAL_BODY}"))?;
    let canonical_body = named_function(&canonical, "__terminal")?.body()?;
    (tokens(masked_body.syntax()) == tokens(canonical_body.syntax())).then_some(selected)
}

fn expression(source: &str) -> Option<ast::Expr> {
    let root = parsed(&format!("fn __value() {{ {source} }}"))?;
    named_function(&root, "__value")?
        .body()?
        .stmt_list()?
        .tail_expr()
}

fn owned_string(expression: &ast::Expr) -> Option<String> {
    let call = ast::MethodCallExpr::cast(expression.syntax().clone())?;
    if call.name_ref()?.text() != "to_string" || call.arg_list()?.args().next().is_some() {
        return None;
    }
    plain_string(&call.receiver()?)
}

fn vector(expression: &ast::Expr) -> Option<Vec<ast::Expr>> {
    let call = ast::MacroExpr::cast(expression.syntax().clone())?.macro_call()?;
    if call.path()?.syntax().text() != "vec" {
        return None;
    }
    let text = call.token_tree()?.syntax().text().to_string();
    if !text.starts_with('[') || !text.ends_with(']') {
        return None;
    }
    let array = ast::ArrayExpr::cast(self::expression(&text)?.syntax().clone())?;
    if array.semicolon_token().is_some()
        || array
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    Some(array.exprs().collect())
}

fn strings(expression: &ast::Expr) -> Option<Vec<String>> {
    vector(expression)?.iter().map(owned_string).collect()
}

fn map(expression: &ast::Expr) -> Option<BTreeMap<String, Vec<String>>> {
    if expression
        .syntax()
        .descendants()
        .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let call = ast::CallExpr::cast(expression.syntax().clone())?;
    if call.expr()?.syntax().text() != "BTreeMap::from" {
        return None;
    }
    let args = call.arg_list()?.args().collect::<Vec<_>>();
    let [argument] = args.as_slice() else {
        return None;
    };
    let array = ast::ArrayExpr::cast(argument.syntax().clone())?;
    if array.semicolon_token().is_some() {
        return None;
    }
    let mut values = BTreeMap::new();
    for entry in array.exprs() {
        let tuple = ast::TupleExpr::cast(entry.syntax().clone())?;
        let fields = tuple.fields().collect::<Vec<_>>();
        let [key, value] = fields.as_slice() else {
            return None;
        };
        if values.insert(owned_string(key)?, strings(value)?).is_some() {
            return None;
        }
    }
    Some(values)
}

fn receipt(expression: &ast::Expr) -> Option<(String, String)> {
    let values = vector(expression)?;
    let [value] = values.as_slice() else {
        return None;
    };
    let record = ast::RecordExpr::cast(value.syntax().clone())?;
    if record.path()?.syntax().text() != "ProofReceipt" {
        return None;
    }
    let fields = record.record_expr_field_list()?;
    if fields.spread().is_some() {
        return None;
    }
    let mut id = None;
    let mut result = None;
    for field in fields.fields() {
        let value = owned_string(&field.expr()?)?;
        match field.name_ref()?.text() {
            "id" if id.is_none() => id = Some(value),
            "result" if result.is_none() => result = Some(value),
            _ => return None,
        }
    }
    Some((id?, result?))
}

fn same(expression: &ast::Expr, expected: &str) -> Option<bool> {
    Some(tokens(expression.syntax()) == tokens(self::expression(expected)?.syntax()))
}

fn quoted(value: &str) -> Option<String> {
    // The admitted literal grammar excludes escapes, so re-quoting preserves
    // exactly the parsed literal rather than introducing another language.
    (!value.contains(['"', '\\', '\n', '\r'])).then(|| format!("{value:?}"))
}

fn string_vector(values: &[String]) -> Option<String> {
    let items = values
        .iter()
        .map(|value| Some(format!("{}.to_string()", quoted(value)?)))
        .collect::<Option<Vec<_>>>()?;
    Some(format!("vec![{}]", items.join(",")))
}

/// A single native top-level oracle, after a direct five-argument owner call.
/// Its immutable map inputs determine the tuple, and literal expectations pin
/// status, reason, sorted/deduplicated requests and the exact receipt identity.
/// Extra statements, nested assertions, aliases and writes remain unsupported.
pub(super) fn projection_observes(
    function: &ast::Fn,
    owner: &str,
    result: &str,
    selected: Option<[bool; 2]>,
) -> Option<bool> {
    // Map construction has its standard-library semantics only in this
    // standalone namespace. The caller separately refuses imports/aliases
    // that replace the owner or assertion macros.
    let root = function
        .syntax()
        .ancestors()
        .find_map(ast::SourceFile::cast)?;
    let standard_import = parsed("use std::collections::BTreeMap;")?;
    let standard_import = standard_import.syntax().children().next()?;
    if !root
        .syntax()
        .children()
        .filter_map(ast::Use::cast)
        .any(|import| tokens(import.syntax()) == tokens(&standard_import))
    {
        return None;
    }
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
    if body.tail_expr().is_some()
        || body
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let statements = body.statements().collect::<Vec<_>>();
    let [
        ast::Stmt::LetStmt(receipts),
        ast::Stmt::LetStmt(proofs),
        ast::Stmt::LetStmt(requests),
        ast::Stmt::LetStmt(terminal),
        ast::Stmt::ExprStmt(_),
        ast::Stmt::ExprStmt(_),
        ast::Stmt::ExprStmt(_),
        ast::Stmt::ExprStmt(_),
    ] = statements.as_slice()
    else {
        return None;
    };
    if immutable_binding_name(receipts)? != "receipts"
        || immutable_binding_name(proofs)? != "proof_request_ids"
        || immutable_binding_name(requests)? != "receipt_request_ids"
    {
        return None;
    }
    let binding = immutable_binding_name(terminal)?;
    if ["receipts", "proof_request_ids", "receipt_request_ids"].contains(&binding.as_str()) {
        return None;
    }
    let (identity, terminal_result) = receipt(&receipts.initializer()?)?;
    if terminal_result != "head_passed" {
        return None;
    }
    let proof_map = map(&proofs.initializer()?)?;
    let receipt_map = map(&requests.initializer()?)?;
    let unwrap = ast::MethodCallExpr::cast(terminal.initializer()?.syntax().clone())?;
    if unwrap.name_ref()?.text() != "unwrap" || unwrap.arg_list()?.args().next().is_some() {
        return None;
    }
    let call = ast::CallExpr::cast(unwrap.receiver()?.syntax().clone())?;
    if direct_name(&call.expr()?).as_deref() != Some(owner) {
        return None;
    }
    let args = call.arg_list()?.args().collect::<Vec<_>>();
    let [task, plan, fed_receipts, fed_proofs, fed_requests] = args.as_slice() else {
        return None;
    };
    let task_id = plain_string(task)?;
    if plain_string(plan)?.as_str() != "planned"
        || !same(fed_receipts, "&receipts")?
        || !same(fed_proofs, "&proof_request_ids")?
        || !same(fed_requests, "&receipt_request_ids")?
    {
        return None;
    }
    let mut request_ids = proof_map.get(&task_id).cloned().unwrap_or_default();
    request_ids.sort();
    request_ids.dedup();
    let request_match = receipt_map
        .get(&identity)
        .is_some_and(|ids| ids.iter().any(|id| request_ids.contains(id)));
    let input = [request_match, identity == task_id];
    if input == [false, false] || selected.is_some_and(|selected| selected != input) {
        return Some(false);
    }
    let expected_reason = format!("{identity}={terminal_result} join={result}");
    let expectations = [
        quoted(&terminal_result)?,
        quoted(&expected_reason)?,
        string_vector(&request_ids)?,
        string_vector(&[identity])?,
    ];
    let mut observed = [false; 4];
    for statement in &statements[4..] {
        let ast::Stmt::ExprStmt(statement) = statement else {
            return None;
        };
        let macro_expr = ast::MacroExpr::cast(statement.expr()?.syntax().clone())?;
        let assertion = macro_expr.macro_call()?;
        if assertion.path()?.syntax().text() != "assert_eq" {
            return None;
        }
        let (left, right) = assertion_operands(&assertion)?;
        let mut matched = false;
        for (field, expected) in expectations.iter().enumerate() {
            let access = format!("{binding}.{field}");
            if (same(&left, &access)? && same(&right, expected)?)
                || (same(&right, &access)? && same(&left, expected)?)
            {
                if observed[field] {
                    return Some(false);
                }
                observed[field] = true;
                matched = true;
            }
        }
        if !matched {
            return Some(false);
        }
    }
    Some(observed.iter().all(|value| *value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_literal_tokens_preserve_whitespace_and_ignore_only_trivia() {
        let left = expression(r#"format!("id=result join={relation}")"#);
        let respelled = expression(r#"format! /* harmless */ ( "id=result join={relation}" )"#);
        let changed = expression(r#"format!("id=resultjoin={relation}")"#);
        assert!(left.is_some() && respelled.is_some() && changed.is_some());
        assert_eq!(
            left.as_ref().map(|value| tokens(value.syntax())),
            respelled.as_ref().map(|value| tokens(value.syntax()))
        );
        assert_ne!(
            left.as_ref().map(|value| tokens(value.syntax())),
            changed.as_ref().map(|value| tokens(value.syntax()))
        );
    }

    #[test]
    fn terminal_maps_refuse_duplicate_keys_and_nonliteral_values() {
        for source in [
            r#"BTreeMap::from([("task".to_string(), vec!["request".to_string()]), ("task".to_string(), vec![])])"#,
            r#"BTreeMap::from([("task".to_string(), vec![request.to_string()])])"#,
            r#"OtherMap::from([("task".to_string(), vec!["request".to_string()])])"#,
        ] {
            assert!(expression(source).is_some(), "control must parse");
            assert!(
                expression(source).and_then(|value| map(&value)).is_none(),
                "{source}"
            );
        }
        let admitted =
            expression(r#"BTreeMap::from([("task".to_string(), vec!["request".to_string()])])"#)
                .and_then(|value| map(&value));
        assert_eq!(
            admitted.and_then(|values| values.get("task").cloned()),
            Some(vec!["request".to_string()])
        );
    }
}
