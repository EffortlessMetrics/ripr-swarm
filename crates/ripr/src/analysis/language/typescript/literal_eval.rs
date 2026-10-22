//! Literal-input evaluation of a changed `return` expression (RIPR-SPEC-0243
//! rule 16).
//!
//! A strong exact assertion on an owner call only discriminates a changed
//! `return` expression when the call's inputs make the old and new
//! expressions produce different values. When every input is a literal and
//! the expression is built from literals, owner parameters, arithmetic,
//! comparisons, conditionals and `Math.min` / `Math.max` / `Math.abs` /
//! `Math.floor` / `Math.ceil` / `Math.trunc`, both sides can be evaluated
//! exactly. Anything else (a free identifier, a call, a non-literal
//! argument, mixed-type coercion) is unsupported and evaluates to `None`, so
//! the caller keeps the existing verdict: this evaluator can only withdraw
//! credit by showing two expressions agree, never grant any.

use super::*;
use oxc_ast::ast::{BinaryOperator, LogicalOperator, UnaryOperator};

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Num(f64),
    Str(String),
    Bool(bool),
}

type Env = Vec<(String, Val)>;

/// `Some(true)` when the old and new `return` expressions evaluate to the
/// same value for the literal `arguments` bound to `params`; `Some(false)`
/// when they differ; `None` when either side or any argument is outside the
/// supported subset.
pub(crate) fn return_expressions_agree(
    file: &Path,
    old_expression: &str,
    new_expression: &str,
    params: &[String],
    arguments: &[String],
) -> Option<bool> {
    if params.is_empty() || params.len() != arguments.len() {
        return None;
    }
    let old = old_expression.to_string();
    let new = new_expression.to_string();
    let params = params.to_vec();
    let arguments = arguments.to_vec();
    parse_on_worker(file, "", move |file, _source, allocator| {
        let mut env: Env = Vec::new();
        for (param, argument) in params.iter().zip(&arguments) {
            let value = evaluate_text(file, allocator, argument, &Vec::new())?;
            env.push((param.clone(), value));
        }
        let old = evaluate_text(file, allocator, &old, &env)?;
        let new = evaluate_text(file, allocator, &new, &env)?;
        Some(old == new)
    })
    .ok()
    .flatten()
}

fn evaluate_text(
    file: &Path,
    allocator: &oxc_allocator::Allocator,
    text: &str,
    env: &Env,
) -> Option<Val> {
    let snippet = format!("({});", text.trim());
    let snippet: &str = allocator.alloc_str(&snippet);
    let ret = Parser::new(allocator, snippet, source_type_for(file)).parse();
    if !ret.errors.is_empty() || ret.program.body.len() != 1 {
        return None;
    }
    let Statement::ExpressionStatement(statement) = &ret.program.body[0] else {
        return None;
    };
    evaluate(&statement.expression, env, 0)
}

const MAX_DEPTH: usize = 32;

fn evaluate(expression: &Expression<'_>, env: &Env, depth: usize) -> Option<Val> {
    if depth > MAX_DEPTH {
        return None;
    }
    let next = depth + 1;
    match expression {
        Expression::ParenthesizedExpression(inner) => evaluate(&inner.expression, env, next),
        Expression::NumericLiteral(literal) => Some(Val::Num(literal.value)),
        Expression::StringLiteral(literal) => Some(Val::Str(literal.value.to_string())),
        Expression::BooleanLiteral(literal) => Some(Val::Bool(literal.value)),
        Expression::Identifier(identifier) => env
            .iter()
            .find(|(name, _)| name == identifier.name.as_str())
            .map(|(_, value)| value.clone()),
        Expression::TemplateLiteral(template) => {
            let mut out = String::new();
            for (index, quasi) in template.quasis.iter().enumerate() {
                out.push_str(quasi.value.cooked.as_ref()?.as_str());
                if let Some(substitution) = template.expressions.get(index) {
                    out.push_str(&to_text(&evaluate(substitution, env, next)?)?);
                }
            }
            Some(Val::Str(out))
        }
        Expression::UnaryExpression(unary) => {
            let operand = evaluate(&unary.argument, env, next)?;
            match (unary.operator, operand) {
                (UnaryOperator::UnaryNegation, Val::Num(n)) => finite(-n),
                (UnaryOperator::UnaryPlus, Val::Num(n)) => finite(n),
                (UnaryOperator::LogicalNot, Val::Bool(b)) => Some(Val::Bool(!b)),
                _ => None,
            }
        }
        Expression::BinaryExpression(binary) => {
            let left = evaluate(&binary.left, env, next)?;
            let right = evaluate(&binary.right, env, next)?;
            binary_value(binary.operator, left, right)
        }
        Expression::LogicalExpression(logical) => {
            let Val::Bool(left) = evaluate(&logical.left, env, next)? else {
                return None;
            };
            match logical.operator {
                LogicalOperator::And if !left => Some(Val::Bool(false)),
                LogicalOperator::Or if left => Some(Val::Bool(true)),
                LogicalOperator::And | LogicalOperator::Or => {
                    match evaluate(&logical.right, env, next)? {
                        Val::Bool(right) => Some(Val::Bool(right)),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        Expression::ConditionalExpression(conditional) => {
            let Val::Bool(test) = evaluate(&conditional.test, env, next)? else {
                return None;
            };
            // Lazy: only the selected arm needs to be supported.
            if test {
                evaluate(&conditional.consequent, env, next)
            } else {
                evaluate(&conditional.alternate, env, next)
            }
        }
        Expression::CallExpression(call) => math_call(call, env, next),
        _ => None,
    }
}

fn finite(value: f64) -> Option<Val> {
    value.is_finite().then_some(Val::Num(value))
}

fn to_text(value: &Val) -> Option<String> {
    match value {
        Val::Str(text) => Some(text.clone()),
        Val::Bool(flag) => Some(flag.to_string()),
        Val::Num(number) if number.fract() == 0.0 && number.abs() < 1e15 => {
            Some(format!("{}", *number as i64))
        }
        Val::Num(_) => None,
    }
}

fn binary_value(operator: BinaryOperator, left: Val, right: Val) -> Option<Val> {
    use BinaryOperator as Op;
    match (operator, left, right) {
        (Op::Addition, Val::Num(a), Val::Num(b)) => finite(a + b),
        (Op::Addition, Val::Str(a), Val::Str(b)) => Some(Val::Str(a + &b)),
        (Op::Subtraction, Val::Num(a), Val::Num(b)) => finite(a - b),
        (Op::Multiplication, Val::Num(a), Val::Num(b)) => finite(a * b),
        (Op::Division, Val::Num(a), Val::Num(b)) => finite(a / b),
        (Op::Remainder, Val::Num(a), Val::Num(b)) => finite(a % b),
        (Op::StrictEquality | Op::Equality, a, b) => same_type(&a, &b).then(|| Val::Bool(a == b)),
        (Op::StrictInequality | Op::Inequality, a, b) => {
            same_type(&a, &b).then(|| Val::Bool(a != b))
        }
        (Op::LessThan, Val::Num(a), Val::Num(b)) => Some(Val::Bool(a < b)),
        (Op::LessEqualThan, Val::Num(a), Val::Num(b)) => Some(Val::Bool(a <= b)),
        (Op::GreaterThan, Val::Num(a), Val::Num(b)) => Some(Val::Bool(a > b)),
        (Op::GreaterEqualThan, Val::Num(a), Val::Num(b)) => Some(Val::Bool(a >= b)),
        _ => None,
    }
}

fn same_type(a: &Val, b: &Val) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn math_call(call: &oxc_ast::ast::CallExpression<'_>, env: &Env, depth: usize) -> Option<Val> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(object) = &member.object else {
        return None;
    };
    // A parameter named `Math` would shadow the global.
    if object.name != "Math" || env.iter().any(|(name, _)| name == "Math") {
        return None;
    }
    let mut numbers = Vec::new();
    for argument in &call.arguments {
        match evaluate(argument.as_expression()?, env, depth)? {
            Val::Num(number) => numbers.push(number),
            _ => return None,
        }
    }
    match (member.property.name.as_str(), numbers.as_slice()) {
        ("min", [first, rest @ ..]) => finite(rest.iter().fold(*first, |acc, n| acc.min(*n))),
        ("max", [first, rest @ ..]) => finite(rest.iter().fold(*first, |acc, n| acc.max(*n))),
        ("abs", [n]) => finite(n.abs()),
        ("floor", [n]) => finite(n.floor()),
        ("ceil", [n]) => finite(n.ceil()),
        ("trunc", [n]) => finite(n.trunc()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agree(old: &str, new: &str, params: &[&str], args: &[&str]) -> Option<bool> {
        let params: Vec<String> = params.iter().map(|p| p.to_string()).collect();
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        return_expressions_agree(Path::new("owner.ts"), old, new, &params, &args)
    }

    #[test]
    fn clamp_shift_agrees_off_the_lower_arm_and_differs_on_it() {
        let old = "Math.min(hi, Math.max(lo, n))";
        let new = "Math.min(hi, Math.max(lo + 1, n))";
        let params = ["n", "lo", "hi"];
        assert_eq!(agree(old, new, &params, &["5", "0", "10"]), Some(true));
        assert_eq!(agree(old, new, &params, &["15", "0", "10"]), Some(true));
        assert_eq!(agree(old, new, &params, &["0", "0", "10"]), Some(false));
        assert_eq!(agree(old, new, &params, &["-3", "0", "10"]), Some(false));
    }

    #[test]
    fn ternary_agrees_only_on_the_unchanged_arm() {
        let old = "score >= 90 ? \"gold\" : \"silver\"";
        let new = "score >= 90 ? \"gold\" : \"bronze\"";
        assert_eq!(agree(old, new, &["score"], &["90"]), Some(true));
        assert_eq!(agree(old, new, &["score"], &["89"]), Some(false));
    }

    #[test]
    fn equality_to_inclusive_bound_agrees_at_and_above_the_literal() {
        let old = "attempt === 0 ? \"first\" : `retry ${attempt}`";
        let new = "attempt <= 0 ? \"first\" : `retry ${attempt}`";
        assert_eq!(agree(old, new, &["attempt"], &["0"]), Some(true));
        assert_eq!(agree(old, new, &["attempt"], &["2"]), Some(true));
        assert_eq!(agree(old, new, &["attempt"], &["-1"]), Some(false));
    }

    #[test]
    fn unsupported_inputs_stay_unknown() {
        let params = ["n"];
        // free identifier, call, non-literal argument, arity mismatch
        assert_eq!(agree("n + LIMIT", "n + LIMIT + 1", &params, &["1"]), None);
        assert_eq!(agree("f(n)", "f(n) + 1", &params, &["1"]), None);
        assert_eq!(agree("n + 1", "n + 2", &params, &["seed"]), None);
        assert_eq!(agree("n + 1", "n + 2", &params, &[]), None);
        // mixed-type coercion is not modelled
        assert_eq!(agree("n + 1", "n + 2", &params, &["\"a\""]), None);
    }
}
