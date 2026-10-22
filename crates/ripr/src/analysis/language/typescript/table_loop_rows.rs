//! A `for (const [a, b] of rows)` loop over a test-local literal table as
//! concrete cases (RIPR-SPEC-0243 rule 11, amended for #9533).
//!
//! `const rows: Array<[number, boolean]> = [[4999, false], [5000, true]];
//! for (const [cents, want] of rows) { assert.equal(f(cents), want); }`
//! runs the body once per row, so it is read as the body with the names
//! replaced by that row's literals, exactly as `it.each` rows are.
//!
//! Fail-closed: anything else keeps the loop's own assertions. The table
//! `const` must be a top-level binding the test mentions only in its
//! declaration and in this loop (a `rows.push(...)` or a second read could
//! change what the loop sees), every row must be an array of one-line
//! literals as long as the destructuring pattern, and the loop body must be
//! a block with no `break`, `continue`, `return` or `throw`, which could end
//! the loop before a row runs, and no branch or inner loop (`if`, `switch`,
//! `while`, `?:`, `&&`, `||`), because a substituted row would then credit an
//! assertion that its own literals never reach.

use super::*;

/// A top-level `const` holding an inline literal table.
#[derive(Clone, Debug)]
pub(crate) struct TableConst {
    name: String,
    rows: Vec<Vec<String>>,
    declared_end: u32,
}

/// The test body's top-level `const <name> = [[...], ...]` tables whose
/// name appears nowhere else but one other place (the loop that reads it).
pub(crate) fn table_loop_consts(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &SourceText<'_>,
) -> Vec<TableConst> {
    let (Some(first), Some(last)) = (statements.first(), statements.last()) else {
        return Vec::new();
    };
    let text: &str = source;
    let Some(body_text) = text.get(first.span().start as usize..last.span().end as usize) else {
        return Vec::new();
    };
    let mut declared: Vec<String> = Vec::new();
    let mut tables = Vec::new();
    for stmt in statements {
        match stmt {
            Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    declared.extend(
                        declarator
                            .id
                            .get_binding_identifiers()
                            .iter()
                            .map(|ident| ident.name.to_string()),
                    );
                    if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const {
                        continue;
                    }
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let Some(Expression::ArrayExpression(array)) = declarator
                        .init
                        .as_ref()
                        .map(Expression::get_inner_expression)
                    else {
                        continue;
                    };
                    let Some(rows) = literal_array_rows(array, source) else {
                        continue;
                    };
                    tables.push(TableConst {
                        name: id.name.to_string(),
                        rows,
                        declared_end: declaration.span.end,
                    });
                }
            }
            Statement::FunctionDeclaration(function) => {
                declared.extend(function.id.as_ref().map(|id| id.name.to_string()));
            }
            Statement::ClassDeclaration(class) => {
                declared.extend(class.id.as_ref().map(|id| id.name.to_string()));
            }
            _ => {}
        }
    }
    tables.retain(|table| {
        declared.iter().filter(|name| **name == table.name).count() == 1
            && body_text
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                .filter(|word| *word == table.name)
                .count()
                == 2
    });
    tables
}

/// The assertions of a `for (const [a, b] of <table const>)` loop read once
/// per row, or `None` when the loop is not that shape (see the module docs).
pub(crate) fn table_loop_assertions(
    for_of: &oxc_ast::ast::ForOfStatement<'_>,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
    returns_to_runner: bool,
    tables: &[TableConst],
) -> Option<Vec<TypeScriptAssertion>> {
    if for_of.r#await {
        return None;
    }
    let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) = &for_of.left else {
        return None;
    };
    if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const {
        return None;
    }
    let [declarator] = declaration.declarations.as_slice() else {
        return None;
    };
    let oxc_ast::ast::BindingPattern::ArrayPattern(pattern) = &declarator.id else {
        return None;
    };
    if pattern.rest.is_some() || pattern.elements.is_empty() {
        return None;
    }
    let mut params = Vec::new();
    for element in &pattern.elements {
        // A defaulted element is an assignment pattern, not a plain binding.
        let Some(oxc_ast::ast::BindingPattern::BindingIdentifier(ident)) = element.as_ref() else {
            return None;
        };
        // The substitution scanner reads ASCII identifiers only.
        if !ident.name.is_ascii() {
            return None;
        }
        params.push(ident.name.to_string());
    }
    let Expression::Identifier(table_name) = for_of.right.get_inner_expression() else {
        return None;
    };
    let table = tables
        .iter()
        .find(|table| table.name == table_name.name.as_str())
        .filter(|table| table.declared_end <= for_of.span.start)?;
    if table.rows.iter().any(|row| row.len() != params.len()) {
        return None;
    }
    let Statement::BlockStatement(block) = &for_of.body else {
        return None;
    };
    let text: &str = source;
    let body = text.get(block.span.start as usize..block.span.end as usize)?;
    if !body.is_ascii()
        || body
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .any(|word| {
                matches!(
                    word,
                    "break"
                        | "continue"
                        | "return"
                        | "throw"
                        | "if"
                        | "else"
                        | "switch"
                        | "case"
                        | "default"
                        | "while"
                        | "do"
                        | "for"
                        | "try"
                        | "with"
                )
            })
        || body.contains(['?'])
        || body.contains("&&")
        || body.contains("||")
        || params
            .iter()
            .any(|param| local_identifier_declared_in_test_body(body, param))
    {
        return None;
    }
    let mut assertions = Vec::new();
    for row in &table.rows {
        let substituted = substitute_row(body, &params, row)?;
        assertions.extend(reparse_row_assertions(
            text,
            block.span.start as usize,
            &format!("() => {substituted}"),
            bindings,
            returns_to_runner,
        )?);
    }
    Some(assertions)
}
