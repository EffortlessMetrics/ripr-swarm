//! #9766: a same-file literal fixture feeding a receiver's tuple projection.
//!
//! This is deliberately a subset of owner-return pins: a `&self` method's
//! only expression is `self.field.iter().map(|(.., x, ..)| x).sum()`, where
//! `field` is a standard `Vec` of tuples and `x` is `u32`. The helper's only
//! expression constructs that record with nonempty literal tuple data.
//! Neither helper return types alone nor literal data in a different field
//! establish that the changed closure runs.

use super::{CallShape, OwnerPinSyntax, collapse_whitespace, is_plain_identifier};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};

pub(super) struct FixtureReceiver {
    pub(super) type_name: String,
    field: String,
    tuple_types: Vec<String>,
    projected: usize,
}

impl FixtureReceiver {
    pub(super) fn establish(
        owner: &FunctionSummary,
        expression: &str,
        index: &RustIndex,
    ) -> Option<Self> {
        let FunctionContainer::Inherent { self_ty } = &owner.item.container else {
            return None;
        };
        if !is_plain_identifier(self_ty) || super::declared_return_type(&owner.body)? != "u32" {
            return None;
        }
        let function = function(&owner.body)?;
        let params = function.param_list()?;
        if params.self_param()?.syntax().text().to_string() != "&self"
            || params.params().next().is_some()
            || !plain_function(&function)
        {
            return None;
        }
        let body = function.body()?;
        if body.statements().next().is_some() {
            return None;
        }
        let tail = body.tail_expr()?;
        if collapse_whitespace(&tail.syntax().text().to_string())
            != collapse_whitespace(expression.trim().trim_end_matches(';'))
        {
            return None;
        }
        let sum = method(tail, "sum", 0)?;
        let map = method(sum.receiver()?, "map", 1)?;
        let ast::Expr::ClosureExpr(closure) = map.arg_list()?.args().next()? else {
            return None;
        };
        if closure.move_token().is_some()
            || closure.async_token().is_some()
            || closure.ret_type().is_some()
        {
            return None;
        }
        let list = closure.param_list()?;
        let mut params = list.params();
        let (Some(param), None) = (params.next(), params.next()) else {
            return None;
        };
        if param.ty().is_some() {
            return None;
        }
        let ast::Pat::TuplePat(pattern) = param.pat()? else {
            return None;
        };
        let mut projected = None;
        let mut arity = 0;
        for (position, pattern) in pattern.fields().enumerate() {
            arity += 1;
            match pattern {
                ast::Pat::WildcardPat(_) => {}
                ast::Pat::IdentPat(binding)
                    if binding.ref_token().is_none()
                        && binding.mut_token().is_none()
                        && binding.pat().is_none()
                        && projected.is_none()
                        && closure.body()?.syntax().text() == binding.name()?.text() =>
                {
                    projected = Some(position);
                }
                _ => return None,
            }
        }
        let projected = projected?;
        let iter = method(map.receiver()?, "iter", 0)?;
        let ast::Expr::FieldExpr(field) = iter.receiver()? else {
            return None;
        };
        if field.expr()?.syntax().text() != "self" {
            return None;
        }
        let field = field.name_ref()?.text().to_string();
        let source = &index.files().get(&owner.file)?.source;
        let parse = parse_clean_source_file(source)?;
        let mut records = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Struct::cast)
            .filter(|record| record.name().is_some_and(|name| name.text() == self_ty));
        let (Some(record), None) = (records.next(), records.next()) else {
            return None;
        };
        if record.attrs().next().is_some() || record.generic_param_list().is_some() {
            return None;
        }
        let ast::FieldList::RecordFieldList(fields) = record.field_list()? else {
            return None;
        };
        let mut fields = fields.fields();
        let (Some(declared), None) = (fields.next(), fields.next()) else {
            return None;
        };
        if declared.name()?.text() != field || declared.attrs().next().is_some() {
            return None;
        }
        let ty = collapse_whitespace(&declared.ty()?.syntax().text().to_string());
        let inner = ty.strip_prefix("Vec<(")?.strip_suffix(")>")?;
        let tuple_types: Vec<String> = inner
            .split(',')
            .map(str::trim)
            .filter(|ty| !ty.is_empty())
            .map(str::to_string)
            .collect();
        if tuple_types.len() != arity
            || tuple_types.get(projected)? != "u32"
            || tuple_types
                .iter()
                .any(|ty| !matches!(ty.as_str(), "u32" | "String"))
            || ["Vec", "String", "u32"].iter().any(|name| {
                super::declaring_file_rebinds(source, name, false, index)
                    || source_declares_type(source, name)
            })
            || !super::super::element_field_pin::drop_is_plain(index, self_ty)
        {
            return None;
        }
        Some(Self {
            type_name: self_ty.clone(),
            field,
            tuple_types,
            projected,
        })
    }

    /// The constructed call spelling is recognized only for this gate;
    /// general receiver/call identity admission remains unchanged.
    pub(super) fn inline_call<'a>(operand: &'a str, name: &str) -> Option<CallShape<'a>> {
        let receiver = operand.strip_suffix(&format!(".{name}()"))?;
        let helper = receiver.strip_suffix("()")?;
        is_plain_identifier(helper).then_some(CallShape::Constructed(receiver))
    }

    pub(super) fn admits(
        &self,
        test: &TestSummary,
        assertion: &OracleFact,
        call: &str,
        expected: &str,
        index: &RustIndex,
        syntax: &OwnerPinSyntax,
    ) -> Option<()> {
        integer(expected)?;
        let call_expr = expression(call)?;
        let ast::Expr::MethodCallExpr(call_expr) = call_expr else {
            return None;
        };
        if call_expr.arg_list()?.args().next().is_some() {
            return None;
        }
        let receiver = call_expr.receiver()?;
        let test_fn = function(&test.body)?;
        let block = test_fn.body()?;
        // Only an eager top-level assertion, optionally preceded by its one
        // immutable fixture binding. No later write, alias or local helper
        // can change the record or shadow the called helper.
        let statements: Vec<_> = block.statements().collect();
        let (helper_call, expected_statements) = match receiver {
            ast::Expr::CallExpr(helper) => (helper, 1),
            ast::Expr::PathExpr(path) => {
                let name = path.syntax().text().to_string();
                let ast::Stmt::LetStmt(binding) = statements.first()? else {
                    return None;
                };
                let ast::Pat::IdentPat(pattern) = binding.pat()? else {
                    return None;
                };
                if pattern.name()?.text() != name
                    || pattern.mut_token().is_some()
                    || pattern.ref_token().is_some()
                    || pattern.pat().is_some()
                    || binding.attrs().next().is_some()
                {
                    return None;
                }
                let ast::Expr::CallExpr(helper) = binding.initializer()? else {
                    return None;
                };
                (helper, 2)
            }
            _ => return None,
        };
        if statements.len() != expected_statements || block.tail_expr().is_some() {
            return None;
        }
        if !statements
            .last()?
            .syntax()
            .text()
            .to_string()
            .contains(&assertion.text)
        {
            return None;
        }
        if helper_call.arg_list()?.args().next().is_some() {
            return None;
        }
        let helper_name = helper_call.expr()?.syntax().text().to_string();
        if !is_plain_identifier(&helper_name) {
            return None;
        }
        let mut helpers = index
            .functions()
            .iter()
            .filter(|helper| helper.name == helper_name);
        let (Some(helper), None) = (helpers.next(), helpers.next()) else {
            return None;
        };
        if helper.file != test.file || helper.item.container != FunctionContainer::Free {
            return None;
        }
        let source = &index.files().get(&test.file)?.source;
        if super::item_nesting(source, &helper.name, helper.start_line)?.modules
            != super::item_nesting(source, &test.name, test.start_line)?.modules
            || super::file_renames_to(source, &helper_name)
        {
            return None;
        }
        let helper_fn = function(&helper.body)?;
        if !plain_function(&helper_fn)
            || helper_fn.param_list()?.params().next().is_some()
            || super::declared_return_type(&helper.body)? != self.type_name
        {
            return None;
        }
        let helper_body = helper_fn.body()?;
        if helper_body.statements().next().is_some() {
            return None;
        }
        let ast::Expr::RecordExpr(record) = helper_body.tail_expr()? else {
            return None;
        };
        if record.path()?.syntax().text() != self.type_name.as_str() {
            return None;
        }
        let fields = record.record_expr_field_list()?;
        if fields.spread().is_some() {
            return None;
        }
        let mut fields = fields.fields();
        let (Some(field), None) = (fields.next(), fields.next()) else {
            return None;
        };
        if field.name_ref()?.text() != self.field {
            return None;
        }
        let ast::Expr::MacroExpr(vector) = field.expr()? else {
            return None;
        };
        let vector = vector.macro_call()?;
        if vector.path()?.syntax().text() != "vec"
            || !syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, "vec", index)
        {
            return None;
        }
        let tokens = vector.token_tree()?.syntax().text().to_string();
        let ast::Expr::ArrayExpr(elements) = expression(&tokens)? else {
            return None;
        };
        if elements.semicolon_token().is_some() {
            return None;
        }
        let mut count = 0;
        for element in elements.exprs() {
            let ast::Expr::TupleExpr(tuple) = element else {
                return None;
            };
            let values: Vec<_> = tuple.fields().collect();
            if values.len() != self.tuple_types.len() {
                return None;
            }
            for (value, ty) in values.iter().zip(&self.tuple_types) {
                let value = value.syntax().text().to_string();
                match ty.as_str() {
                    "u32" => {
                        integer(&value)?;
                    }
                    "String" => {
                        let value = value.strip_suffix(".to_string()")?;
                        let ast::Expr::Literal(literal) = expression(value)? else {
                            return None;
                        };
                        if !matches!(literal.kind(), ast::LiteralKind::String(_)) {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
            integer(&values.get(self.projected)?.syntax().text().to_string())?;
            count += 1;
        }
        (count > 0).then_some(())
    }
}

fn plain_function(function: &ast::Fn) -> bool {
    function.async_token().is_none()
        && function.const_token().is_none()
        && function.unsafe_token().is_none()
        && function.generic_param_list().is_none()
        && function.attrs().next().is_none()
}

fn function(source: &str) -> Option<ast::Fn> {
    parse_clean_source_file(source)?
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
}

fn expression(source: &str) -> Option<ast::Expr> {
    function(&format!("fn fixture_expression() {{ {source} }}"))?
        .body()?
        .tail_expr()
}

fn method(expr: ast::Expr, name: &str, arguments: usize) -> Option<ast::MethodCallExpr> {
    let ast::Expr::MethodCallExpr(call) = expr else {
        return None;
    };
    (call.name_ref()?.text() == name
        && call.generic_arg_list().is_none()
        && call.arg_list()?.args().count() == arguments)
        .then_some(call)
}

fn integer(value: &str) -> Option<u32> {
    let ast::Expr::Literal(literal) = expression(value)? else {
        return None;
    };
    let ast::LiteralKind::IntNumber(number) = literal.kind() else {
        return None;
    };
    if number.suffix().is_some_and(|suffix| suffix != "u32") {
        return None;
    }
    u32::try_from(number.value().ok()?).ok()
}

fn source_declares_type(source: &str, name: &str) -> bool {
    let Some(parse) = parse_clean_source_file(source) else {
        return true;
    };
    parse.tree().syntax().descendants().any(|node| {
        let named = ast::Struct::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::Enum::cast(node.clone()).and_then(|item| item.name()))
            .or_else(|| ast::Union::cast(node.clone()).and_then(|item| item.name()))
            .or_else(|| ast::TypeAlias::cast(node.clone()).and_then(|item| item.name()))
            .or_else(|| ast::Trait::cast(node).and_then(|item| item.name()));
        named.is_some_and(|declared| declared.text() == name)
    })
}
