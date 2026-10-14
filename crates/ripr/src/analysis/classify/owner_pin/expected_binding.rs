//! Bounded SPEC-0225 expected literal bindings; no general value-flow resolution.

use super::{
    OracleFact, RustIndex, TestSummary, file_use_statements, is_plain_identifier,
    is_workspace_root, starts_with_word, use_statement_first_segment, whole_word_offsets,
    wrapper_may_be_rebound,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};

/// Resolve only an immutable, once-used expected binding immediately before
/// this exact top-level assertion in the test's own block. The caller still
/// applies every whole-value literal/equality/owner gate to its initializer.
pub(super) fn expected_literal_binding(
    test: &TestSummary,
    assertion: &OracleFact,
    operand: &str,
) -> Option<String> {
    let name = operand.trim();
    if !is_plain_identifier(name) {
        return None;
    }
    let masked = mask_comments_and_strings(&test.body);
    let uses = whole_word_offsets(&masked, name);
    let [binding_use, _] = uses.as_slice() else {
        return None;
    };
    let parse = parse_clean_source_file(&test.body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let block = function.body()?.stmt_list()?;
    let statements: Vec<_> = block.statements().collect();
    for pair in statements.windows(2) {
        let statement = pair[1].syntax();
        let text = statement.text().to_string();
        let start = usize::from(statement.text_range().start());
        if test.start_line + test.body.get(..start)?.matches('\n').count() != assertion.line
            || text.trim().trim_end_matches(';').trim()
                != assertion.text.trim().trim_end_matches(';').trim()
        {
            continue;
        }
        let ast::Stmt::LetStmt(binding) = &pair[0] else {
            return None;
        };
        if binding.attrs().next().is_some() || binding.let_else().is_some() {
            return None;
        }
        let ast::Pat::IdentPat(pattern) = binding.pat()? else {
            return None;
        };
        let declared = pattern.name()?;
        if pattern.mut_token().is_some()
            || pattern.ref_token().is_some()
            || pattern.pat().is_some()
            || declared.text() != name
            || usize::from(declared.syntax().text_range().start()) != *binding_use
        {
            return None;
        }
        return Some(binding.initializer()?.syntax().text().to_string());
    }
    None
}

/// RIPR-SPEC-0225 decision 7: the expected operand `helper()` reads as the
/// literal its zero-argument helper returns, and `T { .., ..T::default() }`
/// reads as `T` with every omitted field taken from the workspace's one
/// manual `impl Default for T` whose `default()` is a bare `T { .. }`/
/// `Self { .. }` tail. Each must be the one such function in the workspace,
/// so the caller can apply every whole-value literal gate to the
/// substituted literal.
pub(super) fn expected_item_literal(
    index: &RustIndex,
    operand: &str,
    type_name: &str,
) -> Option<String> {
    // A glob from outside the workspace can bind the helper's name or
    // `Default` in the test's scope, directly or through `use super::*`,
    // and the index cannot read what it brings (#7954 review).
    if workspace_has_opaque_glob(index) {
        return None;
    }
    let parse = parse_clean_source_file(&format!("fn ripr_expected() {{ ({operand}); }}"))?;
    let value = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::ParenExpr::cast)?
        .expr()?;
    match value {
        ast::Expr::CallExpr(call) => {
            let ast::Expr::PathExpr(callee) = call.expr()? else {
                return None;
            };
            let path = callee.path()?;
            let segment = path.segment()?;
            if path.qualifier().is_some()
                || segment.generic_arg_list().is_some()
                || call.arg_list()?.args().next().is_some()
            {
                return None;
            }
            let name = segment.name_ref()?.text().to_string();
            // `r#name` and `name` are one identifier the text scans would
            // count apart.
            if name.starts_with("r#") {
                return None;
            }
            let helper = sole_helper(index, &name)?;
            let literal = record_tail(&helper)?;
            (!has_field_attributes(&literal)).then(|| literal.syntax().text().to_string())
        }
        ast::Expr::RecordExpr(literal) => {
            let fields = literal.record_expr_field_list()?;
            let ast::Expr::CallExpr(base) = fields.spread()? else {
                return None;
            };
            let callee = base.expr()?.syntax().text().to_string();
            if base.arg_list()?.args().next().is_some()
                || (callee != format!("{type_name}::default") && callee != "Default::default")
                || literal
                    .path()
                    .is_none_or(|path| path.syntax().text() != type_name)
                || wrapper_may_be_rebound("Default", index)
            {
                return None;
            }
            let base_literal = record_tail(&sole_default(index, type_name)?)?;
            if base_literal.path().is_none_or(|path| {
                path.syntax().text() != type_name && path.syntax().text() != "Self"
            }) {
                return None;
            }
            let base_fields = base_literal.record_expr_field_list()?;
            if base_fields.spread().is_some() || base_fields.dotdot_token().is_some() {
                return None;
            }
            let named = |field: &ast::RecordExprField| {
                field
                    .field_name()
                    .map(|name| name.text().trim_start_matches("r#").to_string())
            };
            // A `#[cfg]`-disabled explicit field would drop the default's
            // value from the merge while the base still supplies it.
            if has_field_attributes(&literal) || has_field_attributes(&base_literal) {
                return None;
            }
            let explicit: Vec<_> = fields.fields().collect();
            let mut merged: Vec<String> = explicit
                .iter()
                .map(|field| field.syntax().text().to_string())
                .collect();
            for field in base_fields.fields() {
                let name = named(&field)?;
                if !explicit
                    .iter()
                    .any(|candidate| named(candidate).as_deref() == Some(name.as_str()))
                {
                    merged.push(field.syntax().text().to_string());
                }
            }
            Some(format!("{type_name} {{ {} }}", merged.join(", ")))
        }
        _ => None,
    }
}

/// Whether any indexed file glob-imports from a root other than `crate`,
/// `self`, `super` or a workspace package; standard-library globs count.
fn workspace_has_opaque_glob(index: &RustIndex) -> bool {
    index.files().values().any(|facts| {
        facts.source.contains('*')
            && file_use_statements(&facts.source).iter().any(|statement| {
                statement.contains('*')
                    && use_statement_first_segment(statement)
                        .is_none_or(|root| !is_workspace_root(root, index))
            })
    })
}

/// Every `fn name` in the workspace's parsed sources, or `None` when a
/// macro may hide another (the masked `fn name` count differs).
fn functions_named(index: &RustIndex, name: &str) -> Option<Vec<ast::Fn>> {
    let mut functions = Vec::new();
    for facts in index.files().values() {
        if !facts.source.contains(name) {
            continue;
        }
        let masked = mask_comments_and_strings(&facts.source);
        let declared = whole_word_offsets(&masked, "fn")
            .into_iter()
            .filter(|offset| starts_with_word(masked[offset + 2..].trim_start(), name))
            .count();
        if declared == 0 {
            continue;
        }
        let parse = parse_clean_source_file(&facts.source)?;
        let found: Vec<ast::Fn> = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                function
                    .name()
                    .is_some_and(|candidate| candidate.text() == name)
            })
            .collect();
        if found.len() != declared {
            return None;
        }
        functions.extend(found);
    }
    Some(functions)
}

/// The workspace's only `fn name`, a free function (in a file or module),
/// when every other mention of `name` in every file calls it: no binding,
/// constant, import or rename can stand in for it at the call.
fn sole_helper(index: &RustIndex, name: &str) -> Option<ast::Fn> {
    for facts in index.files().values() {
        if !facts.source.contains(name) {
            continue;
        }
        let masked = mask_comments_and_strings(&facts.source);
        let calls_or_declares = whole_word_offsets(&masked, name)
            .into_iter()
            .all(|offset| masked[offset + name.len()..].trim_start().starts_with('('));
        if !calls_or_declares {
            return None;
        }
    }
    let [helper] = <[ast::Fn; 1]>::try_from(functions_named(index, name)?).ok()?;
    let container = helper.syntax().parent()?;
    let free = ast::SourceFile::can_cast(container.kind())
        || container
            .parent()
            .is_some_and(|module| ast::Module::can_cast(module.kind()));
    (free
        && helper.async_token().is_none()
        && helper.generic_param_list().is_none()
        && helper.attrs().next().is_none())
    .then_some(helper)
}

/// The `default()` of the workspace's one manual, unattributed
/// `impl Default for type_name`, when no other `fn default` belongs to an
/// impl of that type: an inherent `default` wins `T::default()`, and a
/// derived `Default` beside it would run instead for `Default::default()`.
fn sole_default(index: &RustIndex, type_name: &str) -> Option<ast::Fn> {
    let mut found = None;
    for function in functions_named(index, "default")? {
        let Some(block) = function
            .syntax()
            .parent()
            .and_then(|list| list.parent())
            .and_then(ast::Impl::cast)
        else {
            continue;
        };
        if block
            .self_ty()
            .is_none_or(|ty| ty.syntax().text() != type_name)
        {
            continue;
        }
        if found.is_some()
            || block
                .trait_()
                .is_none_or(|trait_| trait_.syntax().text() != "Default")
            || block.attrs().next().is_some()
            || block.generic_param_list().is_some()
            || function.async_token().is_some()
            || function.attrs().next().is_some()
        {
            return None;
        }
        found = Some(function);
    }
    found
}

/// Whether any field of `literal` carries an attribute (`#[cfg(..)]`).
fn has_field_attributes(literal: &ast::RecordExpr) -> bool {
    literal
        .record_expr_field_list()
        .is_some_and(|list| list.fields().any(|field| field.attrs().next().is_some()))
}

/// The function's body when it is exactly a tail struct literal.
fn record_tail(function: &ast::Fn) -> Option<ast::RecordExpr> {
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    match body.tail_expr()? {
        ast::Expr::RecordExpr(literal) => Some(literal),
        _ => None,
    }
}
