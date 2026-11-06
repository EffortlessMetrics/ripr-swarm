//! Bounded Vitest `test.for` whole-row semantics (#10760).
//!
//! This feeds the existing assertion producer, never the classifier. Unsupported
//! rows, bindings, context uses and expansion budgets remain extraction gaps.

use super::*;
use oxc_ast_visit::Visit;

const MAX_FOR_ROWS: usize = 64;
const MAX_FOR_CELLS: usize = 64;

/// One synchronous inline callback, read with each literal row. Array rows bind
/// parameter zero whole; only parameter one can be the Vitest context.
pub(super) fn assertions(
    call: &oxc_ast::ast::CallExpression<'_>,
    table_call: &oxc_ast::ast::CallExpression<'_>,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let [Argument::ArrayExpression(table)] = table_call.arguments.as_slice() else {
        return None;
    };
    if table.elements.is_empty() || table.elements.len() > MAX_FOR_ROWS {
        return None;
    }
    let text = source.text();
    if !row_text_is_inert(text.get(call.span.start as usize..call.span.end as usize)?) {
        return None;
    }
    let callback = call
        .arguments
        .get(super::declaration_callback_index(call))?;
    let (params, span, body) = match callback {
        Argument::ArrowFunctionExpression(arrow) if !arrow.r#async => {
            (&arrow.params, arrow.span, &*arrow.body)
        }
        Argument::FunctionExpression(function) if !function.r#async && !function.generator => {
            (&function.params, function.span, &**function.body.as_ref()?)
        }
        _ => return None,
    };
    if params.rest.is_some()
        || params.items.len() > 2
        || params.items.iter().any(|p| p.initializer.is_some())
    {
        return None;
    }
    let body_text = text.get(body.span.start as usize..body.span.end as usize)?;
    if !body_text.is_ascii() {
        return None;
    }
    let names = super::argument_parameter_names(callback);
    let writes = super::strict_identifier_writes(body_text);
    if names.iter().any(|name| {
        !name.is_ascii()
            || local_identifier_declared_in_test_body(body_text, name)
            || writes.iter().any(|(written, _)| written == name)
    }) {
        return None;
    }
    let mut bindings = bindings.without_shadowed(|name| {
        names.iter().any(|param| param == name)
            || local_identifier_declared_in_test_body(body_text, name)
            || writes.iter().any(|(written, _)| written == name)
    });
    let context = match params.items.get(1).map(|p| &p.pattern) {
        None => None,
        Some(BindingPattern::BindingIdentifier(id)) => Some(id.name.as_str()),
        Some(BindingPattern::ObjectPattern(object))
            if object.rest.is_none() && object.properties.len() == 1 =>
        {
            let property = &object.properties[0];
            if property.computed
                || property.key.static_name().as_deref() != Some("expect")
                || !matches!(&property.value, BindingPattern::BindingIdentifier(id) if id.name == "expect")
            {
                return None;
            }
            if local_identifier_declared_in_test_body(body_text, "expect")
                || writes.iter().any(|(name, _)| name == "expect")
            {
                return None;
            }
            bindings = bindings.with_context_expect();
            None
        }
        _ => return None,
    };
    // `ctx.expect` is lowered only after every use of ctx has been checked.
    // A body-local expect must never receive the lowered call's authority.
    if context.is_some()
        && (names.iter().any(|name| name == "expect")
            || local_identifier_declared_in_test_body(body_text, "expect")
            || writes.iter().any(|(name, _)| name == "expect"))
    {
        return None;
    }
    let mut result = Vec::new();
    for element in &table.elements {
        let row = element.as_expression()?;
        let mut locals = Vec::new();
        let mut cells = Vec::new();
        let mut indexed = None;
        match (params.items.first().map(|p| &p.pattern), row) {
            (None, _) => {}
            (Some(BindingPattern::ArrayPattern(pattern)), Expression::ArrayExpression(array))
                if pattern.rest.is_none() =>
            {
                for (index, item) in pattern.elements.iter().enumerate() {
                    let BindingPattern::BindingIdentifier(id) = item.as_ref()? else {
                        return None;
                    };
                    locals.push(id.name.to_string());
                    cells.push(literal(array.elements.get(index)?.as_expression()?, text)?);
                }
            }
            (Some(BindingPattern::BindingIdentifier(id)), Expression::ArrayExpression(array)) => {
                let values = array
                    .elements
                    .iter()
                    .map(|cell| literal(cell.as_expression()?, text))
                    .collect::<Option<Vec<_>>>()?;
                for (index, cell) in values.into_iter().enumerate() {
                    let local = format!("__ripr_for_cell_{index}");
                    if body_text.contains(&local) {
                        return None;
                    }
                    locals.push(local);
                    cells.push(cell);
                }
                indexed = Some(id.name.as_str());
            }
            (Some(BindingPattern::BindingIdentifier(id)), scalar) => {
                locals.push(id.name.to_string());
                cells.push(literal(scalar, text)?);
            }
            _ => return None,
        }
        // Validate all row cells, including cells ignored by destructuring or
        // a parameterless callback. Object/spread/dynamic rows stay limited.
        match row {
            Expression::ArrayExpression(array) => {
                if array.elements.len() > MAX_FOR_CELLS {
                    return None;
                }
                for cell in &array.elements {
                    literal(cell.as_expression()?, text)?;
                }
            }
            scalar => {
                literal(scalar, text)?;
            }
        }
        let mut normalizer = Reads {
            source: text,
            indexed,
            context,
            cells: &locals,
            replacements: Vec::new(),
            valid: true,
            context_expect: false,
            bare_expect: false,
        };
        normalizer.visit_function_body(body);
        // Context lowering must not lend its authority to another binding's
        // bare `expect`. Mixed spellings remain partial rather than merging
        // those two identities in one assertion-binding environment.
        if !normalizer.valid || (normalizer.context_expect && normalizer.bare_expect) {
            return None;
        }
        let mut normalized = body_text.to_string();
        normalizer
            .replacements
            .sort_unstable_by_key(|(start, _, _)| *start);
        for (start, end, replacement) in normalizer.replacements.into_iter().rev() {
            normalized.replace_range(
                start - body.span.start as usize..end - body.span.start as usize,
                &replacement,
            );
        }
        if indexed.is_some() && !super::super::object_rows::single_expect_statement(&normalized) {
            return None;
        }
        let bindings = if normalizer.context_expect {
            bindings.clone().with_context_expect()
        } else {
            bindings.clone()
        };
        let substituted = substitute_row(&normalized, &locals, &cells)?;
        let callback_text = format!(
            "{}{}{}",
            text.get(span.start as usize..body.span.start as usize)?,
            substituted,
            text.get(body.span.end as usize..span.end as usize)?
        );
        result.extend(reparse_row_assertions(
            text,
            span.start as usize,
            &callback_text,
            &bindings,
            true,
        )?);
    }
    Some(result)
}

fn literal(expression: &Expression<'_>, source: &str) -> Option<String> {
    if !is_literal_cell(expression) || matches!(expression, Expression::Identifier(_)) {
        return None;
    }
    let span = expression.span();
    let value = source.get(span.start as usize..span.end as usize)?;
    (!value.contains(['\n', '\r'])).then(|| value.to_string())
}

/// AST-qualified member reads become scalar substitution slots. Every remaining
/// mention of the indexed row or context fails closed (aliases, skip, writes,
/// nested shadows, optional/computed context members and escaped references).
struct Reads<'s> {
    source: &'s str,
    indexed: Option<&'s str>,
    context: Option<&'s str>,
    cells: &'s [String],
    replacements: Vec<(usize, usize, String)>,
    valid: bool,
    context_expect: bool,
    bare_expect: bool,
}

impl<'a> Visit<'a> for Reads<'_> {
    fn visit_unary_expression(&mut self, unary: &oxc_ast::ast::UnaryExpression<'a>) {
        if unary.operator == oxc_ast::ast::UnaryOperator::Delete {
            self.valid = false;
        }
        oxc_ast_visit::walk::walk_unary_expression(self, unary);
    }
    fn visit_identifier_reference(&mut self, id: &oxc_ast::ast::IdentifierReference<'a>) {
        self.bare_expect |= id.name == "expect";
        if self.indexed == Some(id.name.as_str()) || self.context == Some(id.name.as_str()) {
            self.valid = false;
        }
    }
    fn visit_computed_member_expression(
        &mut self,
        member: &oxc_ast::ast::ComputedMemberExpression<'a>,
    ) {
        if matches!(&member.object, Expression::Identifier(id) if self.indexed == Some(id.name.as_str()))
        {
            let Expression::NumericLiteral(index) = &member.expression else {
                self.valid = false;
                return;
            };
            let index_text = self
                .source
                .get(index.span.start as usize..index.span.end as usize)
                .unwrap_or_default();
            let Some(cell) = index_text
                .parse::<usize>()
                .ok()
                .and_then(|i| self.cells.get(i))
            else {
                self.valid = false;
                return;
            };
            self.valid &= !member.optional;
            self.replacements.push((
                member.span.start as usize,
                member.span.end as usize,
                cell.clone(),
            ));
        } else {
            oxc_ast_visit::walk::walk_computed_member_expression(self, member);
        }
    }
    fn visit_static_member_expression(
        &mut self,
        member: &oxc_ast::ast::StaticMemberExpression<'a>,
    ) {
        if matches!(&member.object, Expression::Identifier(id) if self.context == Some(id.name.as_str()))
        {
            let after = self
                .source
                .get(member.span.end as usize..)
                .unwrap_or_default()
                .trim_start();
            if member.optional || member.property.name != "expect" || !after.starts_with('(') {
                self.valid = false;
                return;
            }
            self.context_expect = true;
            self.replacements.push((
                member.span.start as usize,
                member.span.end as usize,
                "expect".to_string(),
            ));
        } else {
            oxc_ast_visit::walk::walk_static_member_expression(self, member);
        }
    }
}
