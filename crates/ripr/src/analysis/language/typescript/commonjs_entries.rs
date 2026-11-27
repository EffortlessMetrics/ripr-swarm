//! CommonJS exports as same-module entries (#16369).
//!
//! The entry graph in `module_entries` starts from a module's exported names
//! so that a test calling an export reaches a private helper the export
//! calls. ES export statements were its only entries, so the CommonJS twin of
//!
//! ```js
//! function total(n) { /* changed */ }
//! function other(n) { return total(n); }
//! module.exports = { other };
//! ```
//!
//! read `no_static_path` while `export function other` related. This module
//! reads the CommonJS export writes of a file as entries:
//!
//! - each identifier-valued property of an object whole export
//!   (`module.exports = { other, alias: other }`) is a named entry;
//! - `exports.NAME = local` and `module.exports.NAME = local` are named
//!   entries when no whole export detaches them;
//! - `module.exports = local`, TypeScript `export = local` and an inline
//!   `module.exports = function NAME(...) {...}` (or arrow) are a whole-export
//!   entry, which the owner side keeps only when it proved the same function
//!   is the whole export (`commonjs_default_export`).
//!
//! It is fail-closed: a file with an ES export statement, more than one whole
//! write, or a write the top-level statements do not account for (inside a
//! function or a condition) yields no entries.

use oxc_ast::ast::{AssignmentTarget, Expression, ObjectPropertyKind, PropertyKey, Statement};

/// One CommonJS export of a top-level local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CommonJsExport<'a> {
    /// The local the export names, or the node key of an inline function.
    pub(super) local: &'a str,
    /// The property a test reads; `None` for the whole export.
    pub(super) property: Option<String>,
}

/// The inline function value of the one whole export, with the node key the
/// owner side names it by (its own name, else `default`).
pub(super) enum InlineWhole<'a> {
    Function(&'a oxc_ast::ast::Function<'a>, &'a str),
    Arrow(&'a oxc_ast::ast::ArrowFunctionExpression<'a>),
}

/// The CommonJS exports of `statements`, and the inline whole-export function
/// when there is one. Empty when the file's export shape is not known.
pub(super) fn commonjs_exports<'a>(
    statements: &'a [Statement<'a>],
    source: &str,
) -> (Vec<CommonJsExport<'a>>, Option<InlineWhole<'a>>) {
    let none = (Vec::new(), None);
    let mut whole: Option<(usize, &'a Expression<'a>)> = None;
    let mut whole_writes = 0usize;
    let mut text_whole_writes = 0usize;
    let mut named: Vec<(usize, CommonJsExport<'a>, bool)> = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        match statement {
            Statement::ExportNamedDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
            | Statement::ExportAllDeclaration(_) => return none,
            Statement::TSExportAssignment(assignment) => {
                whole_writes += 1;
                whole = Some((index, &assignment.expression));
            }
            Statement::ExpressionStatement(expression) => {
                let Expression::AssignmentExpression(assign) = &expression.expression else {
                    continue;
                };
                if assign.operator != oxc_ast::ast::AssignmentOperator::Assign {
                    continue;
                }
                match &assign.left {
                    AssignmentTarget::StaticMemberExpression(member)
                        if is_module_exports(member) =>
                    {
                        whole_writes += 1;
                        text_whole_writes += 1;
                        whole = Some((index, &assign.right));
                    }
                    AssignmentTarget::StaticMemberExpression(member) => {
                        let bare = matches!(
                            member.object.get_inner_expression(),
                            Expression::Identifier(object) if object.name == "exports"
                        );
                        let on_module_exports = matches!(
                            member.object.get_inner_expression(),
                            Expression::StaticMemberExpression(inner)
                                if inner.property.name == "exports"
                                    && matches!(&inner.object, Expression::Identifier(object) if object.name == "module")
                        );
                        if !(bare || on_module_exports) {
                            continue;
                        }
                        if let Expression::Identifier(value) = assign.right.get_inner_expression() {
                            named.push((
                                index,
                                CommonJsExport {
                                    local: value.name.as_str(),
                                    property: Some(member.property.name.to_string()),
                                },
                                bare,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // A `module.exports =` write the top-level statements did not see (in a
    // function, a branch or a sequence) may replace the export at any time.
    if named.is_empty() && whole.is_none() {
        return none;
    }
    // The mask is whole-file, so it runs only when there is a write to check.
    let text_writes = if source.contains("module.exports") {
        module_exports_writes_in_text(source)
    } else {
        0
    };
    if text_writes != text_whole_writes || whole_writes > 1 {
        return none;
    }
    let mut exports = Vec::new();
    let mut inline = None;
    match whole {
        None => exports.extend(named.into_iter().map(|(_, export, _)| export)),
        Some((at, value)) => {
            match value.get_inner_expression() {
                Expression::Identifier(ident) => exports.push(CommonJsExport {
                    local: ident.name.as_str(),
                    property: None,
                }),
                Expression::FunctionExpression(func) => {
                    let key = func.id.as_ref().map_or("default", |id| id.name.as_str());
                    exports.push(CommonJsExport {
                        local: key,
                        property: None,
                    });
                    inline = Some(InlineWhole::Function(func, key));
                }
                Expression::ArrowFunctionExpression(arrow) => {
                    exports.push(CommonJsExport {
                        local: "default",
                        property: None,
                    });
                    inline = Some(InlineWhole::Arrow(arrow));
                }
                Expression::ObjectExpression(object) => {
                    for property in &object.properties {
                        let ObjectPropertyKind::ObjectProperty(property) = property else {
                            continue;
                        };
                        if property.computed {
                            continue;
                        }
                        let PropertyKey::StaticIdentifier(key) = &property.key else {
                            continue;
                        };
                        if let Expression::Identifier(value) = property.value.get_inner_expression()
                        {
                            exports.push(CommonJsExport {
                                local: value.name.as_str(),
                                property: Some(key.name.to_string()),
                            });
                        }
                    }
                }
                _ => {}
            }
            // `module.exports.NAME = local` after the whole write adds a
            // property to the export; `exports.NAME` is detached from it, and
            // a write before it is replaced.
            exports.extend(
                named
                    .into_iter()
                    .filter(|(index, _, bare)| *index > at && !bare)
                    .map(|(_, export, _)| export),
            );
        }
    }
    (exports, inline)
}

fn is_module_exports(member: &oxc_ast::ast::StaticMemberExpression<'_>) -> bool {
    member.property.name == "exports"
        && matches!(&member.object, Expression::Identifier(object) if object.name == "module")
}

/// `module.exports` followed by a plain `=` in the code, literals and
/// comments masked out.
fn module_exports_writes_in_text(source: &str) -> usize {
    let code = super::ts_mask_literals_and_comments(source);
    let needle = "module.exports";
    code.match_indices(needle)
        .filter(|(at, _)| {
            let before = code[..*at].chars().next_back();
            if before.is_some_and(|ch| super::is_javascript_identifier_char(ch) || ch == '.') {
                return false;
            }
            let rest = code[at + needle.len()..].trim_start();
            rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
        })
        .count()
}
