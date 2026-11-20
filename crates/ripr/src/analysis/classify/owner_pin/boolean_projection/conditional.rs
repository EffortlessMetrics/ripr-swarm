//! A Boolean owner call selects a primitive literal or the presence of one.
//! Carry both branch outcomes so even permanently false equalities can
//! participate in the whole-test contradiction check.

use super::{ComparedCall, OracleFact, OwnerPinSyntax, RustIndex, TestSummary};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs},
};

pub(in crate::analysis::classify) fn has_conditional_operand(compared: [&str; 2]) -> bool {
    compared.into_iter().any(is_conditional)
}

fn is_conditional(side: &str) -> bool {
    let side = side.trim_start();
    side.starts_with("if ") || side.starts_with("match ") || side.contains(".then_some(")
}

pub(super) fn comparison(
    [left, right]: [&str; 2],
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<ComparedCall> {
    let (projection, expected) = if is_conditional(left) {
        (left, right)
    } else {
        (right, left)
    };
    let (call, when_false, when_true) = match operand(projection)? {
        ast::Expr::IfExpr(branch) => {
            let ast::ElseBranch::Block(other) = branch.else_branch()? else {
                return None;
            };
            let expected = literal(operand(expected)?)?;
            let yes = literal_block(branch.then_branch()?)?;
            let no = literal_block(other)?;
            (
                branch.condition()?,
                no.agrees(&expected)?,
                yes.agrees(&expected)?,
            )
        }
        ast::Expr::MatchExpr(branch) => {
            let expected = literal(operand(expected)?)?;
            let mut no = None;
            let mut yes = None;
            for arm in branch.match_arm_list()?.arms() {
                if arm.guard().is_some() || arm.attrs().next().is_some() {
                    return None;
                }
                let slot = match arm.pat()?.syntax().text().to_string().as_str() {
                    "true" => &mut yes,
                    "false" => &mut no,
                    _ => return None,
                };
                if slot
                    .replace(literal(arm.expr()?)?.agrees(&expected)?)
                    .is_some()
                {
                    return None;
                }
            }
            (branch.expr()?, no?, yes?)
        }
        ast::Expr::MethodCallExpr(method) => {
            if method.name_ref()?.text() != "then_some" || method.generic_arg_list().is_some() {
                return None;
            }
            let args = method.arg_list()?;
            let mut args = args.args();
            let carried = literal(args.next()?)?;
            if args.next().is_some() {
                return None;
            }
            let (absent, present) = if expected.trim() == "None" {
                if !super::standard_variant_in_scope("None", test, assertion, index, syntax) {
                    return None;
                }
                (true, false)
            } else {
                if !super::standard_variant_in_scope("Some", test, assertion, index, syntax) {
                    return None;
                }
                let expected = literal(operand(super::some_payload(expected)?)?)?;
                (false, carried.agrees(&expected)?)
            };
            (method.receiver()?, absent, present)
        }
        _ => return None,
    };
    let ast::Expr::CallExpr(call) = call else {
        return None;
    };
    Some(ComparedCall {
        call: call.syntax().text().to_string(),
        when_false,
        when_true,
    })
}

fn operand(text: &str) -> Option<ast::Expr> {
    let text = text.trim();
    let expression = super::super::super::owner_value_sink::parse_expression(text)?;
    (expression.syntax().text() == text
        && !expression
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind())))
    .then_some(expression)
}

fn literal_block(block: ast::BlockExpr) -> Option<Literal> {
    let list = block.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    literal(list.tail_expr()?)
}

#[derive(PartialEq)]
enum Literal {
    Int(i32),
    Char(char),
    Bool(bool),
}

impl Literal {
    fn agrees(&self, expected: &Self) -> Option<bool> {
        (std::mem::discriminant(self) == std::mem::discriminant(expected)).then(|| self == expected)
    }
}

fn literal(expression: ast::Expr) -> Option<Literal> {
    let ast::Expr::Literal(value) = expression else {
        return None;
    };
    if value.attrs().next().is_some() {
        return None;
    }
    match value.kind() {
        ast::LiteralKind::IntNumber(number) if number.suffix().is_none() => {
            Some(Literal::Int(i32::try_from(number.value().ok()?).ok()?))
        }
        ast::LiteralKind::Char(ch) => Some(Literal::Char(ch.value().ok()?)),
        ast::LiteralKind::Bool(value) => Some(Literal::Bool(value)),
        _ => None,
    }
}
