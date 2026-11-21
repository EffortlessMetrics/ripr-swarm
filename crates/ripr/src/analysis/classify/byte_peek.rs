//! A literal second-byte comparison returned by a free function (#5329).
//! The positive evidence is one admitted assertion on the very call whose
//! byte makes the old and new comparisons return different results.

use super::context::ProbeContext;
use super::gap_admission::is_ignored;
use super::owner_pin::{OwnerPinSyntax, builtin_type_unshadowed, targeted_free_call_reaches_owner};
use super::reveal::assertion_comparison_operands;
use crate::analysis::diff::{ChangedFile, ModuleDeclarations, verified_base_owner};
use crate::analysis::facts::{OracleFact, TestSummary};
use crate::analysis::syntax::{parse_clean_source_file, source_binds_value_name};
use crate::domain::{OracleKind, OracleStrength, ProbeFamily, RelationReason};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasGenericParams, HasName};
use ra_ap_syntax::{AstNode, AstToken, SyntaxKind, SyntaxToken, ast};

#[derive(Clone, Copy)]
pub(in crate::analysis) struct BytePeekChange {
    old: u8,
    new: u8,
}

pub(in crate::analysis) fn byte_peek_change(
    context: &ProbeContext<'_>,
    changed: &ChangedFile,
    modules: &ModuleDeclarations,
    syntax: &OwnerPinSyntax,
) -> Option<BytePeekChange> {
    let probe = context.probe;
    if !matches!(
        probe.family,
        ProbeFamily::Predicate | ProbeFamily::ReturnValue
    ) || !probe.expression.contains(".get(1)")
        || !context.workspace_complete
        || changed.added_lines.len() != 1
        || changed.removed_lines.len() != 1
        || context.related_tests.is_empty()
    {
        return None;
    }
    let owner = context.owner_fn?;
    let before = context.owner_sole_change.as_deref()?;
    let function = sole_function(&owner.body)?;
    if function.attrs().next().is_some()
        || function.generic_param_list().is_some()
        || function.async_token().is_some()
        || function.const_token().is_some()
        || function.unsafe_token().is_some()
        || function.ret_type()?.ty()?.syntax().text() != "bool"
    {
        return None;
    }
    let params = function.param_list()?;
    let mut params = params.params();
    let param = params.next()?;
    let ast::Pat::IdentPat(binding) = param.pat()? else {
        return None;
    };
    if params.next().is_some()
        || param.attrs().next().is_some()
        || param.ty()?.syntax().text() != "&[u8]"
        || binding.at_token().is_some()
        || binding.ref_token().is_some()
        || binding.mut_token().is_some()
    {
        return None;
    }
    let name = binding.name()?.text().to_string();
    let body = function.body()?.stmt_list()?;
    let tail = body.tail_expr()?;
    let start = usize::from(tail.syntax().text_range().start());
    if body.statements().next().is_some()
        || body
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::POUND)
        || tail.syntax().text() != probe.expression.as_str()
        || probe.location.line != owner.start_line + owner.body.get(..start)?.matches('\n').count()
    {
        return None;
    }

    let (new, replaced) = compared_byte(tail, &name)?;
    // The only changed syntax is the byte token, on the selected tail line.
    let expression = &probe.expression;
    let token_range = replaced.text_range();
    let from = usize::from(token_range.start()).checked_sub(start)?;
    let to = usize::from(token_range.end()).checked_sub(start)?;
    let prefix = expression.get(..from)?;
    let suffix = expression.get(to..)?;
    let old_text = before.trim().strip_prefix(prefix)?.strip_suffix(suffix)?;
    let old = byte_literal(parse_expression(old_text)?)?.value().ok()?;
    if old == new {
        return None;
    }
    let file = &context.index.files().get(&owner.file)?.source;
    let base = verified_base_owner(
        file,
        &owner.file,
        changed,
        modules,
        &owner.name,
        owner.start_line,
        owner.end_line,
    )?;
    if base.cfg_gated
        || !["u8", "bool", "Some"]
            .iter()
            .all(|name| builtin_type_unshadowed(name, context.index, syntax))
        || source_binds_value_name(file, "Some")
    {
        return None;
    }
    Some(BytePeekChange { old, new })
}

impl BytePeekChange {
    /// Whether this exact assertion distinguishes the edit. Unknown inputs,
    /// assertion execution, and owner identity do not supply either answer.
    pub(in crate::analysis) fn assertion_distinguishes(
        self,
        context: &ProbeContext<'_>,
        syntax: &OwnerPinSyntax,
        test: &TestSummary,
        assertion: &OracleFact,
    ) -> Option<bool> {
        let owner = context.owner_fn?;
        if assertion.kind != OracleKind::ExactValue
            || assertion.strength != OracleStrength::Strong
            || !assertion.text.trim_start().starts_with("assert_eq!")
        {
            return None;
        }
        let [left, right] = assertion_comparison_operands(&assertion.text)?;
        let distinguishes = self
            .compares(left, right, &owner.name)
            .or_else(|| self.compares(right, left, &owner.name))?;
        (context.related_tests.iter().any(|(candidate, relation)| {
            *relation == RelationReason::DirectOwnerCall && std::ptr::eq(*candidate, test)
        }) && !is_ignored(test)
            && !test
                .attrs
                .iter()
                .any(|attribute| attribute.contains("should_panic"))
            && targeted_free_call_reaches_owner(test, owner, None, context.index, syntax)
            && syntax.admits_equality_assertion(
                context.probe,
                Some(owner),
                test,
                assertion,
                context.index,
            ))
        .then_some(distinguishes)
    }

    fn compares(self, operand: &str, expected: &str, owner: &str) -> Option<bool> {
        let expected = match expected.trim() {
            "true" => true,
            "false" => false,
            _ => return None,
        };
        let byte = compared_input_byte(operand, owner)?;
        let current = byte == self.new;
        (current == expected).then_some(current != (byte == self.old))
    }
}

fn sole_function(source: &str) -> Option<ast::Fn> {
    let parsed = parse_clean_source_file(source)?;
    let mut items = parsed.tree().syntax().children();
    let function = ast::Fn::cast(items.next()?)?;
    items.next().is_none().then_some(function)
}

fn parse_expression(text: &str) -> Option<ast::Expr> {
    let body = sole_function(&format!("fn expression() {{ {text} }}"))?
        .body()?
        .stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    body.tail_expr()
}

fn compared_byte(expression: ast::Expr, parameter: &str) -> Option<(u8, SyntaxToken)> {
    let ast::Expr::BinExpr(comparison) = expression else {
        return None;
    };
    if comparison.op_kind()? != ast::BinaryOp::CmpOp(ast::CmpOp::Eq { negated: false }) {
        return None;
    }
    let left = comparison.lhs()?;
    let right = comparison.rhs()?;
    let peek = format!("{parameter}.get(1)");
    let is_peek = |operand: &ast::Expr| operand.syntax().text() == peek.as_str();
    let constructor = match (is_peek(&left), is_peek(&right)) {
        (true, false) => right,
        (false, true) => left,
        _ => return None,
    };
    let ast::Expr::CallExpr(some) = constructor else {
        return None;
    };
    if some.expr()?.syntax().text() != "Some" {
        return None;
    }
    let mut args = some.arg_list()?.args();
    let ast::Expr::RefExpr(reference) = args.next()? else {
        return None;
    };
    if args.next().is_some() || reference.mut_token().is_some() || reference.raw_token().is_some() {
        return None;
    }
    let byte = byte_literal(reference.expr()?)?;
    Some((byte.value().ok()?, byte.syntax().clone()))
}

fn byte_literal(expression: ast::Expr) -> Option<ast::Byte> {
    let ast::Expr::Literal(literal) = expression else {
        return None;
    };
    let ast::LiteralKind::Byte(byte) = literal.kind() else {
        return None;
    };
    (literal.syntax().text() == byte.syntax().text()).then_some(byte)
}

fn compared_input_byte(operand: &str, owner: &str) -> Option<u8> {
    let ast::Expr::CallExpr(call) = parse_expression(operand)? else {
        return None;
    };
    if call.expr()?.syntax().text() != owner {
        return None;
    }
    let mut args = call.arg_list()?.args();
    let (Some(ast::Expr::Literal(literal)), None) = (args.next(), args.next()) else {
        return None;
    };
    let ast::LiteralKind::ByteString(bytes) = literal.kind() else {
        return None;
    };
    if literal.syntax().text() != bytes.syntax().text() {
        return None;
    }
    bytes.value().ok()?.get(1).copied()
}
