//! Static evaluation of a related test's exact-value assertion against the
//! seam owner's body (#6026).
//!
//! A related test that asserts `assert_eq!(owner(literal, ..), literal)` is
//! credited as a strong `exact_value` oracle even when the asserted literal
//! is statically wrong: both the arguments and the expected value are
//! literals, and the owner is frequently one pure integer expression that a
//! constant fold can evaluate. The wrong-value assert is a discriminator
//! pointing the wrong way — it fails at baseline and would pass under the
//! mutation the seam carries — yet static evidence closed the gap on it and
//! reported nothing weak or unknown. Per the repository invariant, a wrong
//! actionable repair signal is worse than a missed advisory finding, so the
//! fold's contradiction must be typed instead of credited.
//!
//! Scope is deliberately narrow and fail-closed toward the current credit:
//! when the owner is not a free function of pure literal arithmetic, when any
//! argument or expected expression does not fold, or when anything about the
//! shape is unexpected, the verdict is [`ExactValueVerdict::NotEvaluable`]
//! and every consumer keeps today's behavior. Only a fold that completes and
//! contradicts the asserted relation downgrades the oracle, states the
//! contradiction in the receipt, and stops closing the gap. The downgrade
//! direction is always weaker, so a fold bug cannot manufacture stronger
//! evidence — it can only withhold it.

use crate::analysis::extract::equality_assertion_arguments;
use crate::analysis::facts::FunctionSummary;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, ast, ast::HasName};
use std::collections::BTreeMap;

/// What static evaluation can say about one related test's equality
/// assertion against the seam owner's body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExactValueVerdict {
    /// The assertion's operand relation is statically false: the owner folds
    /// to `evaluated` at the asserted literal arguments while the assertion
    /// pins `expected` (equality) or excludes `expected` (inequality). The
    /// test fails at baseline and is not evidence for the seam's behavior.
    Contradicted { expected: String, evaluated: i128 },
    /// The fold completed and agrees with the asserted relation.
    Consistent,
    /// Static evaluation is not possible for this pair. No claim either way;
    /// the current behavior (and its limitation) is retained.
    NotEvaluable,
}

/// Statically evaluate one oracle's equality assertion against `owner`'s
/// body. `owner` is the seam's owning function; `oracle_text` is the
/// assertion fact's text (for example
/// `assert_eq!(discounted_total(5_000, 5_000), 5_000)`).
pub(super) fn exact_value_assertion_verdict(
    owner: Option<&FunctionSummary>,
    oracle_text: &str,
) -> ExactValueVerdict {
    let Some(owner) = owner else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some((asserts_inequality, arguments)) = equality_assertion_shape(oracle_text) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let [actual, expected, ..] = arguments.as_slice() else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some(call_arguments) = owner_call_arguments(actual, &owner.name) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let empty = BTreeMap::new();
    let mut values = Vec::with_capacity(call_arguments.len());
    for argument in &call_arguments {
        let Some(value) = fold_expression(argument, &empty) else {
            return ExactValueVerdict::NotEvaluable;
        };
        values.push(value);
    }
    let Some(expected_value) = fold_expression(expected, &empty) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some(actual_value) = fold_owner_call(owner, &values) else {
        return ExactValueVerdict::NotEvaluable;
    };
    // `assert_eq!` fails at baseline exactly when the folded values differ;
    // `assert_ne!` fails exactly when they are equal.
    let contradicted = if asserts_inequality {
        actual_value == expected_value
    } else {
        actual_value != expected_value
    };
    if contradicted {
        ExactValueVerdict::Contradicted {
            expected: expected.trim().to_string(),
            evaluated: actual_value,
        }
    } else {
        ExactValueVerdict::Consistent
    }
}

/// The one-line disclosure a receipt carries when the oracle is downgraded.
pub(super) fn contradiction_summary(expected: &str, evaluated: i128) -> String {
    format!(
        "assertion expected value contradicts static evaluation (asserts {expected}, owner folds to {evaluated})"
    )
}

/// Which equality macro family the assertion uses and its first operands.
/// `Some((true, ..))` is the `assert_ne!` family (the asserted relation is
/// inequality); `None` is not an equality assertion at all.
fn equality_assertion_shape(oracle_text: &str) -> Option<(bool, Vec<String>)> {
    let trimmed = oracle_text.trim_start();
    let starts_with_any = |names: &[&str]| {
        names.iter().any(|name| {
            trimmed.starts_with(name) || trimmed.starts_with(format!("::{name}").as_str())
        })
    };
    let asserts_inequality = starts_with_any(&["assert_ne!", "debug_assert_ne!"]);
    let asserts_equality = starts_with_any(&["assert_eq!", "debug_assert_eq!"]);
    if !asserts_equality && !asserts_inequality {
        return None;
    }
    let arguments = equality_assertion_arguments(oracle_text)?;
    Some((asserts_inequality, arguments))
}

/// The call arguments when `operand` is a bare-path call to `owner_name`
/// (`owner(..)` or `module::owner(..)`). A method call
/// (`receiver.owner(..)`) names a different entity than the free function
/// the seam owns, so it is not evaluated rather than folded against the
/// wrong body.
fn owner_call_arguments(operand: &str, owner_name: &str) -> Option<Vec<String>> {
    let trimmed = operand.trim();
    let open = super::named_call_open_paren_index(trimmed, owner_name)?;
    let before = trimmed[..open - owner_name.len()].trim_end();
    if !(before.is_empty() || before.ends_with("::")) {
        return None;
    }
    let inside = super::delimited_contents_at(trimmed, open)?;
    Some(super::split_top_level_commas(&inside))
}

/// Evaluate the owner's body as a pure integer function of `arguments`.
/// Free functions only: a method or nested fn is reached through a receiver
/// or closure that the bare-path assertion does not name.
fn fold_owner_call(owner: &FunctionSummary, arguments: &[i128]) -> Option<i128> {
    let parse = parse_clean_source_file(owner.body.as_str())?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    for ancestor in function.syntax().ancestors().skip(1) {
        if ast::Impl::can_cast(ancestor.kind())
            || ast::Trait::can_cast(ancestor.kind())
            || ast::Fn::can_cast(ancestor.kind())
        {
            return None;
        }
    }
    let param_list = function.param_list()?;
    if param_list.self_param().is_some() {
        return None;
    }
    let mut names = Vec::new();
    for param in param_list.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        // A subpattern (`x @ (a, b)`) does not bind the written name alone.
        if ident.pat().is_some() {
            return None;
        }
        names.push(ident.name()?.text().to_string());
    }
    if names.len() != arguments.len() {
        return None;
    }
    let block = function.body()?;
    let environment: BTreeMap<String, i128> =
        names.into_iter().zip(arguments.iter().copied()).collect();
    // Comments and strings carry no arithmetic; masking them first keeps the
    // tokenizer away from brace-bearing content inside either.
    let masked =
        crate::analysis::extract::mask_comments_and_strings(&block.syntax().text().to_string());
    let tokens = tokenize(&masked)?;
    fold_block(&tokens, &environment)
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Int(i128),
    Ident(String),
    Symbol(&'static str),
}

const INTEGER_SUFFIXES: [&str; 12] = [
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// Tokenize masked Rust expression text. Any byte the fold grammar cannot
/// mean (`?`, `|`, `.`, `:`), a non-suffix identifier glued to a number, or
/// a non-ASCII byte fails the whole fold.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
                index += 1;
            }
            let digits_end = index;
            if index < bytes.len() && (bytes[index].is_ascii_alphabetic() || bytes[index] == b'_') {
                let suffix_start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                if !INTEGER_SUFFIXES.contains(&&text[suffix_start..index]) {
                    return None;
                }
            }
            let digits = text[start..digits_end].replace('_', "");
            let value = digits.parse::<i128>().ok()?;
            tokens.push(Token::Int(value));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token::Ident(text[start..index].to_string()));
            continue;
        }
        let two_byte_symbol = |pair: &[u8]| match pair {
            [b'=', b'='] => "==",
            [b'!', b'='] => "!=",
            [b'<', b'='] => "<=",
            [b'>', b'='] => ">=",
            [b'&', b'&'] => "&&",
            [b'|', b'|'] => "||",
            _ => "",
        };
        let symbol = if index + 2 <= bytes.len() {
            two_byte_symbol(&bytes[index..index + 2])
        } else {
            ""
        };
        let symbol = if !symbol.is_empty() {
            symbol
        } else if !byte.is_ascii() {
            return None;
        } else {
            match byte {
                b'<' => "<",
                b'>' => ">",
                b'+' => "+",
                b'-' => "-",
                b'*' => "*",
                b'/' => "/",
                b'%' => "%",
                b'!' => "!",
                b'(' => "(",
                b')' => ")",
                b'{' => "{",
                b'}' => "}",
                b',' => ",",
                b';' => ";",
                _ => return None,
            }
        };
        index += symbol.len();
        tokens.push(Token::Symbol(symbol));
    }
    Some(tokens)
}

/// A folded value: the only two types a pure integer fold produces.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Int(i128),
    Bool(bool),
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    environment: &'a BTreeMap<String, i128>,
}

impl<'a> Parser<'a> {
    fn peek_symbol(&self) -> Option<&'static str> {
        match self.tokens.get(self.position) {
            Some(Token::Symbol(symbol)) => Some(symbol),
            _ => None,
        }
    }

    fn peek_ident(&self) -> Option<&str> {
        match self.tokens.get(self.position) {
            Some(Token::Ident(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        if self.peek_symbol() == Some(symbol) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: &str) -> Option<()> {
        self.eat_symbol(symbol).then_some(())
    }

    /// A `{ .. }` block: a sequence of expression statements and one optional
    /// trailing expression. `let` bindings, `return`, and any statement shape
    /// outside the fold leave it unevaluated instead of being guessed at.
    fn parse_block_value(&mut self) -> Option<Value> {
        self.expect_symbol("{")?;
        let mut value = None;
        loop {
            match self.tokens.get(self.position) {
                None => return None,
                Some(Token::Symbol("}")) => {
                    self.position += 1;
                    return value;
                }
                _ => {}
            }
            if matches!(self.peek_ident(), Some("let" | "return")) {
                return None;
            }
            value = Some(self.parse_expression()?);
            match self.peek_symbol() {
                Some(";") => {
                    self.position += 1;
                    value = None;
                }
                Some("}") => {}
                _ => return None,
            }
        }
    }

    fn parse_expression(&mut self) -> Option<Value> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<Value> {
        let mut left = self.parse_and()?;
        while self.eat_symbol("||") {
            let right = self.parse_and()?;
            left = Value::Bool(bool_value(left)? || bool_value(right)?);
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Value> {
        let mut left = self.parse_comparison()?;
        while self.eat_symbol("&&") {
            let right = self.parse_comparison()?;
            left = Value::Bool(bool_value(left)? && bool_value(right)?);
        }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<Value> {
        let left = self.parse_additive()?;
        let Some(op) = self.peek_symbol() else {
            return Some(left);
        };
        if !matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
            return Some(left);
        }
        self.position += 1;
        let right = self.parse_additive()?;
        match (left, right) {
            (Value::Int(left), Value::Int(right)) => {
                let result = match op {
                    "==" => left == right,
                    "!=" => left != right,
                    "<" => left < right,
                    "<=" => left <= right,
                    ">" => left > right,
                    _ => left >= right,
                };
                Some(Value::Bool(result))
            }
            (Value::Bool(left), Value::Bool(right)) if matches!(op, "==" | "!=") => {
                Some(Value::Bool(if op == "==" {
                    left == right
                } else {
                    left != right
                }))
            }
            _ => None,
        }
    }

    fn parse_additive(&mut self) -> Option<Value> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_symbol() {
                Some("+") => "+",
                Some("-") => "-",
                _ => return Some(left),
            };
            self.position += 1;
            let right = self.parse_multiplicative()?;
            let Value::Int(left_value) = left else {
                return None;
            };
            let Value::Int(right_value) = right else {
                return None;
            };
            left = Value::Int(match op {
                "+" => left_value.checked_add(right_value)?,
                _ => left_value.checked_sub(right_value)?,
            });
        }
    }

    fn parse_multiplicative(&mut self) -> Option<Value> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_symbol() {
                Some("*") => "*",
                Some("/") => "/",
                Some("%") => "%",
                _ => return Some(left),
            };
            self.position += 1;
            let right = self.parse_unary()?;
            let Value::Int(left_value) = left else {
                return None;
            };
            let Value::Int(right_value) = right else {
                return None;
            };
            left = Value::Int(match op {
                "*" => left_value.checked_mul(right_value)?,
                "/" => left_value.checked_div(right_value)?,
                _ => left_value.checked_rem(right_value)?,
            });
        }
    }

    fn parse_unary(&mut self) -> Option<Value> {
        if self.eat_symbol("-") {
            let Value::Int(value) = self.parse_unary()? else {
                return None;
            };
            return Some(Value::Int(value.checked_neg()?));
        }
        if self.eat_symbol("!") {
            let value = self.parse_unary()?;
            return Some(Value::Bool(!bool_value(value)?));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<Value> {
        match self.tokens.get(self.position) {
            Some(Token::Symbol("(")) => {
                self.position += 1;
                let value = self.parse_expression()?;
                self.expect_symbol(")")?;
                Some(value)
            }
            Some(Token::Symbol("{")) => self.parse_block_value(),
            // Keywords lex as identifiers: `if` opens the fold's only
            // control form.
            Some(Token::Ident(name)) if name == "if" => {
                self.position += 1;
                self.parse_if_tail()
            }
            Some(Token::Int(value)) => {
                let value = *value;
                self.position += 1;
                Some(Value::Int(value))
            }
            Some(Token::Ident(name)) => {
                let name = name.clone();
                self.position += 1;
                match name.as_str() {
                    "true" => return Some(Value::Bool(true)),
                    "false" => return Some(Value::Bool(false)),
                    _ => {}
                }
                // Calls, method chains, paths and field access are outside
                // the fold's scope; only bound parameters evaluate.
                match self.peek_symbol() {
                    Some("(" | "." | "::") => None,
                    _ => self.environment.get(&name).copied().map(Value::Int),
                }
            }
            _ => None,
        }
    }

    /// The `if` keyword has been consumed; parse the condition, the taken
    /// block, and (when present) the `else` arm. An `if` without `else` is
    /// evaluable only when the condition folds true.
    fn parse_if_tail(&mut self) -> Option<Value> {
        let condition = bool_value(self.parse_expression()?)?;
        let taken = self.parse_block_value()?;
        if self.peek_ident() != Some("else") {
            return if condition { Some(taken) } else { None };
        }
        self.position += 1;
        let alternative = if self.peek_ident() == Some("if") {
            self.position += 1;
            self.parse_if_tail()?
        } else {
            self.parse_block_value()?
        };
        if condition {
            Some(taken)
        } else {
            Some(alternative)
        }
    }
}

fn bool_value(value: Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(value),
        Value::Int(_) => None,
    }
}

/// Fold a standalone expression (an assertion operand) with no bound names.
fn fold_expression(text: &str, environment: &BTreeMap<String, i128>) -> Option<i128> {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let tokens = tokenize(&masked)?;
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        environment,
    };
    let value = parser.parse_expression()?;
    if parser.position != tokens.len() {
        return None;
    }
    match value {
        Value::Int(value) => Some(value),
        Value::Bool(_) => None,
    }
}

/// Fold a `{ .. }` block with `environment` as the parameter bindings.
fn fold_block(tokens: &[Token], environment: &BTreeMap<String, i128>) -> Option<i128> {
    let mut parser = Parser {
        tokens,
        position: 0,
        environment,
    };
    let value = parser.parse_block_value()?;
    if parser.position != tokens.len() {
        return None;
    }
    match value {
        Value::Int(value) => Some(value),
        Value::Bool(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};

    fn verdict_for_owner(owner: &str, assertion: &str) -> Result<ExactValueVerdict, String> {
        let adapter = RaRustSyntaxAdapter;
        let facts = adapter
            .summarize_file(std::path::Path::new("src/lib.rs"), &format!("{owner}\n"))
            .map_err(|error| format!("owner source summarizes: {error}"))?;
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "discounted_total")
            .ok_or("owner function is indexed")?;
        Ok(exact_value_assertion_verdict(Some(owner), assertion))
    }

    const BOUNDARY_OWNER: &str = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents * 70 / 100
    } else {
        amount_cents
    }
}
"#;

    #[test]
    fn wrongval_assertion_of_the_mutants_value_is_contradicted() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(5_000, 5_000), 5_000);",
        );
        assert_eq!(
            verdict?,
            ExactValueVerdict::Contradicted {
                expected: "5_000".to_string(),
                evaluated: 3_500,
            }
        );
        Ok(())
    }

    #[test]
    fn correct_boundary_literal_is_consistent() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(5_000, 5_000), 3_500);",
        );
        assert_eq!(verdict?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn non_boundary_literal_is_consistent() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(1_000, 5_000), 1_000);",
        );
        assert_eq!(verdict?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn non_literal_expected_value_is_not_evaluable() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(5_000, 5_000), expected_total());",
        );
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn non_literal_call_argument_is_not_evaluable() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(amount, 5_000), 3_500);",
        );
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn qualified_owner_path_call_folds() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(crate::discounted_total(5_000, 5_000), 5_000);",
        );
        assert!(matches!(verdict?, ExactValueVerdict::Contradicted { .. }));
        Ok(())
    }

    #[test]
    fn owner_that_calls_another_function_is_not_evaluable() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        discount_factor(amount_cents)
    } else {
        amount_cents
    }
}
fn discount_factor(amount_cents: u64) -> u64 { amount_cents * 70 / 100 }
"#;
        let verdict =
            verdict_for_owner(owner, "assert_eq!(discounted_total(5_000, 5_000), 5_000);")?;
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn method_owner_is_not_evaluable() -> Result<(), String> {
        let owner = r#"
pub struct Pricing { pub threshold: u64 }
impl Pricing {
    pub fn discounted_total(&self, amount_cents: u64) -> u64 {
        if amount_cents >= self.threshold {
            amount_cents * 70 / 100
        } else {
            amount_cents
        }
    }
}
"#;
        let verdict =
            verdict_for_owner(owner, "assert_eq!(discounted_total(5_000, 5_000), 5_000);")?;
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn method_call_operand_is_not_evaluable() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(order.discounted_total(5_000, 5_000), 5_000);",
        );
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn wrong_assert_ne_is_contradicted_and_correct_ne_is_consistent() -> Result<(), String> {
        let wrong = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_ne!(discounted_total(5_000, 5_000), 3_500);",
        );
        assert_eq!(
            wrong?,
            ExactValueVerdict::Contradicted {
                expected: "3_500".to_string(),
                evaluated: 3_500,
            }
        );
        let correct = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_ne!(discounted_total(5_000, 5_000), 5_000);",
        );
        assert_eq!(correct?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn contradiction_summary_names_both_values() {
        let summary = contradiction_summary("5_000", 3_500);
        assert!(
            summary.contains("5_000") && summary.contains("3500"),
            "{summary}"
        );
        assert!(
            summary.contains("contradicts static evaluation"),
            "{summary}"
        );
    }

    #[test]
    fn else_if_chain_folds() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents * 70 / 100
    } else if amount_cents >= discount_threshold_cents / 2 {
        amount_cents * 90 / 100
    } else {
        amount_cents
    }
}
"#;
        // 3_000 < 5_000 takes the else-if arm (3_000 >= 5_000 / 2), which
        // folds to 3_000 * 90 / 100 = 2_700.
        let contradicted =
            verdict_for_owner(owner, "assert_eq!(discounted_total(3_000, 5_000), 3_000)?;");
        assert_eq!(
            contradicted?,
            ExactValueVerdict::Contradicted {
                expected: "3_000".to_string(),
                evaluated: 2_700,
            }
        );
        let consistent =
            verdict_for_owner(owner, "assert_eq!(discounted_total(3_000, 5_000), 2_700)?;");
        assert_eq!(consistent?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn negative_and_suffixed_literals_fold() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: i64, discount_threshold_cents: i64) -> i64 {
    if amount_cents >= discount_threshold_cents {
        -amount_cents + 10i64
    } else {
        amount_cents
    }
}
"#;
        // -30 >= -100 takes the discounted arm: -(-30) + 10 = 40.
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(-30, -100), 40)?;");
        assert_eq!(verdict?, ExactValueVerdict::Consistent);
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(-30, -100), -40)?;");
        assert_eq!(
            verdict?,
            ExactValueVerdict::Contradicted {
                expected: "-40".to_string(),
                evaluated: 40,
            }
        );
        Ok(())
    }

    #[test]
    fn division_by_zero_is_not_evaluable() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents / discount_threshold_cents
    } else {
        amount_cents
    }
}
"#;
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(10, 0), 0)?;");
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn non_equality_assertions_are_not_evaluable() -> Result<(), String> {
        for assertion in [
            "assert!(discounted_total(5_000, 5_000) > 0);",
            "assert!(matches!(discounted_total(5_000, 5_000), 3_500));",
            "assert_eq!(result, Ok(()));",
        ] {
            assert_eq!(
                verdict_for_owner(BOUNDARY_OWNER, assertion)?,
                ExactValueVerdict::NotEvaluable,
                "{assertion}"
            );
        }
        Ok(())
    }
}
