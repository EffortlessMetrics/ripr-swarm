//! Bounded node-tap callback assertion authority (#9187).
//!
//! Registration alone does not establish that a matcher can fail the test:
//! tap requires completion, permits skipped assertions, and has a different
//! method vocabulary from AVA. Keep that profile separate from generic t.*.

use super::*;
use oxc_ast_visit::{Visit, walk};

/// Inline callbacks with an untouched context, two-argument equality calls,
/// and either a returned async lifetime or one final synchronous `t.end()`.
/// Everything else keeps its test relation but receives no oracle credit.
pub(super) fn callback_has_assertion_authority(
    callback: &Argument<'_>,
    receiver: Option<&str>,
    source: &SourceText<'_>,
) -> bool {
    let Some(receiver) = receiver else {
        return false;
    };
    let (statements, asynchronous) = match callback {
        Argument::ArrowFunctionExpression(arrow) => (&arrow.body.statements, arrow.r#async),
        Argument::FunctionExpression(function) if !function.generator => {
            let Some(body) = &function.body else {
                return false;
            };
            (&body.statements, function.r#async)
        }
        _ => return false,
    };
    let (Some(first), Some(last)) = (statements.first(), statements.last()) else {
        return false;
    };
    let Some(text) = source.get(first.span().start as usize..last.span().end as usize) else {
        return false;
    };
    let masked = super::tests_extract::mask_comments_and_strings_text(text);
    if super::tests_extract::file_is_opaque(&masked) {
        return false;
    }
    let mut calls = ContextCalls {
        receiver,
        uses: 0,
        count: 0,
        ends: 0,
        supported: true,
    };
    for statement in statements {
        calls.visit_statement(statement);
    }
    // Every use must be a supported method call. Rebinding, aliasing,
    // matcher replacement and shadow declarations all add another use.
    if !calls.supported || calls.uses != calls.count {
        return false;
    }
    if calls.ends == 0 {
        return asynchronous;
    }
    calls.ends == 1
        && matches!(last, Statement::ExpressionStatement(statement)
            if matches!(statement.expression.get_inner_expression(), Expression::CallExpression(call)
                if is_context_end(call, receiver)))
}

struct ContextCalls<'s> {
    receiver: &'s str,
    uses: usize,
    count: usize,
    ends: usize,
    supported: bool,
}

impl<'a> Visit<'a> for ContextCalls<'_> {
    fn visit_identifier_reference(&mut self, id: &oxc_ast::ast::IdentifierReference<'a>) {
        self.uses += usize::from(id.name == self.receiver);
    }

    fn visit_binding_identifier(&mut self, id: &oxc_ast::ast::BindingIdentifier<'a>) {
        self.uses += usize::from(id.name == self.receiver);
    }

    fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        if let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
            && matches!(member.object.get_inner_expression(), Expression::Identifier(id)
                if id.name == self.receiver)
        {
            self.count += 1;
            let method = member.property.name.as_str();
            if method == "end" {
                self.ends += 1;
            }
            self.supported &= !call.optional
                && !member.optional
                && match method {
                    "equal" | "strictEqual" => {
                        call.arguments.len() == 2
                            && call
                                .arguments
                                .iter()
                                .all(|arg| !matches!(arg, Argument::SpreadElement(_)))
                    }
                    "end" => call.arguments.is_empty(),
                    _ => false,
                };
        }
        walk::walk_call_expression(self, call);
    }
}

fn is_context_end(call: &oxc_ast::ast::CallExpression<'_>, receiver: &str) -> bool {
    call.arguments.is_empty()
        && matches!(call.callee.get_inner_expression(), Expression::StaticMemberExpression(member)
            if member.property.name == "end"
                && matches!(member.object.get_inner_expression(), Expression::Identifier(id)
                    if id.name == receiver))
}
