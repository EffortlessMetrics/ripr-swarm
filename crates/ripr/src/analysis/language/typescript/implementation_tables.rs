//! One suite run over several implementations of a module (#13592, unjs/ohash).
//!
//! ```ts
//! import * as cryptoJS from "../src/crypto/js";
//! import * as cryptoNode from "../src/crypto/node";
//! const impls = { js: cryptoJS, node: cryptoNode };
//! for (const [name, { digest }] of Object.entries(impls)) {
//!   it(name, () => expect(digest("x")).toBe("..."));
//! }
//! ```
//!
//! Each iteration binds `digest` to the export of one namespace import, so the
//! test calls `cryptoJS.digest` on one iteration and `cryptoNode.digest` on
//! another: the same call a namespace member call (`cryptoJS.digest(...)`)
//! writes out. Without this the loop binding only shadows, and no test calls
//! the export or anything it reaches.
//!
//! Admitted, and nothing else:
//! - the table is a file-level `const` bound to an object literal whose
//!   property values (`{ js: cryptoJS }` or `{ cryptoJS }`), or an array
//!   literal whose elements, are identifiers; at least one names a namespace
//!   import;
//! - every other mention of the table name in the file is the argument of
//!   `Object.entries(` or `Object.values(`, or the iterable after `of`, so no
//!   write can swap an entry;
//! - the loop is `for (const [_, { m }] of Object.entries(T))`,
//!   `for (const { m } of Object.values(T))`, or, over an array table,
//!   `for (const { m } of T)`, and `m` is shorthand: no rename, default or
//!   rest. The member read is the binding's own name;
//! - the iteration registers the same tests every time: no branch, skip or
//!   member callee on the path, and no test body or registration argument
//!   reads another loop binding (the key or another member).

use super::*;
use oxc_ast::ast::{BindingProperty, ForOfStatement, ForStatementLeft};

/// File-level `const` tables of namespace imports, by table name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImplementationTables {
    tables: Vec<ImplementationTable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ImplementationTable {
    name: String,
    /// `true` for an array literal, `false` for an object literal.
    array: bool,
    /// The namespace import locals among its values, in source order.
    namespaces: Vec<String>,
}

impl ImplementationTables {
    pub(crate) fn of_program(
        statements: &[Statement<'_>],
        source: &str,
        imports: &[TypeScriptImport],
    ) -> Self {
        let namespace = |name: &str| {
            imports
                .iter()
                .any(|import| import.namespace && import.local == name)
        };
        let mut tables = Vec::new();
        for statement in statements {
            // An exported table is refused: another file can import it and
            // swap an entry where this file's text scan cannot see.
            let Statement::VariableDeclaration(declaration) = statement else {
                continue;
            };
            if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const
                || declaration.declare
            {
                continue;
            }
            for declarator in &declaration.declarations {
                let (BindingPattern::BindingIdentifier(identifier), Some(init)) =
                    (&declarator.id, &declarator.init)
                else {
                    continue;
                };
                let Some((array, values)) = identifier_values(init) else {
                    continue;
                };
                let namespaces: Vec<String> = values
                    .into_iter()
                    .filter(|value| namespace(value))
                    .collect();
                let name = identifier.name.as_str();
                if namespaces.is_empty()
                    || !only_enumerated(source, name, identifier.span.start as usize)
                {
                    continue;
                }
                tables.push(ImplementationTable {
                    name: name.to_string(),
                    array,
                    namespaces,
                });
            }
        }
        // A name declared twice at file level is a syntax error; refuse both.
        let duplicated: Vec<String> = tables
            .iter()
            .filter(|table| {
                tables
                    .iter()
                    .filter(|other| other.name == table.name)
                    .count()
                    > 1
            })
            .map(|table| table.name.clone())
            .collect();
        tables.retain(|table| !duplicated.contains(&table.name));
        Self { tables }
    }

    /// The names a `for...of` header binds to a member of the table's
    /// namespace imports, each with those namespace locals. `shadowed` says
    /// whether a scope between the file and the loop rebinds a name (for
    /// `Object`, whether anything in the file binds it).
    pub(crate) fn loop_members(
        &self,
        statement: &Statement<'_>,
        source: &str,
        shadowed: impl Fn(&str) -> bool,
    ) -> Vec<(String, Vec<String>)> {
        let Statement::ForOfStatement(for_of) = statement else {
            return Vec::new();
        };
        self.for_of_members(for_of, source, &shadowed)
            .unwrap_or_default()
    }

    fn for_of_members(
        &self,
        for_of: &ForOfStatement<'_>,
        source: &str,
        shadowed: &impl Fn(&str) -> bool,
    ) -> Option<Vec<(String, Vec<String>)>> {
        if for_of.r#await {
            return None;
        }
        let ForStatementLeft::VariableDeclaration(declaration) = &for_of.left else {
            return None;
        };
        // A `var` loop binding is one shared binding every test reads after
        // the loop ends, and a `let` one can be reassigned in the body: only
        // a `const` holds each iteration's member for that iteration's tests.
        if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const {
            return None;
        }
        let [declarator] = declaration.declarations.as_slice() else {
            return None;
        };
        let (table_name, enumeration) = iterated_table(&for_of.right)?;
        if shadowed(table_name) || (enumeration != Enumeration::Direct && shadowed("Object")) {
            return None;
        }
        // Every iteration must register the same tests: a skipped iteration
        // (`if (name === "js") continue;`) would credit a namespace no test
        // runs.
        let body = match &for_of.body {
            Statement::BlockStatement(block) => block.body.as_slice(),
            single => std::slice::from_ref(single),
        };
        if !registers_unconditionally(body) {
            return None;
        }
        let table = self.tables.iter().find(|table| table.name == table_name)?;
        let (key, pattern) = match (enumeration, table.array, &declarator.id) {
            // `Object.entries(T)` yields `[key, value]` pairs.
            (Enumeration::Entries, _, BindingPattern::ArrayPattern(pair)) => {
                if pair.rest.is_some() || pair.elements.len() != 2 {
                    return None;
                }
                let key = match pair.elements.first()? {
                    None => None,
                    Some(BindingPattern::BindingIdentifier(key)) => Some(key.name.as_str()),
                    Some(_) => return None,
                };
                (key, pair.elements.get(1)?.as_ref()?)
            }
            (Enumeration::Values, _, pattern) | (Enumeration::Direct, true, pattern) => {
                (None, pattern)
            }
            _ => return None,
        };
        let BindingPattern::ObjectPattern(object) = pattern else {
            return None;
        };
        if object.rest.is_some() {
            return None;
        }
        // Every name the header binds. A nested pattern or a default is
        // refused outright rather than read.
        let mut bound: Vec<&str> = key.into_iter().collect();
        for property in &object.properties {
            let BindingPattern::BindingIdentifier(binding) = &property.value else {
                return None;
            };
            bound.push(binding.name.as_str());
        }
        // A test body or a registration argument that reads another loop
        // binding (the key, or a member such as `supported` in
        // `if (!supported) return;`) can choose to skip the call on one
        // iteration, so the member is refused.
        let members: Vec<(String, Vec<String>)> = object
            .properties
            .iter()
            .filter_map(shorthand_member)
            .filter(|member| {
                let others: Vec<&str> = bound
                    .iter()
                    .copied()
                    .filter(|name| *name != member.as_str())
                    .collect();
                !test_bodies_mention(body, source, &others)
            })
            .map(|member| (member, table.namespaces.clone()))
            .collect();
        Some(members)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Enumeration {
    Entries,
    Values,
    Direct,
}

/// The table identifier a `for...of` iterates, and how.
fn iterated_table<'a>(right: &'a Expression<'a>) -> Option<(&'a str, Enumeration)> {
    match right.without_parentheses() {
        Expression::Identifier(identifier) => Some((identifier.name.as_str(), Enumeration::Direct)),
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return None;
            };
            if call.optional
                || !matches!(&member.object, Expression::Identifier(object) if object.name == "Object")
            {
                return None;
            }
            let enumeration = match member.property.name.as_str() {
                "entries" => Enumeration::Entries,
                "values" => Enumeration::Values,
                _ => return None,
            };
            let [Argument::Identifier(table)] = call.arguments.as_slice() else {
                return None;
            };
            Some((table.name.as_str(), enumeration))
        }
        _ => None,
    }
}

/// The statements between the loop and its tests run the same way on every
/// iteration: only declarations, call statements and blocks, recursing into
/// the callback of a `describe` / `context` / `suite` call. Every call on
/// that path has a bare identifier callee. No `if`, `switch`, `continue`,
/// `break`, `return`, `try`, label or conditional expression statement
/// (`cond && it(...)`) on that path.
fn registers_unconditionally(statements: &[Statement<'_>]) -> bool {
    statements.iter().all(|statement| match statement {
        Statement::VariableDeclaration(_) | Statement::EmptyStatement(_) => true,
        Statement::BlockStatement(block) => registers_unconditionally(&block.body),
        Statement::ExpressionStatement(expression) => {
            let Expression::CallExpression(call) = &expression.expression else {
                return false;
            };
            // A member or call callee (`describe.only`, `describe.skipIf(c)`,
            // `it.each(rows)`) can skip or repeat by its own arguments, which
            // this walk does not read: only a bare callee is on the path.
            let Expression::Identifier(callee) = &call.callee else {
                return false;
            };
            if !matches!(callee.name.as_str(), "describe" | "context" | "suite") {
                return true;
            }
            call.arguments.iter().all(|argument| match argument {
                Argument::ArrowFunctionExpression(function) => {
                    registers_unconditionally(&function.body.statements)
                }
                Argument::FunctionExpression(function) => function
                    .body
                    .as_ref()
                    .is_none_or(|body| registers_unconditionally(&body.statements)),
                _ => true,
            })
        }
        _ => false,
    })
}

/// Whether any of `names` appears where a registration can read it: in a
/// declaration on the registration path (`const skip = name === "js"`), in
/// the callback of a call that is not a suite (`it`, `test`, a hook: the test
/// bodies), or in any argument after the first that is not a suite callback
/// (Vitest's `it(title, { skip: name === "js" }, fn)`). A call's title, its
/// first argument, may mention them, and a suite callback is walked.
fn test_bodies_mention(statements: &[Statement<'_>], source: &str, names: &[&str]) -> bool {
    let text = |start: u32, end: u32| source.get(start as usize..end as usize).unwrap_or("");
    let mentions = |start: u32, end: u32| {
        names
            .iter()
            .any(|name| mentions_identifier(text(start, end), name))
    };
    statements.iter().any(|statement| match statement {
        Statement::BlockStatement(block) => test_bodies_mention(&block.body, source, names),
        Statement::ExpressionStatement(expression) => {
            let Expression::CallExpression(call) = &expression.expression else {
                return false;
            };
            let suite = matches!(&call.callee, Expression::Identifier(callee) if matches!(callee.name.as_str(), "describe" | "context" | "suite"));
            call.arguments
                .iter()
                .enumerate()
                .any(|(index, argument)| match argument {
                    Argument::ArrowFunctionExpression(function) if suite => {
                        test_bodies_mention(&function.body.statements, source, names)
                    }
                    Argument::FunctionExpression(function) if suite => function
                        .body
                        .as_ref()
                        .is_some_and(|body| test_bodies_mention(&body.statements, source, names)),
                    Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_) => {
                        let span = argument.span();
                        mentions(span.start, span.end)
                    }
                    _ if index == 0 => false,
                    _ => {
                        let span = argument.span();
                        mentions(span.start, span.end)
                    }
                })
        }
        Statement::VariableDeclaration(declaration) => {
            mentions(declaration.span.start, declaration.span.end)
        }
        _ => false,
    })
}

fn mentions_identifier(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|rest| rest.find(name)) {
        let start = from + found;
        let end = start + name.len();
        from = end;
        let before = text.get(..start).and_then(|t| t.chars().next_back());
        let after = text.get(end..).and_then(|t| t.chars().next());
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            return true;
        }
    }
    false
}

/// `{ digest }`: the key is the bound name, with no default.
fn shorthand_member(property: &BindingProperty<'_>) -> Option<String> {
    if property.computed {
        return None;
    }
    let key = property.key.static_name()?;
    let BindingPattern::BindingIdentifier(binding) = &property.value else {
        return None;
    };
    (binding.name.as_str() == key.as_ref()).then(|| key.to_string())
}

/// `{ js: cryptoJS, cryptoNode }` or `[cryptoJS, cryptoNode]` with every
/// value a plain identifier: `(is_array, values)`.
fn identifier_values(init: &Expression<'_>) -> Option<(bool, Vec<String>)> {
    // `as const` and `satisfies` change no runtime value.
    match super::tests_extract::runtime_expression(init) {
        Expression::ObjectExpression(object) => {
            let mut values = Vec::new();
            for property in &object.properties {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    return None;
                };
                if property.computed
                    || property.method
                    || property.kind != oxc_ast::ast::PropertyKind::Init
                {
                    return None;
                }
                let Expression::Identifier(value) = &property.value else {
                    return None;
                };
                values.push(value.name.to_string());
            }
            (!values.is_empty()).then_some((false, values))
        }
        Expression::ArrayExpression(array) => {
            let mut values = Vec::new();
            for element in &array.elements {
                let Some(Expression::Identifier(value)) = element.as_expression() else {
                    return None;
                };
                values.push(value.name.to_string());
            }
            (!values.is_empty()).then_some((true, values))
        }
        _ => None,
    }
}

/// Every mention of `name` in `source` other than its declaration at
/// `declared_at` is the argument of `Object.entries(` / `Object.values(` or
/// the iterable after `of`. A comment or string that mentions the name
/// refuses the table, which only ever withholds reach.
fn only_enumerated(source: &str, name: &str, declared_at: usize) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut from = 0;
    while let Some(found) = source.get(from..).and_then(|rest| rest.find(name)) {
        let start = from + found;
        let end = start + name.len();
        from = end;
        let before = source
            .get(..start)
            .and_then(|text| text.chars().next_back());
        let after = source.get(end..).and_then(|text| text.chars().next());
        if before.is_some_and(is_ident) || after.is_some_and(is_ident) {
            continue;
        }
        if start == declared_at {
            continue;
        }
        let prefix = source.get(..start).unwrap_or("").trim_end();
        let enumerated = prefix.ends_with("Object.entries(") || prefix.ends_with("Object.values(");
        let iterated = prefix.ends_with(" of")
            && prefix
                .get(..prefix.len().saturating_sub(2))
                .and_then(|text| text.chars().next_back())
                .is_some_and(char::is_whitespace);
        let closes = source
            .get(end..)
            .map(str::trim_start)
            .is_some_and(|rest| rest.starts_with(')'));
        if !((enumerated || iterated) && closes) {
            return false;
        }
    }
    true
}
