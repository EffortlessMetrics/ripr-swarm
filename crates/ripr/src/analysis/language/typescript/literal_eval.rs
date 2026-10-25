//! Literal-input evaluation of a changed `return` expression (RIPR-SPEC-0243
//! rule 18).
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
/// same value for the literal `arguments` bound to `params` AND are not
/// equivalent everywhere: some nearby probe input (each numeric argument
/// shifted by one, zero, one, minus one) separates them. A behavior-preserving
/// rewrite (`a + b` to `b + a`) never separates, so it returns `None` and the
/// caller keeps the existing verdict. `Some(false)` when they differ on the
/// asserted inputs; `None` when either side or any argument is outside the
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
    let (old_text, new_text) = (old.clone(), new.clone());
    let params = params.to_vec();
    let arguments = arguments.to_vec();
    parse_on_worker(file, "", move |file, _source, allocator| {
        let old = parse_expression(file, allocator, &old)?;
        let new = parse_expression(file, allocator, &new)?;
        let mut values = Vec::new();
        for argument in &arguments {
            let parsed = parse_expression(file, allocator, argument)?;
            values.push(evaluate(&parsed.expression, &Vec::new(), 0)?);
        }
        let env: Env = params.iter().cloned().zip(values.iter().cloned()).collect();
        if !same_value(
            &evaluate(&old.expression, &env, 0)?,
            &evaluate(&new.expression, &env, 0)?,
        ) {
            return Some(false);
        }
        // The asserted input and its neighbours may sit far from the changed
        // constant (`Math.min(100, ...)` edited to `200`, asserted at 40), so
        // a second grid also tries each number the edit added or removed. An
        // operator edit (`>=` to `>`) changes no number and keeps the
        // boundary-witness reading instead.
        let (old_numbers, new_numbers) = (numeric_literals(&old_text), numeric_literals(&new_text));
        let constants: Vec<f64> = old_numbers
            .iter()
            .filter(|n| !new_numbers.contains(n))
            .chain(new_numbers.iter().filter(|n| !old_numbers.contains(n)))
            .copied()
            .collect();
        (separated_on_probe_grid(&old.expression, &new.expression, &params, &values, &[])
            || (!constants.is_empty()
                && separated_on_probe_grid(
                    &old.expression,
                    &new.expression,
                    &params,
                    &values,
                    &constants,
                )))
        .then_some(true)
    })
    .ok()
    .flatten()
}

/// Whether some probe input makes the two expressions evaluate to different
/// values. Only numeric arguments are varied; others keep their asserted
/// value. Bounded so an owner with many parameters stays cheap.
fn separated_on_probe_grid(
    old: &Expression<'_>,
    new: &Expression<'_>,
    params: &[String],
    values: &[Val],
    constants: &[f64],
) -> bool {
    const MAX_COMBINATIONS: usize = 4096;
    let axes: Vec<Vec<Val>> = values
        .iter()
        .map(|value| match value {
            Val::Num(n) => {
                let mut axis = Vec::new();
                let named = constants.iter().flat_map(|c| [*c, c - 1.0, c + 1.0]);
                for candidate in [*n, n - 1.0, n + 1.0, 0.0, 1.0, -1.0]
                    .into_iter()
                    .chain(named)
                {
                    if !axis.contains(&Val::Num(candidate)) {
                        axis.push(Val::Num(candidate));
                    }
                }
                axis
            }
            other => vec![other.clone()],
        })
        .collect();
    let mut combinations: usize = 1;
    for axis in &axes {
        combinations = combinations.saturating_mul(axis.len());
    }
    if combinations > MAX_COMBINATIONS {
        return false;
    }
    for index in 0..combinations {
        let mut rest = index;
        let mut env: Env = Vec::new();
        for (param, axis) in params.iter().zip(&axes) {
            env.push((param.clone(), axis[rest % axis.len()].clone()));
            rest /= axis.len();
        }
        let (Some(a), Some(b)) = (evaluate(old, &env, 0), evaluate(new, &env, 0)) else {
            continue;
        };
        if a != b {
            return true;
        }
    }
    false
}

/// The decimal number literals in `text`, at most `MAX_CONSTANTS` of them,
/// so the second probe grid stays within its combination bound.
fn numeric_literals(text: &str) -> Vec<f64> {
    const MAX_CONSTANTS: usize = 3;
    let mut found: Vec<f64> = Vec::new();
    let mut token = String::new();
    let flush = |token: &mut String, found: &mut Vec<f64>| {
        if let Ok(number) = token.parse::<f64>()
            && number.is_finite()
            && !found.contains(&number)
            && found.len() < MAX_CONSTANTS
        {
            found.push(number);
        }
        token.clear();
    };
    let mut previous_word = false;
    for c in text.chars().chain(std::iter::once(' ')) {
        let word = c == '_' || c == '$' || c.is_alphanumeric();
        if (c.is_ascii_digit() || (c == '.' && !token.is_empty()))
            && (!token.is_empty() || !previous_word)
        {
            token.push(c);
        } else {
            flush(&mut token, &mut found);
        }
        previous_word = word;
    }
    found
}

/// `Object.is` equality: `0` and `-0` differ, so a sign-only change is never
/// reported as agreeing.
fn same_value(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Num(x), Val::Num(y)) => !x.is_nan() && x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

struct Parsed<'a> {
    expression: Expression<'a>,
}

fn parse_expression<'a>(
    file: &Path,
    allocator: &'a oxc_allocator::Allocator,
    text: &str,
) -> Option<Parsed<'a>> {
    let snippet = format!("({});", text.trim());
    let snippet: &str = allocator.alloc_str(&snippet);
    let ret = Parser::new(allocator, snippet, source_type_for(file)).parse();
    if !ret.errors.is_empty() || ret.program.body.len() != 1 {
        return None;
    }
    let mut body = ret.program.body;
    match body.pop()? {
        Statement::ExpressionStatement(statement) => Some(Parsed {
            expression: statement.unbox().expression,
        }),
        _ => None,
    }
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
    fn a_changed_bound_far_from_the_asserted_input_still_separates() {
        let old = "Math.min(100, Math.max(0, n))";
        let new = "Math.min(200, Math.max(0, n))";
        // Asserted at 40: both clamp to 40, and only an input past the old
        // bound tells them apart, which the named constants supply.
        assert_eq!(agree(old, new, &["n"], &["40"]), Some(true));
        assert_eq!(agree(old, new, &["n"], &["250"]), Some(false));
    }

    #[test]
    fn an_operator_edit_names_no_new_number_and_stays_unknown_far_from_the_bound() {
        assert_eq!(
            agree("total >= 100", "total > 100", &["total"], &["50"]),
            None
        );
        assert_eq!(
            agree("total >= 100", "total > 100", &["total"], &["20"]),
            None
        );
        assert_eq!(
            agree("total >= 100", "total > 100", &["total"], &["100"]),
            Some(false)
        );
    }

    #[test]
    fn an_equivalent_rewrite_is_still_not_reported_as_unexercised() {
        assert_eq!(
            agree("Math.min(100, n)", "Math.min(n, 100)", &["n"], &["40"]),
            None
        );
        // The literals differ (2 against 1 and 1), so the changed-number
        // probes run, and the values still agree everywhere.
        assert_eq!(agree("n + 2", "n + 1 + 1", &["n"], &["40"]), None);
    }

    #[test]
    fn numeric_literals_skip_identifier_digits_and_cap_the_count() {
        assert_eq!(numeric_literals("a1 + b2 * 3.5 - 7"), vec![3.5, 7.0]);
        assert_eq!(numeric_literals("1 2 3 4 5").len(), 3);
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
    fn behavior_preserving_rewrites_are_not_reported_as_unexercised() {
        assert_eq!(agree("a * 3 + 30", "30 + a * 3", &["a"], &["100"]), None);
        assert_eq!(
            agree(
                "`Hello, ${n}!`",
                "\"Hello, \" + n + \"!\"",
                &["n"],
                &["\"Ada\""]
            ),
            None
        );
    }

    #[test]
    fn a_sign_only_zero_change_does_not_agree() {
        assert_eq!(agree("n * 0", "n * -0", &["n"], &["5"]), Some(false));
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
