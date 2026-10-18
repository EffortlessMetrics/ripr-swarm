//! Constructor-preserving projection through the existing per-test binding facts.

use super::*;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericParams, HasName},
};

impl ValueEnvFacts {
    /// Read one closed Option field at this exact shared-borrowed call. The
    /// scalar resolver deliberately unwraps constructors; arm selection must not.
    pub(crate) fn option_field_at_call(
        &self,
        test: &TestSummary,
        source: &str,
        input: &str,
        record_type: &str,
        field: &str,
        call: (usize, &str),
    ) -> Option<String> {
        let (call_line, call_text) = call;
        let object = unwrap_shared_borrow(input.trim())?;
        if !is_simple_identifier(object) {
            return None;
        }
        let relative_line = call_line.checked_sub(test.start_line)?;
        let line = test.body.lines().nth(relative_line)?;
        // A repeated occurrence needs a richer call identity. Refuse rather
        // than borrowing a later same-line write for an earlier invocation.
        let mut occurrences = line.match_indices(call_text);
        let (column, _) = occurrences.next()?;
        if occurrences.next().is_some() {
            return None;
        }
        let call_position = SourcePosition {
            line: call_line,
            column: column.saturating_sub(line.len() - line.trim_start().len()),
        };
        let binding_position = self
            .local_binding_positions
            .get(object)?
            .iter()
            .copied()
            .filter(|p| p.at_or_before(call_position))
            .max_by_key(|p| (p.line, p.column))?;
        let parsed_test = crate::analysis::syntax::parse_clean_source_file(&test.body)?;
        let parsed_source = crate::analysis::syntax::parse_clean_source_file(source)?;
        let test_tree = parsed_test.tree();
        let tree = parsed_source.tree();
        // Conditional attributes can remove bindings or writes without changing
        // their lexical order. This slice has no cfg evaluation authority.
        if test_tree
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
        {
            return None;
        }
        let binding = test_tree
            .syntax()
            .descendants()
            .filter_map(ast::LetStmt::cast)
            .find(|binding| {
                position_at_offset(
                    &test.body,
                    usize::from(binding.syntax().text_range().start()),
                    test.start_line,
                ) == binding_position
            })?;
        if !is_unconditional_test_statement(
            &self.body_clean,
            usize::from(binding.syntax().text_range().start()),
        ) {
            return None;
        }
        let callers = tree
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                let Some(token) = function.fn_token() else {
                    return false;
                };
                let start = usize::from(token.text_range().start());
                let end = usize::from(function.syntax().text_range().end());
                source[..start]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1
                    == test.start_line
                    && source.get(start..end) == Some(test.body.as_ref())
            })
            .collect::<Vec<_>>();
        let [caller] = callers.as_slice() else {
            return None;
        };
        let initial = closed_record_field(
            binding.initializer()?,
            &tree,
            &caller,
            &[],
            record_type,
            field,
            0,
        )?;
        // A whole-object reassignment, move, method call or mutable borrow
        // escapes the supported ledger identity. Field reads/writes and shared
        // borrows preserve the selected primitive Option field.
        for path in test_tree
            .syntax()
            .descendants()
            .filter_map(ast::PathExpr::cast)
        {
            if path.syntax().text().to_string() != object {
                continue;
            }
            let position = position_at_offset(
                &test.body,
                usize::from(path.syntax().text_range().start()),
                test.start_line,
            );
            if !binding_position.at_or_before(position) || !position.at_or_before(call_position) {
                continue;
            }
            let parent = path.syntax().parent()?;
            if let Some(field) = ast::FieldExpr::cast(parent.clone()) {
                if field_use_is_closed(&field) {
                    continue;
                }
                return None;
            }
            if let Some(borrow) = ast::RefExpr::cast(parent)
                && borrow.mut_token().is_none()
            {
                continue;
            }
            return None;
        }
        for call in test_tree
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            let position = position_at_offset(
                &test.body,
                usize::from(call.syntax().text_range().start()),
                test.start_line,
            );
            if binding_position.at_or_before(position)
                && position.at_or_before(call_position)
                && !macro_uses_are_closed(&call, object, 0)?
            {
                return None;
            }
        }
        if let Some(assignment) = self.field_assignment_candidate_at(object, field, call_position) {
            if self.field_assignment_is_invalidated(object, assignment.position, call_position) {
                return None;
            }
            return assignment.option_value.clone();
        }
        if self.field_assignment_is_invalidated(object, binding_position, call_position) {
            return None;
        }
        Some(initial)
    }
}

/// This is a bounded record-return shape, not an evaluator: synchronous bare
/// calls, at most two helper bodies, statement-free tails, literal/parameter
/// arguments and fields. The selected field itself must be a closed Option.
fn closed_record_field(
    expression: ast::Expr,
    source: &ast::SourceFile,
    caller: &ast::Fn,
    parameters: &[String],
    record_type: &str,
    selected: &str,
    depth: usize,
) -> Option<String> {
    match expression {
        ast::Expr::RecordExpr(record) => {
            if record.path()?.syntax().text().to_string() != record_type {
                return None;
            }
            let fields = record.record_expr_field_list()?;
            if fields.dotdot_token().is_some() {
                return None;
            }
            let mut seen = std::collections::BTreeSet::new();
            let mut value = None;
            for field in fields.fields() {
                let name = field
                    .name_ref()
                    .map(|name| name.text().to_string())
                    .or_else(|| match field.expr()? {
                        ast::Expr::PathExpr(path) => Some(path.syntax().text().to_string()),
                        _ => None,
                    })?;
                if !is_simple_identifier(&name) {
                    return None;
                }
                if !seen.insert(name.clone()) || field.attrs().next().is_some() {
                    return None;
                }
                let text = field
                    .expr()
                    .map(|expr| expr.syntax().text().to_string())
                    .unwrap_or_else(|| name.clone());
                if name == selected {
                    value = closed_option(&text);
                    value.as_ref()?;
                } else if !parameters.contains(&text)
                    && !looks_like_literal(&text)
                    && closed_option(&text).is_none()
                {
                    return None;
                }
            }
            value
        }
        ast::Expr::CallExpr(call) if depth < 2 => {
            let name = call.expr()?.syntax().text().to_string();
            if !is_simple_identifier(&name) {
                return None;
            }
            let functions = source
                .syntax()
                .descendants()
                .filter_map(ast::Fn::cast)
                .filter(|f| f.name().is_some_and(|n| n.text() == name))
                .collect::<Vec<_>>();
            let [function] = functions.as_slice() else {
                return None;
            };
            if source
                .syntax()
                .descendants()
                .filter_map(ast::Name::cast)
                .any(|bound| {
                    let text = bound.text();
                    text.strip_prefix("r#").unwrap_or(text) == name
                        && bound.syntax().parent().as_ref() != Some(function.syntax())
                })
            {
                return None;
            }
            if function.async_token().is_some()
                || function.unsafe_token().is_some()
                || function.const_token().is_some()
                || function.abi().is_some()
                || function.generic_param_list().is_some()
                || function.attrs().next().is_some()
                || function.ret_type()?.ty()?.syntax().text().to_string() != record_type
            {
                return None;
            }
            // Root declarations and declarations in this caller's own inline
            // module are the only helper namespace this slice can establish.
            let parent = function.syntax().parent()?;
            let caller_parent = caller.syntax().parent()?;
            if parent != caller_parent && parent != *source.syntax() {
                return None;
            }
            if parent != caller_parent
                && !caller_parent
                    .children()
                    .filter_map(ast::Use::cast)
                    .any(|import| {
                        import.attrs().next().is_none()
                            && import
                                .use_tree()
                                .is_some_and(|tree| tree.syntax().text().to_string() == "super::*")
                    })
            {
                return None;
            }
            // Refuse local shadows and explicit imports with the helper's name.
            if crate::analysis::syntax::fn_name_binding_count(
                &caller.syntax().text().to_string(),
                &name,
            ) != Some(0)
                || source
                    .syntax()
                    .descendants()
                    .filter_map(ast::Use::cast)
                    .any(|import| {
                        contains_identifier_token(&import.syntax().text().to_string(), &name)
                    })
            {
                return None;
            }
            let args = call.arg_list()?.args().collect::<Vec<_>>();
            let params = function.param_list()?.params().collect::<Vec<_>>();
            if args.len() != params.len() {
                return None;
            }
            let mut names = Vec::new();
            for (arg, param) in args.into_iter().zip(params) {
                let text = arg.syntax().text().to_string();
                if !looks_like_literal(&text) && !parameters.contains(&text) {
                    return None;
                }
                let name = param.pat()?.syntax().text().to_string();
                if !is_simple_identifier(&name)
                    || !matches!(
                        param.ty()?.syntax().text().to_string().as_str(),
                        "u64" | "u128"
                    )
                {
                    return None;
                }
                names.push(name);
            }
            let body = function.body()?;
            if body.statements().next().is_some() {
                return None;
            }
            closed_record_field(
                body.tail_expr()?,
                source,
                function,
                &names,
                record_type,
                selected,
                depth + 1,
            )
        }
        _ => None,
    }
}

pub(super) fn closed_option(text: &str) -> Option<String> {
    let text = text.trim();
    if text == "None" {
        return Some(text.to_string());
    }
    let value = text.strip_prefix("Some(")?.strip_suffix(')')?.trim();
    (parse_integer_literal(value)? >= 0).then(|| text.to_string())
}

fn field_use_is_closed(field: &ast::FieldExpr) -> bool {
    let Some(parent) = field_context(field) else {
        return false;
    };
    !ast::MethodCallExpr::can_cast(parent.kind())
        && ast::RefExpr::cast(parent).is_none_or(|borrow| borrow.mut_token().is_none())
}

// Macro arguments remain syntax in the caller. Parse their tuple of expressions
// to apply the same binding-use boundary, including implicit field-method borrows.
fn macro_uses_are_closed(call: &ast::MacroCall, object: &str, depth: usize) -> Option<bool> {
    if depth >= 4 {
        return None;
    }
    let tokens = call.token_tree()?.syntax().text().to_string();
    let inner = tokens.get(1..tokens.len().checked_sub(1)?)?;
    let wrapped = format!("fn f() {{ let _ = ({inner}); }}");
    let parsed = crate::analysis::syntax::parse_clean_source_file(&wrapped)?;
    let tree = parsed.tree();
    for path in tree.syntax().descendants().filter_map(ast::PathExpr::cast) {
        if path.syntax().text().to_string() != object {
            continue;
        }
        let parent = path.syntax().parent()?;
        if let Some(field) = ast::FieldExpr::cast(parent.clone()) {
            if field_use_is_closed(&field) && !field_is_assignment_target(&field) {
                continue;
            }
            return Some(false);
        }
        if ast::RefExpr::cast(parent).is_some_and(|borrow| borrow.mut_token().is_none()) {
            continue;
        }
        return Some(false);
    }
    for nested in tree.syntax().descendants().filter_map(ast::MacroCall::cast) {
        if !macro_uses_are_closed(&nested, object, depth + 1)? {
            return Some(false);
        }
    }
    Some(true)
}

fn field_context(field: &ast::FieldExpr) -> Option<ra_ap_syntax::SyntaxNode> {
    let mut parent = field.syntax().parent()?;
    while ast::ParenExpr::can_cast(parent.kind()) {
        parent = parent.parent()?;
    }
    Some(parent)
}

fn field_is_assignment_target(field: &ast::FieldExpr) -> bool {
    field_context(field)
        .and_then(ast::BinExpr::cast)
        .is_some_and(|binary| matches!(binary.op_kind(), Some(ast::BinaryOp::Assignment { .. })))
}
