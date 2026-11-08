//! Bounded fast-check `assert` → `property` → predicate static evidence (#10751).
//!
//! Only a supported import of `fast-check` plus `assert(property(...))` whose
//! predicate returns a meaningful boolean, throws, or uses a recognized assert
//! call earns oracle credit. Construction-only `property`, ignored `check`,
//! block bodies without `return`, and shadowed bindings do not.

use super::*;

/// Resolved fast-check API bindings for one file (after shadowing).
#[derive(Clone, Debug, Default)]
pub(crate) struct FastCheckExports {
    pub(crate) namespace: Option<String>,
    pub(crate) assert_locals: Vec<String>,
    pub(crate) property_locals: Vec<String>,
    pub(crate) async_property_locals: Vec<String>,
}

impl FastCheckExports {
    pub(crate) fn from_bindings(bindings: &TypeScriptAssertionBindings) -> Self {
        Self {
            namespace: bindings.fast_check_namespace.clone(),
            assert_locals: bindings.fast_check_assert_locals.clone(),
            property_locals: bindings.fast_check_property_locals.clone(),
            async_property_locals: bindings.fast_check_async_property_locals.clone(),
        }
    }

    fn is_empty(&self) -> bool {
        self.namespace.is_none()
            && self.assert_locals.is_empty()
            && self.property_locals.is_empty()
            && self.async_property_locals.is_empty()
    }

    fn resolves_namespace_member(&self, object: &str, member: &str) -> bool {
        self.namespace.as_deref() == Some(object)
            && matches!(
                member,
                "assert"
                    | "property"
                    | "asyncProperty"
                    | "check"
                    | "pre"
                    | "integer"
                    | "commands"
                    | "modelRun"
                    | "asyncModelRun"
                    | "scheduledModelRun"
            )
    }

    fn is_assert(&self, name: &str) -> bool {
        self.assert_locals.iter().any(|local| local == name)
    }

    fn is_property(&self, name: &str, async_property: bool) -> bool {
        if async_property {
            self.async_property_locals.iter().any(|local| local == name)
        } else {
            self.property_locals.iter().any(|local| local == name)
        }
    }
}

pub(crate) fn append_fast_check_assertions(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
    returns_to_runner: bool,
    out: &mut Vec<TypeScriptAssertion>,
) {
    let exports = FastCheckExports::from_bindings(bindings);
    if exports.is_empty() {
        return;
    }
    collect_fast_check_from_statement_vec(statements, source, &exports, returns_to_runner, out);
}

fn collect_fast_check_from_statement_vec(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &SourceText<'_>,
    exports: &FastCheckExports,
    returns_to_runner: bool,
    out: &mut Vec<TypeScriptAssertion>,
) {
    for stmt in statements {
        collect_in_statement(stmt, source, exports, returns_to_runner, out);
        if super::statement_completes_abruptly(stmt) {
            break;
        }
    }
}

pub(crate) fn assertions_from_expression(
    expr: &Expression<'_>,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
    returns_to_runner: bool,
) -> Option<TypeScriptAssertion> {
    let exports = FastCheckExports::from_bindings(bindings);
    if exports.is_empty() {
        return None;
    }
    assertion_from_assert_property_call(expr, source, &exports, returns_to_runner)
}

fn collect_in_statement(
    stmt: &Statement<'_>,
    source: &SourceText<'_>,
    exports: &FastCheckExports,
    returns_to_runner: bool,
    out: &mut Vec<TypeScriptAssertion>,
) {
    match stmt {
        Statement::BlockStatement(block) => {
            collect_fast_check_from_statement_vec(
                &block.body,
                source,
                exports,
                returns_to_runner,
                out,
            );
        }
        Statement::ExpressionStatement(expr_stmt) => {
            if let Some(assertion) = assertion_from_assert_property_call(
                &expr_stmt.expression,
                source,
                exports,
                returns_to_runner,
            ) {
                out.push(assertion);
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(argument) = &ret.argument
                && let Some(assertion) = assertion_from_assert_property_call(
                    argument,
                    source,
                    exports,
                    returns_to_runner,
                )
            {
                out.push(assertion);
            }
        }
        Statement::IfStatement(if_stmt) => match super::constant_condition_value(&if_stmt.test) {
            Some(false) => {
                if let Some(alternate) = &if_stmt.alternate {
                    collect_in_statement(alternate, source, exports, returns_to_runner, out);
                }
            }
            Some(true) => {
                collect_in_statement(&if_stmt.consequent, source, exports, returns_to_runner, out);
            }
            None => {
                collect_in_statement(&if_stmt.consequent, source, exports, returns_to_runner, out);
                if let Some(alternate) = &if_stmt.alternate {
                    collect_in_statement(alternate, source, exports, returns_to_runner, out);
                }
            }
        },
        Statement::ForStatement(for_stmt) => {
            let constantly_dead = for_stmt
                .test
                .as_ref()
                .and_then(super::constant_condition_value)
                == Some(false);
            if !constantly_dead {
                collect_in_statement(&for_stmt.body, source, exports, returns_to_runner, out);
            }
        }
        Statement::ForInStatement(for_in) => {
            collect_in_statement(&for_in.body, source, exports, returns_to_runner, out);
        }
        Statement::ForOfStatement(for_of) => {
            collect_in_statement(&for_of.body, source, exports, returns_to_runner, out);
        }
        Statement::WhileStatement(while_stmt)
            if super::constant_condition_value(&while_stmt.test) != Some(false) =>
        {
            collect_in_statement(&while_stmt.body, source, exports, returns_to_runner, out);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_in_statement(&do_while.body, source, exports, returns_to_runner, out);
        }
        Statement::SwitchStatement(switch_stmt) => {
            for case in &switch_stmt.cases {
                for inner in &case.consequent {
                    collect_in_statement(inner, source, exports, returns_to_runner, out);
                }
            }
        }
        Statement::TryStatement(try_stmt) => {
            for inner in &try_stmt.block.body {
                collect_in_statement(inner, source, exports, returns_to_runner, out);
            }
            if let Some(handler) = &try_stmt.handler {
                for inner in &handler.body.body {
                    collect_in_statement(inner, source, exports, returns_to_runner, out);
                }
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                for inner in &finalizer.body {
                    collect_in_statement(inner, source, exports, returns_to_runner, out);
                }
            }
        }
        _ => {}
    }
}

fn assertion_from_assert_property_call(
    expr: &Expression<'_>,
    source: &SourceText<'_>,
    exports: &FastCheckExports,
    returned_to_runner: bool,
) -> Option<TypeScriptAssertion> {
    let awaited = call_is_awaited(expr);
    let call_expr = match expr.get_inner_expression() {
        Expression::AwaitExpression(await_expr) => &await_expr.argument,
        _ => expr,
    };
    let call = call_expr.as_call_expression()?;
    if !callee_is_fast_check_assert(&call.callee, exports) {
        return None;
    }
    let property_arg = call.arguments.first()?;
    let property_call = argument_expression(property_arg)?.as_call_expression()?;
    let async_property = property_call_is_async(property_call, exports)?;
    if !async_property && !property_call_is_sync(property_call, exports)? {
        return None;
    }
    let predicate = property_call.arguments.last()?;
    let line = source.line_for_offset(call.span.start as usize);
    let rendered = source_text_span(source, call.span.start as usize, call.span.end as usize)
        .unwrap_or_default();
    if async_property && !returned_to_runner && !awaited {
        return None;
    }
    predicate_oracle(predicate, source, line, &rendered, async_property)
}

fn property_call_is_sync(
    call: &oxc_ast::ast::CallExpression<'_>,
    exports: &FastCheckExports,
) -> Option<bool> {
    match call.callee.get_inner_expression() {
        Expression::StaticMemberExpression(member)
            if member.property.name.as_str() == "property" =>
        {
            let Expression::Identifier(identifier) = member.object.get_inner_expression() else {
                return Some(false);
            };
            Some(
                exports.resolves_namespace_member(identifier.name.as_str(), "property")
                    || exports.is_property(identifier.name.as_str(), false),
            )
        }
        Expression::Identifier(identifier) => {
            Some(exports.is_property(identifier.name.as_str(), false))
        }
        _ => Some(false),
    }
}

fn property_call_is_async(
    call: &oxc_ast::ast::CallExpression<'_>,
    exports: &FastCheckExports,
) -> Option<bool> {
    match call.callee.get_inner_expression() {
        Expression::StaticMemberExpression(member)
            if member.property.name.as_str() == "asyncProperty" =>
        {
            let Expression::Identifier(identifier) = member.object.get_inner_expression() else {
                return Some(false);
            };
            Some(
                exports.resolves_namespace_member(identifier.name.as_str(), "asyncProperty")
                    || exports.is_property(identifier.name.as_str(), true),
            )
        }
        Expression::Identifier(identifier) => {
            Some(exports.is_property(identifier.name.as_str(), true))
        }
        _ => Some(false),
    }
}

fn callee_is_fast_check_assert(callee: &Expression<'_>, exports: &FastCheckExports) -> bool {
    match callee.get_inner_expression() {
        Expression::StaticMemberExpression(member) if member.property.name.as_str() == "assert" => {
            let Expression::Identifier(identifier) = member.object.get_inner_expression() else {
                return false;
            };
            exports.resolves_namespace_member(identifier.name.as_str(), "assert")
        }
        Expression::Identifier(identifier) => exports.is_assert(identifier.name.as_str()),
        _ => false,
    }
}

fn call_is_awaited(expr: &Expression<'_>) -> bool {
    matches!(expr.get_inner_expression(), Expression::AwaitExpression(_))
}

fn predicate_oracle(
    predicate: &Argument<'_>,
    source: &SourceText<'_>,
    line: usize,
    rendered: &str,
    async_property: bool,
) -> Option<TypeScriptAssertion> {
    match predicate {
        Argument::ArrowFunctionExpression(arrow) => {
            if arrow.expression {
                let Statement::ExpressionStatement(stmt) = &arrow.body.statements[0] else {
                    return None;
                };
                oracle_from_expression(&stmt.expression, source, line, rendered)
            } else {
                oracle_from_block(&arrow.body.statements, source, line, rendered)
            }
        }
        Argument::FunctionExpression(function) => {
            let body = function.body.as_ref()?;
            oracle_from_block(&body.statements, source, line, rendered)
        }
        _ => None,
    }
    .map(|mut assertion| {
        if async_property {
            assertion.matcher = "fast-check:async-property-predicate".to_string();
        }
        assertion
    })
}

fn oracle_from_block(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &SourceText<'_>,
    line: usize,
    rendered: &str,
) -> Option<TypeScriptAssertion> {
    for stmt in statements {
        if let Some(assertion) = oracle_from_predicate_statement(stmt, source, line, rendered) {
            return Some(assertion);
        }
    }
    None
}

fn oracle_from_predicate_statement(
    stmt: &Statement<'_>,
    source: &SourceText<'_>,
    line: usize,
    rendered: &str,
) -> Option<TypeScriptAssertion> {
    match stmt {
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .and_then(|argument| oracle_from_expression(argument, source, line, rendered)),
        Statement::ThrowStatement(throw_stmt) => {
            let Expression::NewExpression(new_expr) = throw_stmt.argument.get_inner_expression()
            else {
                return None;
            };
            throws_new_error_from_new(new_expr).then(|| throw_oracle(line, rendered))
        }
        Statement::IfStatement(if_stmt) => {
            oracle_from_predicate_statement(&if_stmt.consequent, source, line, rendered).or_else(
                || {
                    if_stmt.alternate.as_ref().and_then(|alternate| {
                        oracle_from_predicate_statement(alternate, source, line, rendered)
                    })
                },
            )
        }
        Statement::BlockStatement(block) => oracle_from_block(&block.body, source, line, rendered),
        _ => None,
    }
}

fn throw_oracle(line: usize, rendered: &str) -> TypeScriptAssertion {
    TypeScriptAssertion {
        matcher: "fast-check:property-throw".to_string(),
        argument_count: 0,
        line,
        oracle_kind: OracleKind::BroadError,
        oracle_strength: OracleStrength::Weak,
        mock_payload: None,
        error_payload: None,
        observed_expression: None,
        expected_value_or_variant: None,
        has_dynamic_matcher_arg: false,
        dynamic_expected_text: None,
        oracle_confidence: OracleConfidence::Low,
        rendered_call: Some(rendered.to_string()),
        loose_number_expected: false,
        compares_reference_identity: false,
        expected_object_const: None,
    }
}

fn oracle_from_expression(
    expr: &Expression<'_>,
    source: &SourceText<'_>,
    line: usize,
    rendered: &str,
) -> Option<TypeScriptAssertion> {
    match expr.get_inner_expression() {
        Expression::BinaryExpression(binary)
            if matches!(
                binary.operator,
                oxc_ast::ast::BinaryOperator::StrictEquality
                    | oxc_ast::ast::BinaryOperator::StrictInequality
                    | oxc_ast::ast::BinaryOperator::Equality
                    | oxc_ast::ast::BinaryOperator::Inequality
            ) =>
        {
            let observed = source_text_span(
                source,
                binary.left.span().start as usize,
                binary.left.span().end as usize,
            )
            .or_else(|| {
                source_text_span(
                    source,
                    binary.right.span().start as usize,
                    binary.right.span().end as usize,
                )
            });
            Some(TypeScriptAssertion {
                matcher: "fast-check:property-return".to_string(),
                argument_count: 2,
                line,
                oracle_kind: OracleKind::RelationalCheck,
                oracle_strength: OracleStrength::Weak,
                mock_payload: None,
                error_payload: None,
                observed_expression: observed,
                expected_value_or_variant: None,
                has_dynamic_matcher_arg: true,
                dynamic_expected_text: source_text_span(
                    source,
                    binary.right.span().start as usize,
                    binary.right.span().end as usize,
                ),
                oracle_confidence: OracleConfidence::Low,
                rendered_call: Some(rendered.to_string()),
                loose_number_expected: false,
                compares_reference_identity: matches!(
                    binary.operator,
                    oxc_ast::ast::BinaryOperator::StrictEquality
                        | oxc_ast::ast::BinaryOperator::StrictInequality
                ),
                expected_object_const: None,
            })
        }
        Expression::UnaryExpression(unary)
            if matches!(unary.operator, oxc_ast::ast::UnaryOperator::LogicalNot) =>
        {
            oracle_from_expression(&unary.argument, source, line, rendered)
        }
        Expression::CallExpression(call) if throws_new_error(call) => {
            Some(throw_oracle(line, rendered))
        }
        _ => None,
    }
}

fn throws_new_error(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    matches!(
        call.callee.get_inner_expression(),
        Expression::NewExpression(new_expr) if throws_new_error_from_new(new_expr)
    )
}

fn throws_new_error_from_new(new_expr: &oxc_ast::ast::NewExpression<'_>) -> bool {
    matches!(
        new_expr.callee.get_inner_expression(),
        Expression::Identifier(id) if id.name.as_str() == "Error"
    )
}

fn source_text_span(source: &SourceText<'_>, start: usize, end: usize) -> Option<String> {
    source.get(start..end).map(|text| text.to_string())
}

fn argument_expression<'a>(argument: &'a Argument<'a>) -> Option<&'a Expression<'a>> {
    match argument {
        Argument::SpreadElement(_) => None,
        _ => Some(argument.as_expression()?),
    }
}

trait ExpressionExt {
    fn as_call_expression(&self) -> Option<&oxc_ast::ast::CallExpression<'_>>;
}

impl ExpressionExt for Expression<'_> {
    fn as_call_expression(&self) -> Option<&oxc_ast::ast::CallExpression<'_>> {
        match self.get_inner_expression() {
            Expression::CallExpression(call) => Some(call),
            _ => None,
        }
    }
}
