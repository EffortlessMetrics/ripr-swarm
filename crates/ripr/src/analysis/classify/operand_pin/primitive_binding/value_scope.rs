//! Value bindings at the actual field, separate from primitive type names.

use super::{DirectBindings, LetBinding};
use crate::analysis::syntax::{UseBinding, name_scope_for_items};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasName},
};
use std::collections::{BTreeMap, BTreeSet};

/// Direct-body bindings cannot settle a field inside another binding scope.
pub(super) fn owner_field(function: &ast::Fn, expression: &str) -> Option<ast::RecordExprField> {
    let body = function.body()?.stmt_list()?;
    let fields = function
        .syntax()
        .descendants()
        .filter_map(ast::RecordExprField::cast)
        .filter(|field| {
            field
                .syntax()
                .text()
                .to_string()
                .trim()
                .trim_end_matches(',')
                .trim()
                == expression.trim().trim_end_matches(',').trim()
        })
        .collect::<Vec<_>>();
    let [field] = fields.as_slice() else {
        return None;
    };
    for scope in field.syntax().ancestors() {
        if scope == *body.syntax() {
            return Some(field.clone());
        }
        if matches!(
            scope.kind(),
            SyntaxKind::STMT_LIST
                | SyntaxKind::CLOSURE_EXPR
                | SyntaxKind::MATCH_ARM
                | SyntaxKind::IF_EXPR
                | SyntaxKind::WHILE_EXPR
                | SyntaxKind::FOR_EXPR
                | SyntaxKind::LOOP_EXPR
                | SyntaxKind::MATCH_EXPR
                | SyntaxKind::LET_EXPR
        ) {
            return None;
        }
    }
    None
}

/// Items are hoisted; lets contribute only after their complete declaration.
pub(super) fn direct_bindings(
    function: &ast::Fn,
    expression: &str,
    type_only_import: &dyn Fn(&[String], bool) -> bool,
) -> Option<DirectBindings> {
    let field = owner_field(function, expression)?;
    let at = field.syntax().text_range().start();
    let start = usize::from(function.fn_token()?.text_range().start());
    let body = function.body()?.stmt_list()?;
    let mut result = DirectBindings {
        lets: Vec::new(),
        value_items: BTreeSet::new(),
        glob_import: false,
        first_let_ends: BTreeMap::new(),
    };
    for item in body.syntax().children().filter_map(ast::Item::cast) {
        match item {
            ast::Item::Fn(item) => record_name(&mut result, item.name()),
            ast::Item::Const(item) => record_name(&mut result, item.name()),
            ast::Item::Static(item) => record_name(&mut result, item.name()),
            ast::Item::Struct(item) => {
                if !matches!(item.field_list(), Some(ast::FieldList::RecordFieldList(_))) {
                    record_name(&mut result, item.name());
                }
            }
            ast::Item::Use(item) => {
                let scope = name_scope_for_items(&[ast::Item::Use(item)]);
                for import in scope.imports {
                    let type_only = match &import.binding {
                        UseBinding::Path(path) | UseBinding::Renamed(path) => {
                            type_only_import(path, false)
                        }
                        UseBinding::Opaque => false,
                    };
                    if !type_only {
                        result.value_items.insert(import.bound);
                    }
                }
                result.glob_import |= scope.globs.iter().any(|glob| {
                    !glob
                        .as_ref()
                        .is_some_and(|path| type_only_import(path, true))
                });
            }
            ast::Item::ExternBlock(_)
            | ast::Item::AsmExpr(_)
            | ast::Item::MacroCall(_)
            | ast::Item::MacroDef(_)
            | ast::Item::MacroRules(_) => return None,
            // These declarations introduce no unqualified value constructor.
            ast::Item::Enum(_)
            | ast::Item::ExternCrate(_)
            | ast::Item::Impl(_)
            | ast::Item::Module(_)
            | ast::Item::Trait(_)
            | ast::Item::TypeAlias(_)
            | ast::Item::Union(_) => {}
        }
    }
    for binding in body.syntax().children().filter_map(ast::LetStmt::cast) {
        // Even a later opaque attribute might expand to hoisted items.
        if binding
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
        {
            return None;
        }
        if binding.syntax().text_range().end() > at {
            continue;
        }
        let binding = read_let(binding, start)?;
        result
            .first_let_ends
            .entry(binding.name.trim_start_matches("r#").to_string())
            .and_modify(|end| *end = (*end).min(binding.completed))
            .or_insert(binding.completed);
        result.lets.push(binding);
    }
    Some(result)
}

fn read_let(binding: ast::LetStmt, start: usize) -> Option<LetBinding> {
    if binding.let_else().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    if !super::super::is_identifier(&name) {
        return None;
    }
    let initializer = binding.initializer()?;
    let offset = usize::from(initializer.syntax().text_range().start()).checked_sub(start)?;
    Some(LetBinding {
        name,
        expression: super::super::strip_line_comments(&initializer.syntax().text().to_string()),
        annotation: binding.ty().map(|ty| ty.syntax().text().to_string()),
        offset,
        completed: usize::from(binding.syntax().text_range().end()).checked_sub(start)?,
    })
}

fn record_name(bindings: &mut DirectBindings, name: Option<ast::Name>) {
    if let Some(name) = name {
        bindings.value_items.insert(name.text().to_string());
    }
}
