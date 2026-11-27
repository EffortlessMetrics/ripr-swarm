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
    // Every write to each export property, whatever its value: a slot written
    // more than once may hold a later value, so it is no entry.
    let mut slot_writes: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
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
                    AssignmentTarget::ComputedMemberExpression(member)
                        if is_exports_object(&member.object) =>
                    {
                        // `exports[key] = ...`: only a literal key names the
                        // slot it writes.
                        let Expression::StringLiteral(key) = &member.expression else {
                            return none;
                        };
                        *slot_writes.entry(key.value.to_string()).or_default() += 1;
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
                        *slot_writes
                            .entry(member.property.name.to_string())
                            .or_default() += 1;
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
    let (text_writes, passed) = if source.contains("exports") {
        let code = super::ts_mask_literals_and_comments(source);
        (
            module_exports_writes_in_text(&code),
            exports_object_passed(&code),
        )
    } else {
        (0, false)
    };
    // `Object.assign(exports, ...)` or `defineProperty(module.exports, ...)`
    // may write any slot.
    if text_writes != text_whole_writes || whole_writes > 1 || passed {
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
                        // A spread or a computed key may define any key.
                        let ObjectPropertyKind::ObjectProperty(property) = property else {
                            return none;
                        };
                        if property.computed {
                            return none;
                        }
                        let key = match &property.key {
                            PropertyKey::StaticIdentifier(key) => key.name.to_string(),
                            PropertyKey::StringLiteral(key) => key.value.to_string(),
                            _ => return none,
                        };
                        *slot_writes.entry(key.clone()).or_default() += 1;
                        if let Expression::Identifier(value) = property.value.get_inner_expression()
                        {
                            exports.push(CommonJsExport {
                                local: value.name.as_str(),
                                property: Some(key),
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
    exports.retain(|export| {
        export
            .property
            .as_ref()
            .is_none_or(|property| slot_writes.get(property).copied().unwrap_or(0) <= 1)
    });
    (exports, inline)
}

/// `exports` or `module.exports`.
fn is_exports_object(object: &Expression<'_>) -> bool {
    match object.get_inner_expression() {
        Expression::Identifier(object) => object.name == "exports",
        Expression::StaticMemberExpression(inner) => is_module_exports(inner),
        _ => false,
    }
}

/// `exports` or `module.exports` passed whole as any argument of a call in
/// the masked code (`Object.assign(exports, ...)`, `Object.assign(\n
/// exports, ...)`, `mixin(target, exports)`), where the callee may write any
/// slot.
fn exports_object_passed(code: &str) -> bool {
    code.match_indices("exports").any(|(at, needle)| {
        // `module.exports` is the same object; step back over `module.`.
        let start = code[..at]
            .strip_suffix("module.")
            .filter(|head| {
                !head
                    .chars()
                    .next_back()
                    .is_some_and(|ch| super::is_javascript_identifier_char(ch) || ch == '.')
            })
            .map_or(at, str::len);
        let before = code[..start].chars().next_back();
        if before.is_some_and(|ch| super::is_javascript_identifier_char(ch) || ch == '.') {
            return false;
        }
        let head = code[..start].trim_end();
        if !(head.ends_with('(') || head.ends_with(',')) {
            return false;
        }
        let rest = code[at + needle.len()..].trim_start();
        !rest.starts_with(|ch: char| {
            super::is_javascript_identifier_char(ch) || matches!(ch, '.' | '=' | '[')
        })
    })
}

fn is_module_exports(member: &oxc_ast::ast::StaticMemberExpression<'_>) -> bool {
    member.property.name == "exports"
        && matches!(&member.object, Expression::Identifier(object) if object.name == "module")
}

/// `module.exports` followed by a plain `=` in the masked code.
fn module_exports_writes_in_text(code: &str) -> usize {
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
