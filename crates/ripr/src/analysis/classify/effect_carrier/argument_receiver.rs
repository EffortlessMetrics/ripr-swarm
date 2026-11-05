//! Assertion-local identity for a concrete effect receiver passed to a free owner.
//! This establishes the call-site binding, not strength or runtime adequacy.
use super::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::reveal::assertion_comparison_operands;
use crate::analysis::classify::{
    OwnerPinSyntax, impl_self_type_name, targeted_free_call_reaches_owner,
};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasName},
};
use std::collections::BTreeSet;

pub(in crate::analysis) struct ArgumentReceiver {
    parameter: usize,
    fields: BTreeSet<String>,
}
fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
fn bare_path(expr: ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(expr) = expr else {
        return None;
    };
    let path = expr.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    Some(path.segment()?.name_ref()?.text().to_string())
}
fn parse_function(source: &str) -> Option<ast::Fn> {
    parse_clean_source_file(source)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}
fn expression(statement: &ast::Stmt) -> Option<ast::Expr> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    statement.expr()
}
fn mentions(node: &ra_ap_syntax::SyntaxNode, binding: &str) -> bool {
    node.descendants().any(|node| {
        ast::IdentPat::cast(node.clone())
            .and_then(|pattern| pattern.name())
            .is_some_and(|name| name.text() == binding)
            || ast::PathExpr::cast(node).is_some_and(|path| {
                bare_path(ast::Expr::PathExpr(path)).as_deref() == Some(binding)
            })
    })
}
fn field_root(expr: ast::Expr) -> Option<(String, String)> {
    match expr {
        ast::Expr::PrefixExpr(prefix) => field_root(prefix.expr()?),
        ast::Expr::RefExpr(reference) => field_root(reference.expr()?),
        ast::Expr::ParenExpr(paren) => field_root(paren.expr()?),
        ast::Expr::MethodCallExpr(call) if call.arg_list()?.args().next().is_none() => {
            field_root(call.receiver()?)
        }
        ast::Expr::FieldExpr(field) => Some((
            bare_path(field.expr()?)?,
            field.name_ref()?.text().to_string(),
        )),
        _ => None,
    }
}
impl ArgumentReceiver {
    pub(in crate::analysis) fn establish(
        probe: &Probe,
        owner: &FunctionSummary,
        index: &RustIndex,
        complete: bool,
    ) -> Option<Self> {
        if !complete
            || owner.item.has_self_param
            || !matches!(
                probe.family,
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion
            )
            || index.files().get(&owner.file)?.used_lexical_fallback
        {
            return None;
        }
        let probe_fn = parse_function(&format!("fn probe() {{ {}; }}", probe.expression))?;
        let statements: Vec<_> = probe_fn.body()?.stmt_list()?.statements().collect();
        let [statement] = statements.as_slice() else {
            return None;
        };
        let ast::Expr::MethodCallExpr(call) = expression(statement)? else {
            return None;
        };
        let receiver = bare_path(call.receiver()?)?;
        let method = call.name_ref()?.text().to_string();
        let function = parse_function(&owner.body)?;
        let parameters: Vec<_> = function.param_list()?.params().collect();
        let parameter = parameters.iter().position(|parameter| {
            matches!(parameter.pat(), Some(ast::Pat::IdentPat(pattern)) if pattern.name().is_some_and(|name| name.text() == receiver))
        })?;
        let ast::Type::RefType(ty) = parameters.get(parameter)?.ty()? else {
            return None;
        };
        let ast::Type::PathType(ty) = ty.ty()? else {
            return None;
        };
        let receiver_ty = ty.path()?.segment()?.name_ref()?.text().to_string();
        // Rebinding the owner parameter would invalidate the call-site projection.
        if function
            .body()?
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .any(|pattern| pattern.name().is_some_and(|name| name.text() == receiver))
        {
            return None;
        }
        let candidates: Vec<_> = index
            .functions()
            .iter()
            .filter(|candidate| candidate.name == method)
            .collect();
        let [callee] = candidates.as_slice() else {
            return None;
        };
        if impl_self_type_name(&callee.id.0).as_deref() != Some(receiver_ty.as_str()) {
            return None;
        }
        // Reuse the carrier's write attribution. Merely reading self.status
        // must not identify an observer of the self.sent write.
        let fields = super::function_field_use(callee)?.writes;
        (!fields.is_empty()).then_some(Self { parameter, fields })
    }
    pub(in crate::analysis) fn admits(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
        test: &TestSummary,
        assertion: &OracleFact,
        syntax: &OwnerPinSyntax,
    ) -> bool {
        self.bound(owner, index, test, assertion, syntax).is_some()
    }
    fn bound(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
        test: &TestSummary,
        assertion: &OracleFact,
        syntax: &OwnerPinSyntax,
    ) -> Option<()> {
        if !syntax.admits(test, assertion, index) {
            return None;
        }
        let operands = assertion_comparison_operands(&assertion.text)?;
        let root = operands.iter().find_map(|operand| {
            let function = parse_function(&format!("fn oracle() {{ {operand}; }}"))?;
            let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
            let [statement] = statements.as_slice() else {
                return None;
            };
            let (root, field) = field_root(expression(statement)?)?;
            self.fields.contains(&field).then_some(root)
        })?;
        let function = parse_function(&test.body)?;
        let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
        let assertion_at = statements.iter().position(|statement| {
            let at = usize::from(statement.syntax().text_range().start());
            test.start_line + test.body[..at].matches('\n').count() == assertion.line
                && compact(&statement.syntax().text().to_string()).trim_end_matches(';')
                    == compact(&assertion.text).trim_end_matches(';')
        })?;
        let visible: Vec<_> = statements[..assertion_at]
            .iter()
            .filter(|statement| mentions(statement.syntax(), &root))
            .collect();
        let [definition, statement] = visible.as_slice() else {
            return None;
        };
        let ast::Stmt::LetStmt(definition) = definition else {
            return None;
        };
        let ast::Pat::IdentPat(pattern) = definition.pat()? else {
            return None;
        };
        if pattern.name()?.text() != root || definition.let_else().is_some() {
            return None;
        }
        let ast::Expr::CallExpr(call) = expression(statement)? else {
            return None;
        };
        let ast::Expr::PathExpr(callee) = call.expr()? else {
            return None;
        };
        let path = compact(&callee.syntax().text().to_string());
        let mut segments: Vec<String> = path.split("::").map(str::to_string).collect();
        if segments.pop()?.as_str() != owner.name {
            return None;
        }
        let prefix = (!segments.is_empty()).then(|| format!("{}::", segments.join("::")));
        if !targeted_free_call_reaches_owner(test, owner, prefix.as_deref(), index, syntax)
            || shared_owner_call_identity(index, owner).is_some_and(|identity| {
                identity.resolve_call(test, index, &segments) != CallTarget::Owner
            })
        {
            return None;
        }
        let argument = call.arg_list()?.args().nth(self.parameter)?;
        let argument = match argument {
            ast::Expr::RefExpr(reference) => reference.expr()?,
            other => other,
        };
        (bare_path(argument)?.as_str() == root).then_some(())
    }
}
