//! TypeScript's CommonJS module syntax: `export = Name;` in the owner module
//! and `import x = require("...")` in a test (RIPR-SPEC-0243 rule 19 as
//! amended for #16365).
//!
//! TypeScript compiles `export = Cart;` to `module.exports = Cart;` and
//! `import Cart = require("./cart")` to `const Cart = require("./cart")`, so
//! each reads exactly like its CommonJS twin. The owner side feeds the same
//! whole-export gate as `module.exports = <name>;`; the test side records the
//! same default-named namespace binding as `const x = require("<literal>")`.

use oxc_ast::ast::{Expression, ImportOrExportKind, Program, Statement, TSModuleReference};

/// `(local, source)` of a value `import local = require("<source>")`.
/// A type-only `import type x = require(...)`, an exported `export import`
/// alias and the entity-name forms (`import x = Ns.y`) are not module loads,
/// so they yield `None`.
pub(super) fn import_equals_require<'s>(stmt: &'s Statement<'_>) -> Option<(&'s str, String)> {
    let Statement::TSImportEqualsDeclaration(declaration) = stmt else {
        return None;
    };
    if declaration.import_kind != ImportOrExportKind::Value {
        return None;
    }
    let TSModuleReference::ExternalModuleReference(reference) = &declaration.module_reference
    else {
        return None;
    };
    Some((
        declaration.id.name.as_str(),
        reference.expression.value.to_string(),
    ))
}

/// The identifier of the module's one `export = <identifier>;`.
///
/// `None` when the file has no such statement, more than one export
/// assignment, an export assignment of any other expression (an inline class
/// or function, a call, an object), or any ES export statement beside it
/// (TypeScript refuses the mix, and its runtime shape is then unknown).
pub(super) fn export_equals_target<'a>(program: &'a Program<'a>) -> Option<&'a str> {
    let mut target = None;
    let mut assignments = 0usize;
    for stmt in &program.body {
        match stmt {
            Statement::TSExportAssignment(assignment) => {
                assignments += 1;
                target = match &assignment.expression {
                    Expression::Identifier(ident) => Some(ident.name.as_str()),
                    _ => None,
                };
            }
            Statement::ExportNamedDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
            | Statement::ExportAllDeclaration(_) => return None,
            _ => {}
        }
    }
    target.filter(|_| assignments == 1)
}
