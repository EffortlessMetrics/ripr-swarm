//! Counted call removal (#10157). Identity and execution order are a single
//! bounded witness; unsupported statements withhold this additional credit.

use super::*;

/// Read a whole removed method-call statement and its replacement discard.
pub(crate) fn removed_method_call(new: &str, old: Option<&str>) -> Option<(String, String)> {
    let new_len = new.len();
    let combined = format!("{new}\n{}", old?);
    super::parse::parse_on_worker(
        Path::new("removed-call.ts"),
        &combined,
        move |_, source, allocator| {
            let new = Parser::new(allocator, &source[..new_len], SourceType::ts()).parse();
            let old = Parser::new(allocator, &source[new_len + 1..], SourceType::ts()).parse();
            if !new.errors.is_empty()
                || !old.errors.is_empty()
                || new.program.body.len() != 1
                || old.program.body.len() != 1
            {
                return None;
            }
            let new = &new.program.body[0];
            let old = &old.program.body[0];
            let Statement::ExpressionStatement(discard) = new else {
                return None;
            };
            let Expression::UnaryExpression(unary) = &discard.expression else {
                return None;
            };
            if unary.operator != oxc_ast::ast::UnaryOperator::Void
                || !matches!(&unary.argument, Expression::Identifier(_))
            {
                return None;
            }
            let call = statement_call(old)?;
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return None;
            };
            let Expression::Identifier(receiver) = &member.object else {
                return None;
            };
            if call.optional || member.optional {
                return None;
            }
            Some((receiver.name.to_string(), member.property.name.to_string()))
        },
    )
    .ok()
    .flatten()
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

fn imported(test: &TypeScriptTest, local: &str, module: &str, name: &str) -> bool {
    !shadowed(test, local)
        && !test
            .written_locals_in_file
            .iter()
            .any(|written| written == local)
        && test.imports_in_file.iter().any(|import| {
            !import.namespace
                && import.local == local
                && import.source == module
                && import.imported.as_deref() == Some(name)
        })
}

/// Require a direct owner import and one inline zero-parameter callback.
/// Aliases, context mocks, async callbacks, resets/restores and nested control
/// flow are intentionally outside this witness.
pub(crate) fn call_count_pins_removed_call(
    receiver: &str,
    method: &str,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    if owner.arity != Some(owner.params.len()) || !straight_line_owner(owner, receiver) {
        return false;
    }
    if !module_bindings_are_exported(owner, receiver, workspace_root) {
        return false;
    }
    candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            let test = candidate.test;
            if shadowed(test, &owner.name)
                || test
                    .written_locals_in_file
                    .iter()
                    .any(|local| local == &owner.name)
                || !imported(test, "mock", "node:test", "mock")
                || !imported(test, "test", "node:test", "test")
                || !test.imports_in_file.iter().any(|import| {
                    import.local == owner.name
                        && import.imported.as_deref() == Some(owner.name.as_str())
                        && super::related_tests::import_source_matches_owner(
                            import,
                            &test.file,
                            owner,
                            alias_map,
                            workspace_root,
                        )
                })
            {
                return false;
            }
            let Some(root) = workspace_root else {
                return false;
            };
            let Some(test_source) = admitted_source(root, &test.file) else {
                return false;
            };
            let owned_test = test.clone();
            let owned_owner = owner.clone();
            let receiver = receiver.to_string();
            let method = method.to_string();
            let alias_map = alias_map.cloned();
            let workspace_root = workspace_root.map(Path::to_path_buf);
            super::parse::parse_on_worker(&test.file, &test_source, move |_, source, allocator| {
                let test = &owned_test;
                let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
                if !parsed.errors.is_empty() {
                    return false;
                }
                let mut registration = None;
                for statement in &parsed.program.body {
                    if let Statement::ImportDeclaration(import) = statement {
                        let module = import.source.value.as_str();
                        if module != "node:test"
                            && module != "node:assert/strict"
                            && !test.imports_in_file.iter().any(|fact| {
                                fact.source == module
                                    && super::related_tests::import_source_matches_owner(
                                        fact,
                                        &test.file,
                                        &owned_owner,
                                        alias_map.as_ref(),
                                        workspace_root.as_deref(),
                                    )
                            })
                        {
                            return false;
                        }
                        continue;
                    }
                    let Some(call) = statement_call(statement) else {
                        return false;
                    };
                    let span = call.span();
                    if registration.is_some()
                        || source.get(span.start as usize..span.end as usize)
                            != Some(test.body_text.as_str())
                    {
                        return false;
                    }
                    registration = Some(call);
                }
                let Some(registration) = registration else {
                    return false;
                };
                if identifier(&registration.callee) != Some("test")
                    || registration.arguments.len() != 2
                    || !matches!(
                        registration.arguments.first(),
                        Some(Argument::StringLiteral(_))
                    )
                {
                    return false;
                }
                let Some(Argument::ArrowFunctionExpression(callback)) =
                    registration.arguments.get(1)
                else {
                    return false;
                };
                if callback.r#async
                    || callback.expression
                    || !callback.params.items.is_empty()
                    || callback.params.rest.is_some()
                {
                    return false;
                }
                sequence_pins(
                    &callback.body.statements,
                    source,
                    test,
                    (&receiver, &method),
                    &owned_owner,
                    alias_map.as_ref(),
                    workspace_root.as_deref(),
                )
            })
            .unwrap_or(false)
        })
}

/// No executable statement can sit between registration, invocation and count.
/// Const object declarations are allowed only before registration. This makes
/// rebinding, receiver escape, direct calls, restore/reset and conditional spy
/// installation refusals rather than guesses about value identity.
fn sequence_pins(
    statements: &[Statement<'_>],
    source: &str,
    test: &TypeScriptTest,
    removed: (&str, &str),
    owner: &TypeScriptOwner,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    let (receiver, method) = removed;
    let mut locals = Vec::new();
    let mut spy: Option<(String, String)> = None;
    let mut invoked = false;
    for (index, statement) in statements.iter().enumerate() {
        if let Statement::VariableDeclaration(declaration) = statement {
            if spy.is_some()
                || declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const
                || declaration.declarations.len() != 1
            {
                return false;
            }
            let declaration = &declaration.declarations[0];
            let Some(local) = super::owners::binding_identifier_name(&declaration.id) else {
                return false;
            };
            if locals.iter().any(|name| name == local)
                || ["mock", "assert", "test", owner.name.as_str()].contains(&local)
            {
                return false;
            }
            let Some(init) = &declaration.init else {
                return false;
            };
            match init {
                Expression::ObjectExpression(object) => {
                    if !plain_object(object) {
                        return false;
                    }
                    locals.push(local.to_string());
                }
                Expression::CallExpression(call) => {
                    let Expression::StaticMemberExpression(member) = &call.callee else {
                        return false;
                    };
                    if identifier(&member.object) != Some("mock")
                        || member.property.name != "method"
                        || member.optional
                        || call.optional
                        || !matches!(call.arguments.len(), 2 | 3)
                        || call
                            .arguments
                            .get(2)
                            .is_some_and(|argument| !noop_replacement(argument))
                    {
                        return false;
                    }
                    let Some(Argument::Identifier(target)) = call.arguments.first() else {
                        return false;
                    };
                    let Some(Argument::StringLiteral(key)) = call.arguments.get(1) else {
                        return false;
                    };
                    if key.value != method {
                        return false;
                    }
                    let target = target.name.as_str();
                    let parameter = owner.params.iter().any(|param| param == receiver);
                    if parameter {
                        if !locals.iter().any(|name| name == target) {
                            return false;
                        }
                    } else if target != receiver
                        || shadowed(test, receiver)
                        || locals.iter().any(|name| name == receiver)
                        || test
                            .written_locals_in_file
                            .iter()
                            .any(|name| name == receiver)
                        || !test.imports_in_file.iter().any(|import| {
                            !import.namespace
                                && import.local == receiver
                                && import.imported.as_deref() == Some(receiver)
                                && super::related_tests::import_source_matches_owner(
                                    import,
                                    &test.file,
                                    owner,
                                    alias_map,
                                    workspace_root,
                                )
                        })
                    {
                        return false;
                    }
                    spy = Some((local.to_string(), target.to_string()));
                }
                _ => return false,
            }
            continue;
        }
        let Some((binding, target)) = &spy else {
            return false;
        };
        let Some(call) = statement_call(statement) else {
            return false;
        };
        if !invoked {
            if identifier(&call.callee) != Some(owner.name.as_str()) {
                return false;
            }
            // No argument may execute code, mutate or expose the receiver.
            let receiver_position = owner.params.iter().position(|param| param == receiver);
            if call.arguments.len() != owner.params.len()
                || call
                    .arguments
                    .iter()
                    .enumerate()
                    .any(|(position, argument)| {
                        if Some(position) == receiver_position {
                            return false;
                        }
                        !matches!(
                            argument,
                            Argument::StringLiteral(_)
                                | Argument::NumericLiteral(_)
                                | Argument::BooleanLiteral(_)
                                | Argument::NullLiteral(_)
                        )
                    })
            {
                return false;
            }
            if let Some(position) = owner.params.iter().position(|param| param == receiver)
                && !matches!(call.arguments.get(position), Some(Argument::Identifier(id)) if id.name == target.as_str())
            {
                return false;
            }
            invoked = true;
            continue;
        }
        // The only statement after the owner must be the exact count pin.
        if index + 1 != statements.len()
            || !imported(test, "assert", "node:assert/strict", "default")
        {
            return false;
        }
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return false;
        };
        if identifier(&member.object) != Some("assert")
            || !matches!(member.property.name.as_str(), "equal" | "strictEqual")
            || call.arguments.len() != 2
        {
            return false;
        }
        let Some(observed) = call.arguments[0].as_expression() else {
            return false;
        };
        let span = observed.span();
        let Some(observed) = source.get(span.start as usize..span.end as usize) else {
            return false;
        };
        let observed: String = observed.split_whitespace().collect();
        if observed != format!("{binding}.mock.callCount()")
            && observed != format!("{binding}.mock.calls.length")
        {
            return false;
        }
        return matches!(&call.arguments[1], Argument::NumericLiteral(number) if number.value == 1.0)
            && test.assertions.iter().any(|assertion| {
                assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
                    && matches!(assertion.oracle_kind, OracleKind::ExactValue)
                    && assertion.expected_value_or_variant.as_deref() == Some("1")
                    && assertion
                        .observed_expression
                        .as_deref()
                        .is_some_and(|value| {
                            value.split_whitespace().collect::<String>() == observed
                        })
            });
    }
    false
}

/// Keep the removed call on an unconditional path without another call or
/// write that could replace its receiver before the deleted statement.
fn straight_line_owner(owner: &TypeScriptOwner, receiver: &str) -> bool {
    let Some(source) = owner.source_text.as_deref() else {
        return false;
    };
    let source = source
        .trim()
        .strip_prefix("export ")
        .unwrap_or(source.trim());
    let owned_owner = owner.clone();
    let receiver = receiver.to_string();
    super::parse::parse_on_worker(&owner.file, source, move |_, source, allocator| {
    let owner = &owned_owner;
    let receiver = receiver.as_str();
    let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
    if !parsed.errors.is_empty() || parsed.program.body.len() != 1 { return false; }
    let Statement::FunctionDeclaration(function) = &parsed.program.body[0] else { return false; };
    if function.r#async || function.generator || function.params.items.iter().any(|param| param.initializer.is_some()) { return false; }
    let Some(body) = &function.body else { return false; };
    let mut known = owner.params.clone();
    body.statements.iter().enumerate().all(|(index, statement)| match statement {
        Statement::VariableDeclaration(declaration) => declaration.declarations.iter().all(|declaration| {
            let Some(local) = super::owners::binding_identifier_name(&declaration.id) else { return false; };
            if local == receiver || known.iter().any(|name| name == local)
                || !declaration.init.as_ref().is_some_and(|value| pure_value(value, owner, receiver, &known)) { return false; }
            known.push(local.to_string());
            true
        }),
        Statement::ExpressionStatement(statement) => matches!(&statement.expression,
            Expression::UnaryExpression(unary) if unary.operator == oxc_ast::ast::UnaryOperator::Void
                && matches!(&unary.argument, Expression::Identifier(id) if known.iter().any(|name| name == id.name.as_str()))),
        Statement::ReturnStatement(statement) => index + 1 == body.statements.len() && statement.argument.as_ref().is_none_or(|value| pure_value(value, owner, receiver, &known)),
        _ => false,
    })
    }).unwrap_or(false)
}

fn pure_value(
    expression: &Expression<'_>,
    owner: &TypeScriptOwner,
    receiver: &str,
    known: &[String],
) -> bool {
    match expression {
        Expression::Identifier(id) => known.iter().any(|name| name == id.name.as_str()),
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(template) => {
            template.expressions.iter().all(|value| match value {
                Expression::Identifier(id) => {
                    id.name != receiver
                        && owner.params.iter().any(|param| param == id.name.as_str())
                }
                Expression::StringLiteral(_)
                | Expression::NumericLiteral(_)
                | Expression::BooleanLiteral(_)
                | Expression::NullLiteral(_) => true,
                _ => false,
            })
        }
        _ => false,
    }
}

fn shadowed(test: &TypeScriptTest, local: &str) -> bool {
    test.scope_bindings
        .iter()
        .any(|binding| binding.name == local && !binding.file_level)
}

/// A same-module export name can alias another local. Require a direct const
/// declaration of this binding, reading the admitted source/snapshot under the
/// existing source caps. Re-exports and renamed exports withhold this witness.
fn module_bindings_are_exported(
    owner: &TypeScriptOwner,
    receiver: &str,
    root: Option<&Path>,
) -> bool {
    let Some(root) = root else {
        return false;
    };
    let Some(source) = admitted_source(root, &owner.file) else {
        return false;
    };
    if !owner
        .source_text
        .as_deref()
        .is_some_and(|text| source.contains(text))
    {
        return false;
    }
    let require_receiver = !owner.params.iter().any(|param| param == receiver);
    let owner_name = owner.name.clone();
    let owner_source = owner.source_text.clone().unwrap_or_default();
    let receiver = receiver.to_string();
    super::parse::parse_on_worker(&owner.file, &source, move |_, source, allocator| {
        let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
        if !parsed.errors.is_empty() {
            return false;
        }
        let mut owner_exported = false;
        let mut receiver_exported = !require_receiver;
        for statement in &parsed.program.body {
            let Statement::ExportNamedDeclaration(export) = statement else {
                return false;
            };
            match &export.declaration {
                Some(Declaration::FunctionDeclaration(function)) => {
                    let span = function.span();
                    if function.id.as_ref().is_none_or(|id| id.name != owner_name.as_str()) { return false; }
                    owner_exported |= function
                        .id
                        .as_ref()
                        .is_some_and(|id| id.name == owner_name.as_str())
                        && source.get(span.start as usize..span.end as usize)
                            == Some(
                                owner_source
                                    .trim()
                                    .strip_prefix("export ")
                                    .unwrap_or(owner_source.trim()),
                            );
                }
                Some(Declaration::VariableDeclaration(declaration))
                    if declaration.kind == oxc_ast::ast::VariableDeclarationKind::Const =>
                {
                    if declaration.declarations.len() != 1 { return false; }
                    let exported_receiver = declaration.declarations.iter().any(|declaration| {
                        super::owners::binding_identifier_name(&declaration.id)
                            == Some(receiver.as_str())
                            && matches!(&declaration.init, Some(Expression::ObjectExpression(object)) if plain_object(object))
                    });
                    if !exported_receiver { return false; }
                    receiver_exported = true;
                }
                _ => return false,
            }
        }
        owner_exported && receiver_exported
    })
    .unwrap_or(false)
}

fn noop_replacement(argument: &Argument<'_>) -> bool {
    let Argument::ArrowFunctionExpression(arrow) = argument else {
        return false;
    };
    if arrow.r#async || !arrow.params.items.is_empty() || arrow.params.rest.is_some() {
        return false;
    }
    if !arrow.expression {
        return arrow.body.statements.is_empty();
    }
    matches!(arrow.body.statements.as_slice(), [Statement::ExpressionStatement(statement)]
        if identifier(&statement.expression) == Some("undefined"))
}

fn admitted_source(root: &Path, file: &Path) -> Option<String> {
    super::bounded_read::read_workspace_sources_capped(
        root,
        &[file.to_path_buf()],
        super::bounded_read::ts_file_read_limit(),
        super::bounded_read::ts_workspace_read_budget(),
    )
    .sources
    .remove(file)
}

fn plain_object(object: &oxc_ast::ast::ObjectExpression<'_>) -> bool {
    object.properties.iter().all(|property| matches!(property,
        ObjectPropertyKind::ObjectProperty(property) if property.kind == oxc_ast::ast::PropertyKind::Init
            && !property.computed && property.method && matches!(&property.value, Expression::FunctionExpression(_))))
}
