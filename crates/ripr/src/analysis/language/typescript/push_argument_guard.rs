//! A length pin cannot distinguish the value appended by a single push.
use super::*;
use std::collections::HashMap;

struct PushTestWitness {
    binding: String,
    input: String,
    // Keyed by the extractor's original literal spelling, values are decoded
    // by the same parser used for the input string.
    expected_values: HashMap<String, String>,
}

#[derive(Default)]
pub(crate) struct PushArgumentFacts {
    tests: HashMap<*const TypeScriptTest, PushTestWitness>,
}

impl PushArgumentFacts {
    pub(crate) fn read(
        old: Option<&str>,
        owner: &TypeScriptOwner,
        candidates: &[TypeScriptRelatedCandidate<'_>],
    ) -> Self {
        let Some(old) = old else {
            return Self::default();
        };
        let Some(source) = owner.source_text.as_deref() else {
            return Self::default();
        };
        // Avoid parsing the owner for unrelated side-effect changes.
        if !old.contains("push") || !source.contains("toUpperCase") {
            return Self::default();
        }
        let source = source
            .trim()
            .strip_prefix("export ")
            .unwrap_or(source.trim());
        let old = old.trim().to_owned();
        let parameters = owner.params.clone();
        if owner.arity != Some(2) || parameters.len() != 2 {
            return Self::default();
        }
        let positions =
            super::parse::parse_on_worker(&owner.file, source, move |_, source, allocator| {
                let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
                if !parsed.errors.is_empty() || parsed.program.body.len() != 1 {
                    return None;
                }
                let Statement::FunctionDeclaration(function) = &parsed.program.body[0] else {
                    return None;
                };
                if function.r#async
                    || function.generator
                    || function
                        .params
                        .items
                        .iter()
                        .any(|p| p.initializer.is_some())
                {
                    return None;
                }
                let body = function.body.as_ref()?;
                let [statement] = body.statements.as_slice() else {
                    return None;
                };
                let new = statement_call(statement)?;
                let old_parsed = Parser::new(allocator, &old, SourceType::ts()).parse();
                if !old_parsed.errors.is_empty() || old_parsed.program.body.len() != 1 {
                    return None;
                }
                let old = statement_call(&old_parsed.program.body[0])?;
                let (old_receiver, old_argument) = push(old)?;
                let (new_receiver, new_argument) = push(new)?;
                if old_receiver != new_receiver {
                    return None;
                }
                let Expression::Identifier(old_arg) = old_argument else {
                    return None;
                };
                let Expression::CallExpression(upper) = new_argument else {
                    return None;
                };
                if upper.optional || !upper.arguments.is_empty() {
                    return None;
                }
                let Expression::StaticMemberExpression(member) = &upper.callee else {
                    return None;
                };
                if member.optional
                    || member.property.name != "toUpperCase"
                    || identifier(&member.object) != Some(old_arg.name.as_str())
                {
                    return None;
                }
                Some((
                    parameters.iter().position(|p| p == old_receiver)?,
                    parameters.iter().position(|p| p == old_arg.name.as_str())?,
                ))
            })
            .ok()
            .flatten();
        let Some((receiver, value)) = positions else {
            return Self::default();
        };
        if receiver == value {
            return Self::default();
        }
        let tests = candidates
            .iter()
            .filter_map(|candidate| {
                if candidate.relation != TypeScriptRelationKind::DirectOwnerCall {
                    return None;
                }
                let test = candidate.test;
                if test
                    .member_replacements_in_file
                    .iter()
                    .any(|name| name == "*" || name == "push" || name == "toUpperCase")
                {
                    return None;
                }
                if ["expect", "test"].iter().any(|name| {
                    test.written_locals_in_file
                        .iter()
                        .any(|written| written == name)
                        || test
                            .scope_bindings
                            .iter()
                            .any(|binding| binding.name == *name && !binding.file_level)
                }) {
                    return None;
                }
                let framework = ["vitest", "@jest/globals"].into_iter().find(|framework| {
                    // Keep the registration and assertion in the same framework;
                    // independently trusted imports can still be a mixed pair.
                    ["expect", "test"].iter().all(|name| {
                        test.imports_in_file.iter().any(|import| {
                            import.source == *framework
                                && import.local == *name
                                && import.imported.as_deref() == Some(*name)
                        })
                    })
                })?;
                let witness = test_binding(
                    test,
                    &owner.name,
                    receiver,
                    value,
                    framework == "@jest/globals",
                )?;
                Some((test as *const TypeScriptTest, witness))
            })
            .collect();
        Self { tests }
    }

    /// Return true when this bounded witness cannot attribute the assertion to
    /// the appended value. Both rank and sink observation use this decision.
    pub(crate) fn withhold(&self, test: &TypeScriptTest, assertion: &TypeScriptAssertion) -> bool {
        let Some(witness) = self.tests.get(&(test as *const TypeScriptTest)) else {
            return false;
        };
        let observed = assertion
            .observed_expression
            .as_deref()
            .unwrap_or("")
            .trim();
        let expected = assertion
            .expected_value_or_variant
            .as_deref()
            .and_then(|spelling| witness.expected_values.get(spelling));
        let upper = witness.input.to_ascii_uppercase();
        observed != format!("{}[0]", witness.binding)
            || upper == witness.input
            || !expected.is_some_and(|value| value == &witness.input || value == &upper)
    }
}

fn statement_call<'s, 'a>(
    statement: &'s Statement<'a>,
) -> Option<&'s oxc_ast::ast::CallExpression<'a>> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    match &statement.expression {
        Expression::CallExpression(call) if !call.optional => Some(call),
        _ => None,
    }
}
fn identifier<'s, 'a>(expression: &'s Expression<'a>) -> Option<&'s str> {
    match expression {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}
fn push<'s, 'a>(
    call: &'s oxc_ast::ast::CallExpression<'a>,
) -> Option<(&'s str, &'s Expression<'a>)> {
    if call.optional || call.arguments.len() != 1 {
        return None;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.optional || member.property.name != "push" {
        return None;
    }
    Some((
        identifier(&member.object)?,
        call.arguments.first()?.as_expression()?,
    ))
}

fn test_binding(
    test: &TypeScriptTest,
    owner: &str,
    receiver: usize,
    value: usize,
    require_plain_registration: bool,
) -> Option<PushTestWitness> {
    let owner = owner.to_owned();
    super::parse::parse_on_worker(&test.file, &test.body_text, move |_, source, allocator| {
        let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
        if !parsed.errors.is_empty() || parsed.program.body.len() != 1 { return None }
        let registration = statement_call(&parsed.program.body[0])?;
        // The Jest extension is bounded to the imported test itself. An unused
        // import cannot authenticate an aliased or mixed-framework registration.
        // Preserve the existing Vitest registration readings.
        if require_plain_registration && identifier(&registration.callee) != Some("test") { return None }
        let Some(Argument::ArrowFunctionExpression(callback)) = registration.arguments.get(1) else { return None };
        if callback.r#async || callback.expression || !callback.params.items.is_empty() || callback.params.rest.is_some() { return None }
        let statements = callback.body.statements.as_slice();
        if statements.len() < 3 { return None }
        let Statement::VariableDeclaration(decl) = &statements[0] else { return None };
        if decl.kind != oxc_ast::ast::VariableDeclarationKind::Const || decl.declarations.len() != 1 { return None }
        let decl = &decl.declarations[0];
        let binding = super::owners::binding_identifier_name(&decl.id)?;
        if !matches!(&decl.init, Some(Expression::ArrayExpression(array)) if array.elements.is_empty()) { return None }
        let call = statement_call(&statements[1])?;
        if call.arguments.len() != 2 || identifier(&call.callee) != Some(owner.as_str()) || !matches!(call.arguments.get(receiver), Some(Argument::Identifier(id)) if id.name == binding) || !matches!(call.arguments.get(value), Some(Argument::StringLiteral(_))) { return None }
        // All remaining statements must be plain expect(...).toBe(literal).
        // Preserve OXC-decoded strings with their original source spellings,
        // which are how extracted assertions identify matcher arguments.
        let mut expected_values = HashMap::new();
        for statement in &statements[2..] {
            let outer = statement_call(statement)?;
            let Expression::StaticMemberExpression(member) = &outer.callee else { return None };
            if member.optional || member.property.name != "toBe" || outer.arguments.len() != 1 || !matches!(outer.arguments.first(), Some(Argument::StringLiteral(_) | Argument::NumericLiteral(_))) { return None }
            if let Some(Argument::StringLiteral(literal)) = outer.arguments.first() {
                let span = literal.span();
                let spelling = source.get(span.start as usize..span.end as usize)?.trim();
                expected_values.insert(spelling.to_owned(), literal.value.to_string());
            }
            let Expression::CallExpression(expect) = &member.object else { return None };
            if expect.optional || identifier(&expect.callee) != Some("expect") || expect.arguments.len() != 1 { return None }
            // No assertion may evaluate a call or mutate state before another assertion.
            let inert = match expect.arguments.first() {
                Some(Argument::NumericLiteral(_) | Argument::StringLiteral(_)) => true,
                Some(Argument::StaticMemberExpression(member)) => !member.optional && identifier(&member.object) == Some(binding) && member.property.name == "length",
                Some(Argument::ComputedMemberExpression(member)) => !member.optional && identifier(&member.object) == Some(binding) && matches!(&member.expression, Expression::NumericLiteral(_)),
                _ => false,
            };
            if !inert { return None }
        }
        let Some(Argument::StringLiteral(input)) = call.arguments.get(value) else { return None };
        if !input.value.is_ascii() { return None }
        Some(PushTestWitness { binding: binding.to_owned(), input: input.value.to_string(), expected_values })
    }).ok().flatten()
}
