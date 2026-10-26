//! A closed evaluator for the doctest gate (RIPR-SPEC-0238 rule 26).
//!
//! A doctest example credits an edit only when running the example against the
//! old and the new function really gives different results. This module runs
//! the one thing it can run exactly: a function whose body is a single
//! `return <expression>` over its parameters, with `int`, `bool`, `str` and
//! `list[str]` values and the operations below. Anything else is `None`, which
//! the caller reads as "not decided", never as "equal" or "different".
//!
//! Strings are restricted to printable ASCII (0x20..=0x7e). On that alphabet
//! `lower`, `upper`, `strip` and `split()` mean the same in Python and in
//! Rust, so no Unicode case or whitespace rule is approximated. Integers are
//! exact (`i128`, checked), and `//` and `%` follow Python's floor semantics.

use rustpython_parser::ast::{self, Constant, Expr, Stmt};

/// What an expression evaluates to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Int(i128),
    Bool(bool),
    Str(String),
    /// The result of `str.split()`; only `str.join` and `len` read it.
    Words(Vec<String>),
}

impl Value {
    /// The text Python's `repr` gives, for the value kinds a doctest compares.
    pub(super) fn repr(&self) -> Option<String> {
        match self {
            Self::Int(value) => Some(value.to_string()),
            Self::Bool(true) => Some("True".to_string()),
            Self::Bool(false) => Some("False".to_string()),
            Self::Str(value) => {
                // A string holding both quote kinds repr-escapes one of them,
                // which the doctest reader excludes with backslashes.
                if value.contains('\'') && value.contains('"') {
                    None
                } else if value.contains('\'') {
                    Some(format!("\"{value}\""))
                } else {
                    Some(format!("'{value}'"))
                }
            }
            Self::Words(_) => None,
        }
    }
}

/// The parameters and the one returned expression of a function.
pub(super) struct Function<'a> {
    parameters: Vec<String>,
    expression: &'a Expr,
}

impl<'a> Function<'a> {
    /// A plain `def` with positional parameters only, no decorator and a body
    /// of an optional docstring and one `return`.
    pub(super) fn of(function: &'a ast::StmtFunctionDef) -> Option<Self> {
        let arguments = &function.args;
        if !function.decorator_list.is_empty()
            || !arguments.kwonlyargs.is_empty()
            || arguments.vararg.is_some()
            || arguments.kwarg.is_some()
            || arguments.posonlyargs.len() + arguments.args.len() == 0
            || arguments
                .posonlyargs
                .iter()
                .chain(&arguments.args)
                .any(|argument| argument.default.is_some())
        {
            return None;
        }
        let parameters: Vec<String> = arguments
            .posonlyargs
            .iter()
            .chain(&arguments.args)
            .map(|argument| argument.def.arg.to_string())
            .collect();
        let mut body = function.body.iter();
        let first = body.next()?;
        let returned = match first {
            Stmt::Expr(expression)
                if matches!(
                    expression.value.as_ref(),
                    Expr::Constant(constant) if matches!(constant.value, Constant::Str(_))
                ) =>
            {
                body.next()?
            }
            other => other,
        };
        if body.next().is_some() {
            return None;
        }
        let Stmt::Return(returned) = returned else {
            return None;
        };
        Some(Self {
            parameters,
            expression: returned.value.as_deref()?,
        })
    }

    /// The function's result for these arguments, in parameter order.
    pub(super) fn call(&self, arguments: &[Value]) -> Option<Value> {
        if arguments.len() != self.parameters.len() {
            return None;
        }
        evaluate(self.expression, &|name| {
            self.parameters
                .iter()
                .position(|parameter| parameter == name)
                .map(|index| arguments[index].clone())
        })
    }
}

/// A literal call argument or expected value.
pub(super) fn literal(expression: &Expr) -> Option<Value> {
    match expression {
        Expr::Constant(constant) => match &constant.value {
            Constant::Int(value) => value.to_string().parse().ok().map(Value::Int),
            Constant::Bool(value) => Some(Value::Bool(*value)),
            Constant::Str(value) => printable_ascii(value).then(|| Value::Str(value.clone())),
            _ => None,
        },
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::USub) => {
            match literal(&unary.operand)? {
                Value::Int(value) => value.checked_neg().map(Value::Int),
                _ => None,
            }
        }
        _ => None,
    }
}

fn printable_ascii(text: &str) -> bool {
    text.chars().all(|ch| (' '..='~').contains(&ch))
}

fn evaluate(expression: &Expr, lookup: &dyn Fn(&str) -> Option<Value>) -> Option<Value> {
    match expression {
        Expr::Constant(_) => literal(expression),
        Expr::Name(name) => lookup(name.id.as_str()),
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::USub) => {
            match evaluate(&unary.operand, lookup)? {
                Value::Int(value) => value.checked_neg().map(Value::Int),
                _ => None,
            }
        }
        Expr::BinOp(binary) => {
            let left = evaluate(&binary.left, lookup)?;
            let right = evaluate(&binary.right, lookup)?;
            arithmetic(&binary.op, left, right)
        }
        Expr::Subscript(subscript) => {
            let Value::Str(text) = evaluate(&subscript.value, lookup)? else {
                return None;
            };
            match subscript.slice.as_ref() {
                Expr::Slice(slice) => {
                    if slice.step.is_some() {
                        return None;
                    }
                    let bound = |bound: &Option<Box<Expr>>| -> Option<Option<i128>> {
                        match bound {
                            None => Some(None),
                            Some(expression) => match evaluate(expression, lookup)? {
                                Value::Int(value) => Some(Some(value)),
                                _ => None,
                            },
                        }
                    };
                    slice_of(&text, bound(&slice.lower)?, bound(&slice.upper)?)
                }
                index => match evaluate(index, lookup)? {
                    Value::Int(index) => character_at(&text, index),
                    _ => None,
                },
            }
        }
        Expr::Call(call) => call_of(call, lookup),
        _ => None,
    }
}

fn call_of(call: &ast::ExprCall, lookup: &dyn Fn(&str) -> Option<Value>) -> Option<Value> {
    if !call.keywords.is_empty() {
        return None;
    }
    match call.func.as_ref() {
        // `len(text)` and `len(words)`; a name `len` the function parameters
        // shadow is not the builtin.
        Expr::Name(name) if name.id.as_str() == "len" => {
            if lookup("len").is_some() {
                return None;
            }
            let [argument] = call.args.as_slice() else {
                return None;
            };
            match evaluate(argument, lookup)? {
                Value::Str(text) => i128::try_from(text.chars().count()).ok().map(Value::Int),
                Value::Words(words) => i128::try_from(words.len()).ok().map(Value::Int),
                _ => None,
            }
        }
        Expr::Attribute(attribute) => {
            let receiver = evaluate(&attribute.value, lookup)?;
            let method = attribute.attr.as_str();
            match (receiver, method, call.args.as_slice()) {
                (Value::Str(text), "lower", []) => Some(Value::Str(text.to_ascii_lowercase())),
                (Value::Str(text), "upper", []) => Some(Value::Str(text.to_ascii_uppercase())),
                (Value::Str(text), "strip", []) => Some(Value::Str(text.trim_matches(' ').into())),
                (Value::Str(text), "lstrip", []) => {
                    Some(Value::Str(text.trim_start_matches(' ').into()))
                }
                (Value::Str(text), "rstrip", []) => {
                    Some(Value::Str(text.trim_end_matches(' ').into()))
                }
                (Value::Str(text), "split", []) => Some(Value::Words(
                    text.split(' ')
                        .filter(|word| !word.is_empty())
                        .map(str::to_string)
                        .collect(),
                )),
                (Value::Str(separator), "join", [argument]) => match evaluate(argument, lookup)? {
                    Value::Words(words) => Some(Value::Str(words.join(&separator))),
                    _ => None,
                },
                (Value::Str(text), "replace", [old, new]) => {
                    let (Value::Str(old), Value::Str(new)) =
                        (evaluate(old, lookup)?, evaluate(new, lookup)?)
                    else {
                        return None;
                    };
                    // `"".replace("", x)` inserts between characters; the empty
                    // pattern is a different operation this reader excludes.
                    (!old.is_empty()).then(|| Value::Str(text.replace(&old, &new)))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn arithmetic(op: &ast::Operator, left: Value, right: Value) -> Option<Value> {
    match (op, left, right) {
        (ast::Operator::Add, Value::Str(left), Value::Str(right)) => {
            Some(Value::Str(format!("{left}{right}")))
        }
        (op, Value::Int(left), Value::Int(right)) => match op {
            ast::Operator::Add => left.checked_add(right).map(Value::Int),
            ast::Operator::Sub => left.checked_sub(right).map(Value::Int),
            ast::Operator::Mult => left.checked_mul(right).map(Value::Int),
            ast::Operator::FloorDiv => floor_div(left, right).map(Value::Int),
            ast::Operator::Mod => floor_mod(left, right).map(Value::Int),
            _ => None,
        },
        _ => None,
    }
}

/// Python's `//`: the quotient rounded toward negative infinity.
fn floor_div(left: i128, right: i128) -> Option<i128> {
    if right == 0 {
        return None;
    }
    let quotient = left.checked_div(right)?;
    let remainder = left.checked_rem(right)?;
    if remainder != 0 && ((remainder < 0) != (right < 0)) {
        quotient.checked_sub(1)
    } else {
        Some(quotient)
    }
}

/// Python's `%`: the remainder takes the divisor's sign.
fn floor_mod(left: i128, right: i128) -> Option<i128> {
    if right == 0 {
        return None;
    }
    let remainder = left.checked_rem(right)?;
    if remainder != 0 && ((remainder < 0) != (right < 0)) {
        remainder.checked_add(right)
    } else {
        Some(remainder)
    }
}

/// `text[lower:upper]` with Python's index normalization.
fn slice_of(text: &str, lower: Option<i128>, upper: Option<i128>) -> Option<Value> {
    let characters: Vec<char> = text.chars().collect();
    let length = i128::try_from(characters.len()).ok()?;
    let clamp = |bound: Option<i128>, default: i128| -> i128 {
        match bound {
            None => default,
            Some(index) if index < 0 => (index + length).max(0),
            Some(index) => index.min(length),
        }
    };
    let start = clamp(lower, 0);
    let stop = clamp(upper, length);
    if start >= stop {
        return Some(Value::Str(String::new()));
    }
    let start = usize::try_from(start).ok()?;
    let stop = usize::try_from(stop).ok()?;
    Some(Value::Str(characters[start..stop].iter().collect()))
}

/// `text[index]`; an index out of range raises, which decides nothing.
fn character_at(text: &str, index: i128) -> Option<Value> {
    let characters: Vec<char> = text.chars().collect();
    let length = i128::try_from(characters.len()).ok()?;
    let resolved = if index < 0 { index + length } else { index };
    if resolved < 0 || resolved >= length {
        return None;
    }
    let character = characters.get(usize::try_from(resolved).ok()?)?;
    Some(Value::Str(character.to_string()))
}
