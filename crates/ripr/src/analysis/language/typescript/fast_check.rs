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
    out: &mut Vec<TypeScriptAssertion>,
) {
    let exports = FastCheckExports::from_bindings(bindings);
    if exports.is_empty() {
        return;
    }
    for stmt in statements {
        collect_in_statement(stmt, source, &exports, out);
    }
}

fn collect_in_statement(
    stmt: &Statement<'_>,
    source: &SourceText<'_>,
    exports: &FastCheckExports,
    out: &mut Vec<TypeScriptAssertion>,
) {
    match stmt {
        Statement::BlockStatement(block) => {
            for inner in &block.body {
                collect_in_statement(inner, source, exports, out);
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            if let Some(assertion) =
                assertion_from_assert_property_call(&expr_stmt.expression, source, exports, false)
            {
                out.push(assertion);
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(argument) = &ret.argument
                && let Some(assertion) =
                    assertion_from_assert_property_call(argument, source, exports, true)
            {
                out.push(assertion);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_in_statement(&if_stmt.consequent, source, exports, out);
            if let Some(alternate) = &if_stmt.alternate {
                collect_in_statement(alternate, source, exports, out);
            }
        }
        Statement::ForStatement(for_stmt) => {
            collect_in_statement(&for_stmt.body, source, exports, out);
        }
        Statement::ForInStatement(for_in) => {
            collect_in_statement(&for_in.body, source, exports, out);
        }
        Statement::ForOfStatement(for_of) => {
            collect_in_statement(&for_of.body, source, exports, out);
        }
        Statement::WhileStatement(while_stmt) => {
            collect_in_statement(&while_stmt.body, source, exports, out);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_in_statement(&do_while.body, source, exports, out);
        }
        Statement::SwitchStatement(switch_stmt) => {
            for case in &switch_stmt.cases {
                for inner in &case.consequent {
                    collect_in_statement(inner, source, exports, out);
                }
            }
        }
        Statement::TryStatement(try_stmt) => {
            for inner in &try_stmt.block.body {
                collect_in_statement(inner, source, exports, out);
            }
            if let Some(handler) = &try_stmt.handler {
                for inner in &handler.body.body {
                    collect_in_statement(inner, source, exports, out);
                }
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                for inner in &finalizer.body {
                    collect_in_statement(inner, source, exports, out);
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
    let call = expr.as_call_expression()?;
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
    if async_property && !returned_to_runner && !call_is_awaited(expr) {
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
        if let Statement::ReturnStatement(ret) = stmt
            && let Some(argument) = &ret.argument
        {
            return oracle_from_expression(argument, source, line, rendered);
        }
    }
    None
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
        Expression::CallExpression(call) if throws_new_error(call) => Some(TypeScriptAssertion {
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
        }),
        _ => None,
    }
}

fn throws_new_error(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    matches!(
        call.callee.get_inner_expression(),
        Expression::NewExpression(new_expr)
            if matches!(
                new_expr.callee.get_inner_expression(),
                Expression::Identifier(id) if id.name.as_str() == "Error"
            )
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
