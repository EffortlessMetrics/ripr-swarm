//! Methods wrapped by a pass-through method decorator (RIPR-SPEC-0243 rule 20).
//!
//! A changed line inside `D(target, key, descriptor)` is reached by no test
//! that calls `D`: tests call the class the decorator is applied to. For one
//! narrow shape the decorated method's call result *is* the wrapper's return
//! value, so a test of the method can observe an edit to that return:
//!
//! ```ts
//! export function audited(_t: object, key: string, descriptor: PropertyDescriptor) {
//!   const original = descriptor.value;
//!   descriptor.value = function (this: unknown, ...args: number[]) {
//!     calls.push(key);
//!     return original.apply(this, args) + 1;
//!   };
//! }
//! export class Ledger { @audited add(a, b) { return a + b; } }
//! ```
//!
//! The analysis is syntax-only, same-file and fail-closed. A decorator
//! qualifies only when:
//!
//! - it is one function declaration with exactly three plain parameters
//!   and no other declaration of its name;
//! - its only use of the descriptor parameter is reading `descriptor.value`
//!   into a `const` and assigning `descriptor.value = function (...) {...}`
//!   once; any other descriptor member (`get`, `set`, `writable`) refuses;
//! - the wrapper is a plain function (not async, not a generator) whose
//!   statements before the last are expression statements and whose last
//!   statement is a `return` that calls the captured original with `this`;
//! - the returned expression has no conditional or logical operator and
//!   reads no identifier besides the captured original, `this`, and the
//!   wrapper's own parameters, so it cannot return a cached or stored
//!   value in place of the original's result.
//!
//! An application qualifies when it is the only decorator of a non-static,
//! non-computed ordinary instance method written as a bare `@D` in a class
//! of the same file that carries no class decorator. Anything else keeps the
//! old `decorator_indirection` reading.

use super::*;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentTarget, BindingPattern, Declaration,
    ExportDefaultDeclarationKind, Expression, Function, Statement,
};
use std::collections::BTreeMap;

/// Maximum decorated methods recorded per decorator owner. Dropping entries
/// only withholds reach, so the bound is fail-closed.
const DECORATED_ENTRY_MAX_PER_OWNER: usize = 64;

/// Record, on each qualifying decorator owner, the methods it wraps.
pub(crate) fn assign_decorated_entries(
    program: &oxc_ast::ast::Program<'_>,
    source: &SourceText<'_>,
    owners: &mut [TypeScriptOwner],
) {
    let applications = decorator_applications(program);
    if applications.is_empty() {
        return;
    }
    let mut functions: BTreeMap<&str, Vec<&Function<'_>>> = BTreeMap::new();
    for statement in &program.body {
        let function = match statement {
            Statement::FunctionDeclaration(function) => Some(&**function),
            Statement::ExportNamedDeclaration(export) => match export.declaration.as_ref() {
                Some(Declaration::FunctionDeclaration(function)) => Some(&**function),
                _ => None,
            },
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(function) => Some(&**function),
                _ => None,
            },
            _ => None,
        };
        if let Some(function) = function
            && let Some(id) = function.id.as_ref()
        {
            functions
                .entry(id.name.as_str())
                .or_default()
                .push(function);
        }
    }
    for owner in owners.iter_mut() {
        if owner.owner_kind != OwnerKind::Function || owner.class_name.is_some() {
            continue;
        }
        let Some(methods) = applications.get(owner.name.as_str()) else {
            continue;
        };
        let [function] = functions
            .get(owner.name.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[])
        else {
            continue;
        };
        let Some((return_start_line, return_end_line)) = pass_through_return(function, source)
        else {
            continue;
        };
        let mut entries: Vec<TypeScriptModuleEntry> = methods
            .iter()
            .map(|(class_name, method)| TypeScriptModuleEntry {
                name: method.clone(),
                exported_as_default: false,
                class_member: None,
                decorated: Some(TypeScriptDecoratedMethod {
                    class_name: class_name.clone(),
                    return_start_line,
                    return_end_line,
                }),
            })
            .collect();
        entries.sort();
        entries.dedup();
        entries.truncate(DECORATED_ENTRY_MAX_PER_OWNER);
        owner.module_entries.extend(entries);
    }
}

/// `decorator name -> [(class, method)]` for every qualifying bare `@name`
/// application in the program's top-level classes.
fn decorator_applications(
    program: &oxc_ast::ast::Program<'_>,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut visit = |class: &Class<'_>| {
        let Some(class_name) = class.id.as_ref().map(|id| id.name.as_str().to_string()) else {
            return;
        };
        if !class.decorators.is_empty() {
            return;
        }
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.computed
                || method.r#static
                || method.kind != oxc_ast::ast::MethodDefinitionKind::Method
                || method.decorators.len() != 1
            {
                continue;
            }
            let Some(method_name) = property_key_name(&method.key) else {
                continue;
            };
            if let Expression::Identifier(decorator) = &method.decorators[0].expression {
                out.entry(decorator.name.as_str().to_string())
                    .or_default()
                    .push((class_name.clone(), method_name));
            }
        }
    };
    for statement in &program.body {
        match statement {
            Statement::ClassDeclaration(class) => visit(class),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = export.declaration.as_ref() {
                    visit(class);
                }
            }
            _ => {}
        }
    }
    out
}

/// Lines of the wrapper's final `return` when `decorator` is a pass-through
/// method decorator; `None` for every other shape.
fn pass_through_return(
    decorator: &Function<'_>,
    source: &SourceText<'_>,
) -> Option<(usize, usize)> {
    if decorator.r#async || decorator.generator || decorator.params.rest.is_some() {
        return None;
    }
    let [_, _, descriptor_param] = decorator.params.items.as_slice() else {
        return None;
    };
    let BindingPattern::BindingIdentifier(descriptor) = &descriptor_param.pattern else {
        return None;
    };
    let descriptor = descriptor.name.as_str();
    let body = decorator.body.as_ref()?;
    let text = &source[decorator.span.start as usize..decorator.span.end as usize];
    // The descriptor may only be read once and assigned once, both as `.value`.
    let member = format!("{descriptor}.value");
    let stripped = text.replace(&member, "");
    if word_count(&stripped, descriptor) != 1 || text.matches(&member).count() != 2 {
        return None;
    }
    let mut original: Option<&str> = None;
    let mut wrapper: Option<&Function<'_>> = None;
    for statement in &body.statements {
        match statement {
            Statement::VariableDeclaration(declaration) if wrapper.is_none() => {
                let [declarator] = declaration.declarations.as_slice() else {
                    continue;
                };
                let (BindingPattern::BindingIdentifier(id), Some(init)) =
                    (&declarator.id, declarator.init.as_ref())
                else {
                    continue;
                };
                let init_text = &source[init.span().start as usize..init.span().end as usize];
                if init_text == member || init_text.starts_with(&format!("{member} as ")) {
                    if original.is_some() {
                        return None;
                    }
                    original = Some(id.name.as_str());
                }
            }
            Statement::ExpressionStatement(expression) => {
                if let Expression::AssignmentExpression(assignment) = &expression.expression
                    && assigns_descriptor_value(assignment, descriptor)
                {
                    if wrapper.is_some() {
                        return None;
                    }
                    let Expression::FunctionExpression(function) = &assignment.right else {
                        return None;
                    };
                    wrapper = Some(function);
                }
            }
            _ => {}
        }
    }
    let (original, wrapper) = (original?, wrapper?);
    if wrapper.r#async || wrapper.generator {
        return None;
    }
    let wrapper_body = wrapper.body.as_ref()?;
    let (last, rest) = wrapper_body.statements.split_last()?;
    if !rest
        .iter()
        .all(|statement| matches!(statement, Statement::ExpressionStatement(_)))
    {
        return None;
    }
    let Statement::ReturnStatement(ret) = last else {
        return None;
    };
    let argument = ret.argument.as_ref()?;
    let returned = &source[argument.span().start as usize..argument.span().end as usize];
    let calls_original = [".apply(this", ".call(this"]
        .iter()
        .any(|call| returned.contains(&format!("{original}{call}")));
    if !calls_original
        || returned.contains('?')
        || returned.contains("&&")
        || returned.contains("||")
    {
        return None;
    }
    let mut allowed: Vec<&str> = vec![original, "this", "true", "false", "null", "undefined"];
    for item in &wrapper.params.items {
        if let BindingPattern::BindingIdentifier(id) = &item.pattern {
            allowed.push(id.name.as_str());
        }
    }
    if let Some(rest_param) = wrapper.params.rest.as_ref()
        && let BindingPattern::BindingIdentifier(id) = &rest_param.rest.argument
    {
        allowed.push(id.name.as_str());
    }
    if free_identifiers(returned)
        .iter()
        .any(|name| !allowed.contains(name))
    {
        return None;
    }
    Some((
        source.line_for_offset(ret.span.start as usize),
        source.line_for_offset(ret.span.end as usize),
    ))
}

fn assigns_descriptor_value(assignment: &AssignmentExpression<'_>, descriptor: &str) -> bool {
    if assignment.operator != oxc_ast::ast::AssignmentOperator::Assign {
        return false;
    }
    let AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
        return false;
    };
    member.property.name == "value"
        && matches!(&member.object, Expression::Identifier(object) if object.name == descriptor)
}

fn word_count(text: &str, word: &str) -> usize {
    text.match_indices(word)
        .filter(|(idx, _)| {
            let before = text[..*idx].chars().next_back();
            let after = text[idx + word.len()..].chars().next();
            before.is_none_or(|ch| !is_javascript_identifier_char(ch))
                && after.is_none_or(|ch| !is_javascript_identifier_char(ch))
        })
        .count()
}

/// Identifier tokens of `expression` that are not property names (`.apply`)
/// or part of a numeric literal. Strings and templates are rejected by the
/// caller's other checks only in so far as their words appear here, which is
/// fail-closed.
fn free_identifiers(expression: &str) -> Vec<&str> {
    let bytes = expression.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_digit() {
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || bytes[index] == b'.'
                    || bytes[index] == b'_')
            {
                index += 1;
            }
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || bytes[index] == b'_'
                    || bytes[index] == b'$')
            {
                index += 1;
            }
            let after_dot = expression[..start].trim_end().ends_with('.');
            if !after_dot {
                out.push(&expression[start..index]);
            }
            continue;
        }
        index += 1;
    }
    out
}

#[cfg(test)]
#[path = "decorated_entries_tests.rs"]
mod decorated_entries_tests;
