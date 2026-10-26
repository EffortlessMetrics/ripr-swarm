//! RIPR-SPEC-0243 rule 23: a Playwright fixture that constructs a class.
//!
//! `const test = base.extend<{ cart: Cart }>({ cart: async ({}, use) => {
//! await use(new Cart()); } })` hands every `test("...", ({ cart }) => ...)`
//! a fresh `Cart`. That is the same fact as a file-level `const cart = new
//! Cart()`, so a destructured parameter bound to such a fixture resolves to a
//! constructed scope binding. The recognizer is deliberately narrow: only the
//! plain construct-and-use form of a test imported from `@playwright/test`,
//! in a file that extends once, merges nothing and overrides no fixture.
//! Every other fixture shape leaves its parameter unresolved, as before.

use super::tests_extract::identifier_occurrences;
use super::*;
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, ObjectPropertyKind, PropertyKey, Statement,
};

const PLAYWRIGHT_TEST_MODULE: &str = "@playwright/test";

/// The fixtures one extended Playwright `test` constructs.
pub(crate) struct PlaywrightFixtures {
    /// The local name the extended test is registered through.
    test: String,
    /// `(fixture key, constructed class)`.
    fixtures: Vec<(String, String)>,
    /// Offsets of the fixture keys in the `extend` object and its type
    /// arguments, which name the fixtures without reading or writing them.
    pub(crate) sites: Vec<usize>,
}

/// What one registration's destructured parameters bind to fixtures.
pub(crate) struct FixtureParameters {
    /// `(local name, constructed class)`.
    pub(crate) bindings: Vec<(String, String)>,
    /// Offsets of the keys and local names the destructuring wrote.
    pub(crate) sites: Vec<usize>,
    /// Source offset where the callback's parameter list ends.
    pub(crate) parameter_end: usize,
}

impl PlaywrightFixtures {
    /// The fixtures a top-level `const test = base.extend({...})` declares,
    /// when `base` is Playwright's `test` and the file keeps them intact.
    pub(crate) fn of_program(
        body: &[Statement<'_>],
        source: &str,
        imports: &[TypeScriptImport],
    ) -> Option<Self> {
        let bases: Vec<&str> = imports
            .iter()
            .filter(|import| {
                import.source == PLAYWRIGHT_TEST_MODULE
                    && !import.namespace
                    && import.imported.as_deref() == Some("test")
            })
            .map(|import| import.local.as_str())
            .collect();
        if bases.is_empty() || !file_is_unmerged(source) {
            return None;
        }
        let mut found = None;
        for statement in body {
            let declaration = match statement {
                Statement::VariableDeclaration(declaration) => Some(&**declaration),
                Statement::ExportNamedDeclaration(export) => match &export.declaration {
                    Some(oxc_ast::ast::Declaration::VariableDeclaration(declaration)) => {
                        Some(&**declaration)
                    }
                    _ => None,
                },
                _ => None,
            };
            let Some(declaration) = declaration else {
                continue;
            };
            for declarator in &declaration.declarations {
                if let Some(fixtures) = declarator_fixtures(declarator, &bases, source) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(fixtures);
                }
            }
        }
        let fixtures = found?;
        test_name_is_declared_once(source, &fixtures.test).then_some(fixtures)
    }

    /// The fixture bindings of a registration `test("...", ({ cart }) => ...)`
    /// through the extended test itself, not `test.only` or a nested member.
    pub(crate) fn parameters_of(
        &self,
        call: &oxc_ast::ast::CallExpression<'_>,
    ) -> Option<FixtureParameters> {
        let Expression::Identifier(callee) = &call.callee else {
            return None;
        };
        if callee.name != self.test.as_str() {
            return None;
        }
        let params = match call
            .arguments
            .get(super::tests_extract::declaration_callback_index(call))?
        {
            Argument::ArrowFunctionExpression(arrow) => &arrow.params,
            Argument::FunctionExpression(function) => &function.params,
            _ => return None,
        };
        let BindingPattern::ObjectPattern(pattern) = &params.items.first()?.pattern else {
            return None;
        };
        let mut out = FixtureParameters {
            bindings: Vec::new(),
            sites: Vec::new(),
            parameter_end: params.span.end as usize,
        };
        for property in &pattern.properties {
            let (PropertyKey::StaticIdentifier(key), BindingPattern::BindingIdentifier(local)) =
                (&property.key, &property.value)
            else {
                continue;
            };
            if property.computed {
                continue;
            }
            let Some((_, class)) = self
                .fixtures
                .iter()
                .find(|(name, _)| name == key.name.as_str())
            else {
                continue;
            };
            out.bindings.push((local.name.to_string(), class.clone()));
            out.sites.push(key.span.start as usize);
            out.sites.push(local.span.start as usize);
        }
        Some(out)
    }
}

/// One extend per file, no `mergeTests`, no `test.use` override. Text-level
/// on purpose: a mention in a comment only withholds.
fn file_is_unmerged(source: &str) -> bool {
    source.matches(".extend").count() == 1
        && !source.contains("mergeTests")
        && !source.contains(".use(")
}

/// The extended test's name is declared once and never reassigned or
/// shadowed by a declaration of the same name elsewhere in the file.
fn test_name_is_declared_once(source: &str, name: &str) -> bool {
    let mut bindings = 0;
    for at in identifier_occurrences(source, name) {
        let before = source[..at].trim_end();
        let after = source[at + name.len()..].trim_start();
        let declares = ["const", "let", "var", "function", "class", "import"]
            .iter()
            .any(|keyword| before.ends_with(keyword));
        let assigns =
            after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>");
        if declares || assigns {
            bindings += 1;
        }
    }
    bindings == 1
}

fn declarator_fixtures(
    declarator: &oxc_ast::ast::VariableDeclarator<'_>,
    bases: &[&str],
    source: &str,
) -> Option<PlaywrightFixtures> {
    let BindingPattern::BindingIdentifier(test) = &declarator.id else {
        return None;
    };
    let Expression::CallExpression(call) = declarator.init.as_ref()? else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name != "extend" || member.optional {
        return None;
    }
    let Expression::Identifier(base) = &member.object else {
        return None;
    };
    if !bases.contains(&base.name.as_str()) || call.optional || call.arguments.len() != 1 {
        return None;
    }
    let Argument::ObjectExpression(object) = &call.arguments[0] else {
        return None;
    };
    let mut fixtures: Vec<(String, String)> = Vec::new();
    let mut sites: Vec<usize> = Vec::new();
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.computed || property.method || property.shorthand {
            return None;
        }
        let PropertyKey::StaticIdentifier(key) = &property.key else {
            return None;
        };
        let class = constructed_class(&property.value)?;
        if fixtures.iter().any(|(name, _)| name == key.name.as_str()) {
            return None;
        }
        sites.push(key.span.start as usize);
        fixtures.push((key.name.to_string(), class));
    }
    // The type arguments name each fixture (`{ cart: Cart }`); the text-level
    // rebinding check would read those as writes, so record where they are.
    if let Some(type_arguments) = &call.type_arguments {
        let span = type_arguments.span;
        let region = source.get(span.start as usize..span.end as usize)?;
        for (name, _) in &fixtures {
            sites.extend(identifier_occurrences(region, name).map(|at| span.start as usize + at));
        }
    }
    Some(PlaywrightFixtures {
        test: test.name.to_string(),
        fixtures,
        sites,
    })
}

/// `async ({}, use) => { await use(new Cls(<literals>)); }`: the class a
/// fixture function constructs, when that is all it does.
fn constructed_class(value: &Expression<'_>) -> Option<String> {
    let (params, statements) = match value {
        Expression::ArrowFunctionExpression(arrow) if !arrow.expression => {
            (&arrow.params, &arrow.body.statements)
        }
        Expression::FunctionExpression(function) => {
            (&function.params, &function.body.as_ref()?.statements)
        }
        _ => return None,
    };
    if params.rest.is_some() || params.items.len() != 2 {
        return None;
    }
    let BindingPattern::ObjectPattern(first) = &params.items[0].pattern else {
        return None;
    };
    if !first.properties.is_empty() || first.rest.is_some() {
        return None;
    }
    let BindingPattern::BindingIdentifier(use_name) = &params.items[1].pattern else {
        return None;
    };
    let [Statement::ExpressionStatement(statement)] = statements.as_slice() else {
        return None;
    };
    let Expression::AwaitExpression(awaited) = &statement.expression else {
        return None;
    };
    let Expression::CallExpression(call) = &awaited.argument else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != use_name.name || call.arguments.len() != 1 {
        return None;
    }
    let Argument::NewExpression(construction) = &call.arguments[0] else {
        return None;
    };
    let Expression::Identifier(class) = &construction.callee else {
        return None;
    };
    construction
        .arguments
        .iter()
        .all(|argument| {
            matches!(
                argument,
                Argument::NumericLiteral(_)
                    | Argument::StringLiteral(_)
                    | Argument::BooleanLiteral(_)
                    | Argument::NullLiteral(_)
            )
        })
        .then(|| class.name.to_string())
}
