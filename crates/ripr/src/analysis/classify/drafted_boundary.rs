//! The concrete drafted boundary assertion behind repair starts and agent
//! packets: when the analyzer's static model can pin the equality-boundary
//! input from the related tests' own owner-call rows and fold the owner's
//! returned expression at that input, the guidance drafts
//! `assert_eq!(owner(<boundary inputs>), <value>)` instead of stopping at
//! the placeholder template one derivation short.
//!
//! Discipline (the wrong-actionable-signal invariant): the drafted EXPECTED
//! value comes only from the bounded integer fold over the CURRENT behavior
//! at the boundary input, and only where that model is strong — the
//! predicate-boundary shapes whose comparison operands are owner parameters
//! and whose branch values are plain integer arithmetic over those
//! parameters. Anything the fold cannot evaluate keeps the
//! `/* expected */` placeholder while the concrete INPUT side still emits.
//! The draft is static evidence: ripr has not run or verified anything, and
//! the developer confirms the expected value before relying on the test.

use crate::analysis::rust_index::FunctionSummary;
use crate::domain::DraftedBoundaryAssertion;

/// Draft the concrete boundary assertion, or `None` when the model is not
/// strong enough to pin the input (or to fold the expected value).
///
/// `tested_operand` is the compared operand a test varies and
/// `boundary_operand` the one whose observed value pins the boundary, both
/// already oriented by the caller (`PredicateBoundaryOperand`). `call_rows`
/// are the related tests' direct owner-call argument texts in owner
/// parameter order. `expression` is the changed predicate, read only for
/// its comparison operator, which selects the branch the boundary input
/// executes.
pub(in crate::analysis) fn drafted_boundary_assertion(
    owner_fn: &FunctionSummary,
    tested_operand: &str,
    boundary_operand: &str,
    expression: &str,
    predicate_line: usize,
    call_rows: &[Vec<String>],
) -> Option<DraftedBoundaryAssertion> {
    // A compound condition (`a > b && c`) does not reduce to one equality
    // boundary input, so refuse rather than guess.
    if expression.contains("&&") || expression.contains("||") {
        return None;
    }
    // The equality-boundary input is where the operands compare equal; the
    // predicate holds there only for the non-strict and equality forms.
    let boundary_holds = comparison_operator(expression);
    let boundary_holds = matches!(boundary_holds, Some(">=" | "<=" | "=="));
    let parameters = crate::analysis::classify::activation::function_parameters(owner_fn);
    let tested_index = parameter_index(&parameters, tested_operand)?;
    let boundary_index = parameter_index(&parameters, boundary_operand)?;
    if tested_index == boundary_index {
        return None;
    }
    // Every drafted argument must be a paste-ready plain integer literal:
    // floats (`50.0`), computed arguments (`base + 1`) and identifiers have
    // no pinned value, so a row containing one cannot draft a concrete call.
    let literal_rows: Vec<&Vec<String>> = call_rows
        .iter()
        .filter(|row| row.len() == parameters.len() && row.iter().all(|arg| plain_integer(arg)))
        .collect();
    let mut boundary_values: Vec<(String, String)> = literal_rows // (normalized, spelling)
        .iter()
        .map(|row| {
            (
                comparable_integer(&row[boundary_index]),
                row[boundary_index].clone(),
            )
        })
        .collect();
    boundary_values.sort_by(|a, b| a.1.cmp(&b.1));
    boundary_values.dedup_by(|a, b| a.0 == b.0);
    if boundary_values.len() != 1 {
        // Zero rows, or several distinct boundary operand values: the
        // equality boundary is not pinned to one input.
        return None;
    }
    let boundary_value = boundary_values.remove(0).1;
    // Deterministic row choice: the smallest tested-input literal among the
    // rows that already pass the boundary value.
    let mut candidates: Vec<&Vec<String>> = literal_rows
        .iter()
        .copied()
        .filter(|row| {
            comparable_integer(&row[boundary_index]) == comparable_integer(&boundary_value)
        })
        .collect();
    candidates.sort_by_key(|row| {
        (
            parse_integer(&row[tested_index]).unwrap_or(i64::MAX),
            row.join(", "),
        )
    });
    let row = candidates.first()?;
    let mut arguments = (*row).clone();
    arguments[tested_index] = boundary_value.clone();
    // The expected side stays a placeholder unless the owner's branch value
    // at the boundary input folds under the bounded arithmetic. The branch
    // is picked by whether the predicate holds at the equality boundary:
    // the tail-if reader names the arm the holding boundary executes and
    // the arm a failing boundary executes; explicit-return bodies fall back
    // to the registered return facts (first at/after the predicate, then
    // the following one). A model that does not expose the branch refuses
    // instead of guessing, because a wrong drafted value is worse than a
    // placeholder.
    let branch = if let Some([holding, failing]) =
        crate::analysis::classify::flow::predicate_branch_values(owner_fn, predicate_line)
    {
        if boundary_holds { holding } else { failing }
    } else {
        let texts = crate::analysis::classify::flow::returned_value_texts_at(
            Some(owner_fn),
            predicate_line,
        );
        let index = if boundary_holds { 0 } else { 1 };
        texts.get(index).cloned()
    };
    let bindings: Vec<(&str, i64)> = parameters
        .iter()
        .zip(arguments.iter())
        .map(|(parameter, argument)| Some((parameter.as_str(), parse_integer(argument)?)))
        .collect::<Option<Vec<_>>>()?;
    let expected = branch
        .as_deref()
        .and_then(|branch| fold_integer(branch, &bindings));
    Some(DraftedBoundaryAssertion {
        arguments,
        expected,
    })
}

fn comparison_operator(expression: &str) -> Option<&'static str> {
    // Longest forms first, so `>=` is not read as `>`.
    [">=", "<=", "==", "!=", ">", "<"]
        .into_iter()
        .find(|operator| expression.contains(operator))
}

fn parameter_index(parameters: &[String], operand: &str) -> Option<usize> {
    if operand.is_empty() {
        return None;
    }
    parameters.iter().position(|parameter| parameter == operand)
}

/// A plain integer literal: optional sign, digits with `_` separators, no
/// suffix, no decimal point. Suffixes and floats are refused so the drafted
/// call never mis-states a value's type or rounding.
fn plain_integer(text: &str) -> bool {
    let trimmed = text.trim();
    let body = trimmed.strip_prefix('-').unwrap_or(trimmed);
    !body.is_empty() && body.chars().all(|ch| ch.is_ascii_digit() || ch == '_')
}

fn comparable_integer(text: &str) -> String {
    text.trim().replace('_', "")
}

fn parse_integer(text: &str) -> Option<i64> {
    text.trim().replace('_', "").parse::<i64>().ok()
}

/// Fold a branch value expression under the pinned parameter bindings.
/// Grammar: whitespace, parentheses, `+`/`-`, plain integer literals, and
/// owner parameters whose drafted argument pins the value. Everything else
/// (calls, casts, multiplication, other identifiers, string/char forms)
/// refuses: the fold claims only what it evaluates exactly.
fn fold_integer(expression: &str, bindings: &[(&str, i64)]) -> Option<String> {
    let tokens = tokenize(expression)?;
    let mut position = 0usize;
    let value = parse_sum(&tokens, &mut position, bindings)?;
    if position != tokens.len() {
        return None;
    }
    Some(value.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Number(i64),
    Parameter(String),
    Plus,
    Minus,
    Open,
    Close,
}

fn tokenize(expression: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = expression.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        match ch {
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '(' => {
                tokens.push(Token::Open);
                index += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                index += 1;
            }
            '0'..='9' => {
                let start = index;
                while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '_')
                {
                    index += 1;
                }
                let text: String = chars[start..index].iter().collect();
                tokens.push(Token::Number(parse_integer(&text)?));
            }
            ch if ch == '_' || ch.is_ascii_alphabetic() => {
                let start = index;
                while index < chars.len()
                    && (chars[index] == '_' || chars[index].is_ascii_alphanumeric())
                {
                    index += 1;
                }
                let text: String = chars[start..index].iter().collect();
                tokens.push(Token::Parameter(text));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

fn parse_sum(tokens: &[Token], position: &mut usize, bindings: &[(&str, i64)]) -> Option<i64> {
    let mut value = parse_term(tokens, position, bindings)?;
    while *position < tokens.len() {
        match tokens[*position] {
            Token::Plus => {
                *position += 1;
                value = value.checked_add(parse_term(tokens, position, bindings)?)?;
            }
            Token::Minus => {
                *position += 1;
                value = value.checked_sub(parse_term(tokens, position, bindings)?)?;
            }
            _ => break,
        }
    }
    Some(value)
}

fn parse_term(tokens: &[Token], position: &mut usize, bindings: &[(&str, i64)]) -> Option<i64> {
    match tokens.get(*position)? {
        Token::Minus => {
            // Unary minus binds the term that follows.
            *position += 1;
            let value = parse_term(tokens, position, bindings)?;
            value.checked_neg()
        }
        Token::Number(value) => {
            *position += 1;
            Some(*value)
        }
        Token::Parameter(name) => {
            *position += 1;
            bindings
                .iter()
                .find(|(binding, _)| binding == name)
                .map(|(_, value)| *value)
        }
        Token::Open => {
            *position += 1;
            let value = parse_sum(tokens, position, bindings)?;
            if tokens.get(*position) != Some(&Token::Close) {
                return None;
            }
            *position += 1;
            Some(value)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::rust_index::ReturnFact;
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    /// The two-arm boundary owner the fixtures use, with the signature the
    /// parameter reader scans and the returns the flow model registers.
    fn owner(operator: &str, then_value: &str, else_value: &str) -> FunctionSummary {
        let body = format!(
            "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {{\n    if amount {operator} discount_threshold {{\n        {then_value}\n    }} else {{\n        {else_value}\n    }}\n}}\n"
        );
        FunctionSummary {
            id: SymbolId("src/lib.rs::discounted_total".to_string()),
            name: "discounted_total".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.into(),
            calls: Vec::new(),
            returns: vec![
                ReturnFact {
                    line: 3,
                    text: then_value.to_string(),
                },
                ReturnFact {
                    line: 5,
                    text: else_value.to_string(),
                },
            ],
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn rows(pairs: &[(&str, &str)]) -> Vec<Vec<String>> {
        pairs
            .iter()
            .map(|(amount, threshold)| vec![amount.to_string(), threshold.to_string()])
            .collect()
    }

    #[test]
    fn boundary_rows_and_folded_branch_draft_the_concrete_assertion() -> Result<(), String> {
        let owner_fn = owner(">=", "amount - 10", "amount");
        let drafted = drafted_boundary_assertion(
            &owner_fn,
            "amount",
            "discount_threshold",
            "amount >= discount_threshold",
            2,
            &rows(&[("50", "100"), ("10_000", "100")]),
        )
        .ok_or_else(|| "draft".to_string())?;
        assert_eq!(drafted.arguments, vec!["100", "100"]);
        assert_eq!(drafted.expected.as_deref(), Some("90"));
        Ok(())
    }

    #[test]
    fn strict_flip_reads_the_following_arm() -> Result<(), String> {
        // `amount > discount_threshold` fails at the equality boundary, so
        // the drafted value comes from the arm after the first one.
        let owner_fn = owner(">", "amount - 10", "amount");
        let drafted = drafted_boundary_assertion(
            &owner_fn,
            "amount",
            "discount_threshold",
            "amount > discount_threshold",
            2,
            &rows(&[("50", "100")]),
        )
        .ok_or_else(|| "draft".to_string())?;
        assert_eq!(drafted.arguments, vec!["100", "100"]);
        assert_eq!(drafted.expected.as_deref(), Some("100"));
        Ok(())
    }

    #[test]
    fn reversed_predicate_operands_still_pin_the_observed_threshold() -> Result<(), String> {
        // `discount_threshold <= amount` orients the boundary on the left;
        // the caller passes the oriented operands, so the observed
        // threshold value still pins the drafted call.
        let owner_fn = owner("<=", "amount - 10", "amount");
        let drafted = drafted_boundary_assertion(
            &owner_fn,
            "amount",
            "discount_threshold",
            "discount_threshold <= amount",
            2,
            &rows(&[("50", "100")]),
        )
        .ok_or_else(|| "draft".to_string())?;
        assert_eq!(drafted.arguments, vec!["100", "100"]);
        assert_eq!(drafted.expected.as_deref(), Some("90"));
        Ok(())
    }

    #[test]
    fn unfoldable_branch_keeps_the_input_side_only() -> Result<(), String> {
        let owner_fn = owner(">=", "amount / 2", "amount");
        let drafted = drafted_boundary_assertion(
            &owner_fn,
            "amount",
            "discount_threshold",
            "amount >= discount_threshold",
            2,
            &rows(&[("50", "100")]),
        )
        .ok_or_else(|| "input-side draft".to_string())?;
        assert_eq!(drafted.arguments, vec!["100", "100"]);
        assert_eq!(drafted.expected, None);
        Ok(())
    }

    #[test]
    fn several_distinct_threshold_values_do_not_pin_a_boundary() {
        let owner_fn = owner(">=", "amount - 10", "amount");
        assert!(
            drafted_boundary_assertion(
                &owner_fn,
                "amount",
                "discount_threshold",
                "amount >= discount_threshold",
                2,
                &rows(&[("50", "100"), ("50", "200")]),
            )
            .is_none()
        );
    }

    #[test]
    fn computed_arguments_refuse_the_concrete_call() {
        let owner_fn = owner(">=", "amount - 10", "amount");
        assert!(
            drafted_boundary_assertion(
                &owner_fn,
                "amount",
                "discount_threshold",
                "amount >= discount_threshold",
                2,
                &[vec!["50".to_string(), "base + 1".to_string()]],
            )
            .is_none()
        );
    }

    #[test]
    fn compound_conditions_refuse() {
        let owner_fn = owner(">=", "amount - 10", "amount");
        assert!(
            drafted_boundary_assertion(
                &owner_fn,
                "amount",
                "discount_threshold",
                "amount >= discount_threshold && discount_threshold > 0",
                2,
                &rows(&[("50", "100")]),
            )
            .is_none()
        );
    }

    #[test]
    fn early_return_without_else_keeps_the_expected_placeholder() -> Result<(), String> {
        // A strict predicate fails at the equality boundary; with an
        // early-return guard and no `else`, the failing boundary's arm is
        // not a plain tail value the reader can name, so the expected side
        // stays a placeholder while the concrete input side still emits.
        let body = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount > discount_threshold {\n        return amount - 10;\n    }\n    amount\n}\n";
        let owner_fn = FunctionSummary {
            id: SymbolId("src/lib.rs::discounted_total".to_string()),
            name: "discounted_total".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        let drafted = drafted_boundary_assertion(
            &owner_fn,
            "amount",
            "discount_threshold",
            "amount > discount_threshold",
            2,
            &rows(&[("50", "100")]),
        )
        .ok_or_else(|| "expected an input-side draft".to_string())?;
        assert_eq!(drafted.arguments, vec!["100", "100"]);
        assert_eq!(drafted.expected, None);
        Ok(())
    }

    #[test]
    fn non_parameter_operands_refuse() {
        let owner_fn = owner(">=", "amount - 10", "amount");
        assert!(
            drafted_boundary_assertion(
                &owner_fn,
                "amount",
                "DISCOUNT_THRESHOLD",
                "amount >= DISCOUNT_THRESHOLD",
                2,
                &rows(&[("50", "100")]),
            )
            .is_none()
        );
    }

    #[test]
    fn folded_expected_arithmetic_is_exact_and_bounded() -> Result<(), String> {
        let owner_fn = owner(">=", "amount - 10 + 5", "amount");
        let drafted = drafted_boundary_assertion(
            &owner_fn,
            "amount",
            "discount_threshold",
            "amount >= discount_threshold",
            2,
            &rows(&[("50", "100")]),
        )
        .ok_or_else(|| "expected a drafted assertion".to_string())?;
        assert_eq!(drafted.expected.as_deref(), Some("95"));
        Ok(())
    }
}
