//! Bounded, typed, conservative value transfer from exact test inputs
//! (#3295, P3 of #3215).
//!
//! The classifier receives exact input literals from related test
//! calls (`owner_call_parameter_values`). This module evaluates a
//! single-line `let` initializer over those inputs when — and only
//! when — every operation in the chain is one of the explicitly
//! implemented deterministic families:
//!
//! - `str::find` / `rfind`, `str::len`, `str::starts_with` /
//!   `ends_with` / `contains`, `str::strip_prefix` / `strip_suffix`
//! - `chars().next()` / `chars().next_back()`
//! - `char::len_utf8`
//! - `Option::map_or` with a literal default and the identity closure
//!   (`|idx| idx`, `|c| c`)
//! - `checked_add` / `checked_sub` on exact integers
//! - bounded string slicing from exact indices on established UTF-8
//!   boundaries
//! - exact integer arithmetic (`+ - * / %`) over exact indices, plus
//!   `split_whitespace().count()` and `chars().count()`
//!   (#9637). These families are credit-only: see `CREDIT_ONLY_OPERATIONS`.
//!
//! Literal-driven match arms are NOT an evaluated family: their arms
//! feed sinks, not comparison operands (RIPR-SPEC-0158 Non-Goals).

//! Every other shape fails closed to `Unsupported` naming the earliest
//! edge. No value ever becomes exact from token resemblance, an oracle
//! expectation, or a runtime result.

use std::collections::BTreeMap;

/// One typed value in the bounded transfer lattice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypedValue {
    Str(String),
    Char(char),
    Bool(bool),
    /// A non-negative exact integer (indices, lengths).
    Index(usize),
    OptIndex(Option<usize>),
    OptChar(Option<char>),
    OptStr(Option<String>),
    /// The char sequence produced by `chars()`; only `next()` /
    /// `next_back()` consume it.
    CharSeq(Vec<char>),
    /// The words `split_whitespace()` yields; only `count()` consumes them.
    Words(Vec<String>),
}

impl TypedValue {
    /// Canonical rendering used for exact-value comparison: decimal
    /// for indices, Rust literal forms for char/str, `none` for the
    /// empty option. Two operands observed equal compare equal here.
    pub(crate) fn render(&self) -> String {
        match self {
            TypedValue::Str(value) => format!("\"{value}\""),
            TypedValue::Char(value) => format!("'{value}'"),
            TypedValue::Bool(value) => value.to_string(),
            TypedValue::Index(value) => value.to_string(),
            TypedValue::OptIndex(None) | TypedValue::OptChar(None) | TypedValue::OptStr(None) => {
                "none".to_string()
            }
            TypedValue::CharSeq(chars) => chars.iter().collect::<String>(),
            TypedValue::Words(words) => format!("words{words:?}"),
            TypedValue::OptIndex(Some(value)) => value.to_string(),
            TypedValue::OptChar(Some(value)) => format!("'{value}'"),
            TypedValue::OptStr(Some(value)) => format!("\"{value}\""),
        }
    }
}

/// One provenance step: the operation that produced a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EvalStep {
    pub(crate) operation: String,
    pub(crate) detail: String,
}

/// The bounded evaluation outcome for one initializer under one exact
/// input row (#3295 evidence contract).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EvalOutcome {
    Exact {
        value: TypedValue,
        provenance: Vec<EvalStep>,
    },
    /// The chain names an operation ripr does not evaluate; the
    /// earliest edge is named, never skipped.
    Unsupported { earliest_edge: String },
    /// A slicing boundary is not on a UTF-8 char boundary.
    InvalidBoundary { reason: String },
}

/// Bound the chain length and literal size; the limits are disclosed
/// through `Unsupported` rather than silently truncating.
const MAX_CHAIN_STEPS: usize = 8;
const MAX_LITERAL_BYTES: usize = 512;

/// Exact inputs keyed by parameter name (the literal source text).
pub(crate) type ExactInputs = BTreeMap<String, String>;

/// Evaluate a single-line `let` initializer over the exact inputs.
pub(crate) fn evaluate_initializer(initializer: &str, inputs: &ExactInputs) -> EvalOutcome {
    let text = initializer.trim().trim_end_matches(';').trim();
    if text.len() > MAX_LITERAL_BYTES {
        return unsupported("initializer exceeds the bounded literal size");
    }
    let mut steps = Vec::new();
    match eval_expression(text, inputs, &mut steps) {
        Ok(value) => EvalOutcome::Exact {
            value,
            provenance: steps,
        },
        Err(outcome) => outcome,
    }
}

/// The exact value of a product of non-negative integers (#9329): factors
/// joined by `*`, each a plain decimal literal or an identifier bound in
/// `inputs` to a plain decimal. Nothing else is read: no parentheses, sums,
/// casts, suffixes or method calls. The product is checked against `max`
/// (the result type's largest value), so an overflow answers `None` instead
/// of a wrapped or clamped value. This is a separate entry from
/// [`evaluate_initializer`]: it never changes what a `let` initializer
/// evaluates to.
pub(crate) fn evaluate_integer_product(
    expression: &str,
    inputs: &ExactInputs,
    max: u128,
) -> Option<u128> {
    let mut product: u128 = 1;
    for factor in expression.trim().split('*') {
        let factor = factor.trim();
        let digits = if factor.chars().all(|ch| ch.is_ascii_digit()) {
            factor
        } else if is_identifier(factor) {
            inputs.get(factor)?.as_str()
        } else {
            return None;
        };
        if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        product = product.checked_mul(digits.parse::<u128>().ok()?)?;
        if product > max {
            return None;
        }
    }
    Some(product)
}

type EvalResult = Result<TypedValue, EvalOutcome>;

fn unsupported(edge: &str) -> EvalOutcome {
    EvalOutcome::Unsupported {
        earliest_edge: edge.to_string(),
    }
}

fn eval_expression(text: &str, inputs: &ExactInputs, steps: &mut Vec<EvalStep>) -> EvalResult {
    if steps.len() >= MAX_CHAIN_STEPS {
        return Err(unsupported("chain depth exceeds the bounded step limit"));
    }
    if let Some(literal) = parse_str_literal(text) {
        return Ok(TypedValue::Str(literal));
    }
    if let Some(literal) = parse_char_literal(text) {
        return Ok(TypedValue::Char(literal));
    }
    if let Some(value) = parse_index_literal(text) {
        return Ok(TypedValue::Index(value));
    }
    if let Some(boolean) = match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    } {
        return Ok(TypedValue::Bool(boolean));
    }
    // A bare identifier is exact only when it names an exact input.
    if is_identifier(text) {
        return match inputs.get(text) {
            Some(literal) => eval_expression(literal, inputs, steps),
            None => Err(unsupported(&format!(
                "identifier `{text}` is not one of the exact inputs"
            ))),
        };
    }
    if let Some(outcome) = eval_if_else(text, inputs, steps) {
        return outcome;
    }
    if text.starts_with("if ") {
        // An `if` expression with trailing text is not a binary operation.
        return Err(unsupported(&format!(
            "expression `{text}` is not a supported operation chain"
        )));
    }
    if let Some(outcome) = eval_arithmetic(text, inputs, steps) {
        return outcome;
    }
    if let Some(outcome) = eval_slicing(text, inputs, steps) {
        return outcome;
    }
    if let Some(outcome) = eval_map_or(text, inputs, steps) {
        return outcome;
    }
    eval_method_chain(text, inputs, steps)
}

/// Operation families added for #9637. They may close a boundary through an
/// observed equality row (credit) but never name a missing input, so a
/// consumer that would otherwise turn "no row lands on the boundary" into a
/// repair signal treats a chain using one as unknown instead.
pub(crate) const CREDIT_ONLY_OPERATIONS: [&str; 3] = ["arith", "split_whitespace", "count"];

/// Results above this bound fail closed: the unwritten integer type of a
/// literal-only expression could change the answer beyond it.
const ARITHMETIC_LIMIT: usize = 1 << 31;

/// Exact integer arithmetic (`+ - * / %`) over two exact `Index` operands
/// (`data.len() % 2`, `2 + 2`, `(n + 1) * 2`). The split is at the lowest
/// precedence, rightmost top-level operator, outside literals and brackets.
/// A leading sign, a non-index operand, an overflow, an underflow, a
/// division by zero, or a result beyond `ARITHMETIC_LIMIT` fails closed.
/// `None` means the text has no top-level binary operator and is some other
/// shape.
fn eval_arithmetic(
    text: &str,
    inputs: &ExactInputs,
    steps: &mut Vec<EvalStep>,
) -> Option<EvalResult> {
    let mut depth = 0usize;
    let mut literal: Option<char> = None;
    let mut escaped = false;
    let mut additive: Option<usize> = None;
    let mut multiplicative: Option<usize> = None;
    let mut seen_significant = false;
    let mut outer_group_closes_at: Option<usize> = None;
    for (at, character) in text.char_indices() {
        if let Some(quote) = literal {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                literal = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => literal = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && outer_group_closes_at.is_none() && text.starts_with('(') {
                    outer_group_closes_at = Some(at);
                }
            }
            '+' | '-' | '*' | '/' | '%' if depth == 0 => {
                if !seen_significant {
                    return Some(Err(unsupported(&format!(
                        "expression `{text}` starts with an operator"
                    ))));
                }
                let slot = if matches!(character, '+' | '-') {
                    &mut additive
                } else {
                    &mut multiplicative
                };
                *slot = Some(at);
            }
            _ => {}
        }
        if !character.is_whitespace() {
            seen_significant = true;
        }
    }
    let Some(at) = additive.or(multiplicative) else {
        // `(expr)` whose first group spans the whole text.
        return (outer_group_closes_at == Some(text.len().saturating_sub(1)) && text.len() >= 2)
            .then(|| eval_expression(text[1..text.len() - 1].trim(), inputs, steps));
    };
    let operator = text[at..].chars().next()?;
    let left_text = text[..at].trim();
    let right_text = text[at + 1..].trim();
    if left_text.is_empty()
        || right_text.is_empty()
        || left_text.ends_with(['+', '-', '*', '/', '%'])
        || right_text.starts_with(['+', '-', '*', '/', '%'])
    {
        return Some(Err(unsupported(&format!(
            "expression `{text}` is not a binary integer operation"
        ))));
    }
    let mut operand = |operand: &str| match eval_expression(operand, inputs, steps) {
        Ok(TypedValue::Index(value)) if value <= ARITHMETIC_LIMIT => Ok(value),
        Ok(other) => Err(unsupported(&format!(
            "operand `{operand}` is {} where integer arithmetic needs an exact bounded integer",
            other.render()
        ))),
        Err(outcome) => Err(outcome),
    };
    let left = match operand(left_text) {
        Ok(value) => value,
        Err(outcome) => return Some(Err(outcome)),
    };
    let right = match operand(right_text) {
        Ok(value) => value,
        Err(outcome) => return Some(Err(outcome)),
    };
    let result = match operator {
        '+' => left.checked_add(right),
        '-' => left.checked_sub(right),
        '*' => left.checked_mul(right),
        '/' => left.checked_div(right),
        '%' => left.checked_rem(right),
        _ => None,
    };
    let Some(result) = result.filter(|value| *value <= ARITHMETIC_LIMIT) else {
        return Some(Err(unsupported(&format!(
            "`{left} {operator} {right}` overflows, underflows or divides by zero"
        ))));
    };
    steps.push(EvalStep {
        operation: "arith".to_string(),
        detail: format!("{left} {operator} {right}"),
    });
    Some(Ok(TypedValue::Index(result)))
}

fn eval_method_chain(text: &str, inputs: &ExactInputs, steps: &mut Vec<EvalStep>) -> EvalResult {
    // Split into receiver and trailing method-call segments on '.',
    // skipping dots inside string and char literals (`"a.b".len()` is
    // one receiver, not three segments).
    let mut segments: Vec<&str> = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut literal: Option<char> = None;
    let mut escaped = false;
    for (at, character) in text.char_indices() {
        if let Some(quote) = literal {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                literal = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => literal = Some(character),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => {
                segments.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    segments.push(&text[start..]);
    if segments.len() < 2 {
        return Err(unsupported(&format!(
            "expression `{text}` is not a supported operation chain"
        )));
    }
    let mut value = eval_expression(segments[0], inputs, steps)?;
    for segment in &segments[1..] {
        value = apply_method(value, segment, inputs, steps)?;
    }
    Ok(value)
}

fn apply_method(
    receiver: TypedValue,
    segment: &str,
    inputs: &ExactInputs,
    steps: &mut Vec<EvalStep>,
) -> EvalResult {
    if steps.len() >= MAX_CHAIN_STEPS {
        return Err(unsupported("chain depth exceeds the bounded step limit"));
    }
    let (name, argument) = match segment.split_once('(') {
        Some((name, rest)) => {
            let argument = rest.strip_suffix(')').unwrap_or(rest);
            (name, argument.trim())
        }
        None => (segment, ""),
    };
    // Reject an unknown family before evaluating its arguments so the
    // named edge is the unsupported method itself, not its argument.
    const EVALUATED_METHODS: [&str; 16] = [
        "rfind",
        "find",
        "len",
        "starts_with",
        "ends_with",
        "contains",
        "strip_prefix",
        "strip_suffix",
        "len_utf8",
        "checked_add",
        "checked_sub",
        "chars",
        "next",
        "next_back",
        "split_whitespace",
        "count",
    ];
    if !EVALUATED_METHODS.contains(&name) {
        return Err(unsupported(&format!(
            "method `.{name}({argument})` is not an evaluated operation family"
        )));
    }
    let evaluated_argument = if argument.is_empty() {
        None
    } else {
        Some(eval_expression(argument, inputs, steps)?)
    };
    let receiver_render = receiver.render();
    let detail = format!(
        "{name}({}) on {}",
        evaluated_argument
            .as_ref()
            .map_or(String::new(), TypedValue::render),
        receiver_render
    );
    let next = match (receiver, name, evaluated_argument) {
        (TypedValue::Str(haystack), "rfind", Some(TypedValue::Char(needle))) => {
            TypedValue::OptIndex(haystack.rfind(needle))
        }
        (TypedValue::Str(haystack), "rfind", Some(TypedValue::Str(needle))) => {
            TypedValue::OptIndex(haystack.rfind(needle.as_str()))
        }
        (TypedValue::Str(haystack), "find", Some(TypedValue::Char(needle))) => {
            TypedValue::OptIndex(haystack.find(needle))
        }
        (TypedValue::Str(haystack), "find", Some(TypedValue::Str(needle))) => {
            TypedValue::OptIndex(haystack.find(needle.as_str()))
        }
        (TypedValue::Str(value), "len", None) => TypedValue::Index(value.len()),
        (TypedValue::Str(value), "split_whitespace", None) => {
            TypedValue::Words(value.split_whitespace().map(str::to_string).collect())
        }
        (TypedValue::Words(words), "count", None) => TypedValue::Index(words.len()),
        (TypedValue::CharSeq(chars), "count", None) => TypedValue::Index(chars.len()),
        (TypedValue::Str(value), "starts_with", Some(TypedValue::Str(prefix))) => {
            TypedValue::Bool(value.starts_with(&prefix))
        }
        (TypedValue::Str(value), "ends_with", Some(TypedValue::Str(suffix))) => {
            TypedValue::Bool(value.ends_with(&suffix))
        }
        (TypedValue::Str(value), "contains", Some(TypedValue::Str(needle))) => {
            TypedValue::Bool(value.contains(&needle))
        }
        (TypedValue::Str(value), "strip_prefix", Some(TypedValue::Str(prefix))) => {
            TypedValue::OptStr(value.strip_prefix(&prefix).map(str::to_string))
        }
        (TypedValue::Str(value), "strip_suffix", Some(TypedValue::Str(suffix))) => {
            TypedValue::OptStr(value.strip_suffix(&suffix).map(str::to_string))
        }
        (TypedValue::Char(value), "len_utf8", None) => TypedValue::Index(value.len_utf8()),
        (TypedValue::Str(value), "chars", None) => TypedValue::CharSeq(value.chars().collect()),
        (TypedValue::CharSeq(chars), "next", None) => TypedValue::OptChar(chars.first().copied()),
        (TypedValue::CharSeq(chars), "next_back", None) => {
            TypedValue::OptChar(chars.last().copied())
        }
        // checked arithmetic on exact indices
        (TypedValue::Index(left), "checked_add", Some(TypedValue::Index(right))) => {
            TypedValue::OptIndex(left.checked_add(right))
        }
        (TypedValue::Index(left), "checked_sub", Some(TypedValue::Index(right))) => {
            TypedValue::OptIndex(left.checked_sub(right))
        }
        // The family is evaluated but the receiver (or argument) type
        // is not one of its operand shapes — name the mismatch, not the
        // family (#3295 review).
        _ => {
            return Err(unsupported(&format!(
                "method `.{name}({argument})` receiver {receiver_render} is not an evaluated operand shape"
            )));
        }
    };
    steps.push(EvalStep {
        operation: name.to_string(),
        detail,
    });
    Ok(next)
}

/// `if flag { A } else { B }` (or `if !flag`) over an exact boolean input:
/// the taken arm is evaluated and the other is never read. The condition
/// must be one identifier bound to a `true`/`false` input; any other
/// condition shape, a missing `else`, or trailing text fails closed.
fn eval_if_else(text: &str, inputs: &ExactInputs, steps: &mut Vec<EvalStep>) -> Option<EvalResult> {
    let rest = text.strip_prefix("if ")?;
    let open = rest.find('{')?;
    let condition = rest[..open].trim();
    let (negated, flag) = match condition.strip_prefix('!') {
        Some(flag) => (true, flag.trim()),
        None => (false, condition),
    };
    if !is_identifier(flag) {
        return None;
    }
    let (then_arm, after_then) = take_braced(&rest[open..])?;
    let after_else = after_then.trim_start().strip_prefix("else")?.trim_start();
    let (else_arm, tail) = take_braced(after_else)?;
    if !tail.trim().is_empty() {
        return None;
    }
    let taken = match eval_expression(flag, inputs, &mut Vec::new()) {
        Ok(TypedValue::Bool(value)) => value != negated,
        Ok(_) => {
            return Some(Err(unsupported(&format!(
                "if condition `{flag}` is not a boolean input"
            ))));
        }
        Err(outcome) => return Some(Err(outcome)),
    };
    steps.push(EvalStep {
        operation: "if_else".to_string(),
        detail: format!(
            "`{condition}` selects the {} arm",
            if taken { "then" } else { "else" }
        ),
    });
    let arm = if taken { then_arm } else { else_arm };
    Some(eval_expression(arm.trim(), inputs, steps))
}

/// The inside of the leading `{ ... }` block and the text after it.
/// String literals are skipped so a brace inside one does not close it.
fn take_braced(text: &str) -> Option<(&str, &str)> {
    let body = text.strip_prefix('{')?;
    let mut depth = 1usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut skip_to = 0usize;
    for (at, character) in body.char_indices() {
        if at < skip_to {
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            // A char literal (`'}'`, `'{'`, `'\''`) is skipped whole; a
            // lifetime has no closing quote two characters on.
            '\'' => {
                let rest = &body[at + 1..];
                let width = if rest.starts_with('\\') {
                    rest.get(2..)
                        .and_then(|tail| tail.find('\''))
                        .map(|n| n + 3)
                } else {
                    let mut chars = rest.chars();
                    chars
                        .next()
                        .filter(|_| chars.next() == Some('\''))
                        .map(|first| first.len_utf8() + 1)
                };
                if let Some(width) = width {
                    skip_to = at + 1 + width;
                }
            }
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&body[..at], &body[at + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// `Option::map_or(default, |param| param)`: only the literal default
/// plus the identity closure are supported (`|idx| idx`).
fn eval_map_or(text: &str, inputs: &ExactInputs, steps: &mut Vec<EvalStep>) -> Option<EvalResult> {
    let dot = text.find(".map_or(")?;
    let (receiver_text, rest) = text.split_at(dot);
    let argument = rest
        .strip_prefix(".map_or(")
        .and_then(|inner| inner.strip_suffix(')'))?;
    let (default_text, closure_text) = split_call_arguments(argument)?;
    let closure = closure_text.trim();
    let identity_param = closure
        .strip_prefix('|')
        .and_then(|rest| rest.split_once('|'))
        .map(|(parameter, body)| (parameter.trim(), body.trim().trim_end_matches(';').trim()));
    let Some((parameter, body)) = identity_param else {
        return Some(Err(unsupported(
            "map_or closure is not the supported identity form",
        )));
    };
    if body != parameter {
        return Some(Err(unsupported(
            "map_or closure body is not the identity (only `|p| p` is evaluated)",
        )));
    }
    let receiver = match eval_expression(receiver_text, inputs, steps) {
        Ok(value) => value,
        Err(outcome) => return Some(Err(outcome)),
    };
    if steps.len() >= MAX_CHAIN_STEPS {
        return Some(Err(unsupported(
            "chain depth exceeds the bounded step limit",
        )));
    }
    let applied = match receiver {
        TypedValue::OptIndex(Some(value)) => TypedValue::Index(value),
        TypedValue::OptChar(Some(value)) => TypedValue::Char(value),
        TypedValue::OptStr(Some(value)) => TypedValue::Str(value),
        TypedValue::OptIndex(None) | TypedValue::OptChar(None) | TypedValue::OptStr(None) => {
            // The default produced the value: the step is part of the
            // provenance chain even on the None arm (#3295 review).
            let default_value = match eval_expression(default_text.trim(), inputs, steps) {
                Ok(value) => value,
                Err(outcome) => return Some(Err(outcome)),
            };
            steps.push(EvalStep {
                operation: "map_or".to_string(),
                detail: format!(
                    "default {} over none, default {}",
                    default_text.trim(),
                    default_value.render()
                ),
            });
            return Some(Ok(default_value));
        }
        other => {
            return Some(Err(unsupported(&format!(
                "map_or receiver {} is not an option value",
                other.render()
            ))));
        }
    };
    steps.push(EvalStep {
        operation: "map_or".to_string(),
        detail: format!(
            "identity closure over {}, default {}",
            applied.render(),
            default_text.trim()
        ),
    });
    Some(Ok(applied))
}

/// `&receiver[start..end]` / `&receiver[..end]` / `&receiver[start..]`
/// from exact indices; a boundary not on a char boundary is an
/// explicit `InvalidBoundary`, matching Rust's slice panic semantics
/// without executing them.
fn eval_slicing(text: &str, inputs: &ExactInputs, steps: &mut Vec<EvalStep>) -> Option<EvalResult> {
    let stripped = text
        .strip_prefix('&')
        .map(str::trim_start)
        .unwrap_or(text.trim_start());
    if !stripped.contains("..") {
        return None;
    }
    let (receiver_text, range) = stripped.split_once('[')?;
    let (start_text, end_text) = range.split_once("..")?;
    let end_text = end_text.strip_suffix(']').unwrap_or(end_text);
    let receiver = match eval_expression(receiver_text.trim(), inputs, steps) {
        Ok(TypedValue::Str(value)) => value,
        Ok(other) => {
            return Some(Err(unsupported(&format!(
                "slicing receiver {} is not a string",
                other.render()
            ))));
        }
        Err(outcome) => return Some(Err(outcome)),
    };
    let start = if start_text.trim().is_empty() {
        0
    } else {
        match eval_expression(start_text.trim(), inputs, steps) {
            Ok(TypedValue::Index(value)) => value,
            _ => return Some(Err(unsupported("slice start is not an exact index"))),
        }
    };
    let end = if end_text.trim().is_empty() {
        receiver.len()
    } else {
        match eval_expression(end_text.trim(), inputs, steps) {
            Ok(TypedValue::Index(value)) => value,
            _ => return Some(Err(unsupported("slice end is not an exact index"))),
        }
    };
    // Range relations first: `is_char_boundary` is false for any
    // index beyond `len`, so the boundary check alone would name the
    // wrong cause for an out-of-range or inverted slice.
    if start > end {
        return Some(Err(EvalOutcome::InvalidBoundary {
            reason: format!("slice [{start}..{end}] is inverted"),
        }));
    }
    if end > receiver.len() {
        return Some(Err(EvalOutcome::InvalidBoundary {
            reason: format!(
                "slice end {end} is out of range for \"{receiver}\" (len {})",
                receiver.len()
            ),
        }));
    }
    if !receiver.is_char_boundary(start) || !receiver.is_char_boundary(end) {
        return Some(Err(EvalOutcome::InvalidBoundary {
            reason: format!(
                "slice [{start}..{end}] is not a valid UTF-8 boundary of \"{receiver}\""
            ),
        }));
    }
    steps.push(EvalStep {
        operation: "index".to_string(),
        detail: format!("&[{start}..{end}] of \"{}\"", receiver),
    });
    Some(Ok(TypedValue::Str(receiver[start..end].to_string())))
}

/// Split a two-argument call list at the top-level comma.
fn split_call_arguments(argument: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    let mut literal: Option<char> = None;
    let mut escaped = false;
    for (at, character) in argument.char_indices() {
        if let Some(quote) = literal {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                literal = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => literal = Some(character),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some((&argument[..at], &argument[at + 1..])),
            _ => {}
        }
    }
    None
}

/// Parse a Rust string literal. The common escape grammar is
/// implemented exactly; any other escape, a raw/byte-string prefix, or
/// an over-size literal fails closed (`None`) so the value never
/// becomes a wrong exact (#3295 review: a misparsed `\n` fabricated
/// byte indices end to end).
fn parse_str_literal(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.starts_with('r') || trimmed.starts_with('b') {
        return None;
    }
    let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    let mut parsed = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            parsed.push(character);
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            'n' => parsed.push('\n'),
            't' => parsed.push('\t'),
            'r' => parsed.push('\r'),
            '0' => parsed.push('\0'),
            '\\' => parsed.push('\\'),
            '"' => parsed.push('"'),
            '\'' => parsed.push('\''),
            'x' => {
                let high = chars.next()?.to_digit(16)?;
                let low = chars.next()?.to_digit(16)?;
                let byte = u8::try_from(high * 16 + low).ok()?;
                // Only ASCII byte escapes are valid in a &str literal;
                // anything above is a compile error and fails closed.
                if byte > 0x7f {
                    return None;
                }
                parsed.push(byte as char);
            }
            'u' => {
                let open = chars.next()?;
                if open != '{' {
                    return None;
                }
                let mut digits = String::new();
                loop {
                    let next = chars.next()?;
                    if next == '}' {
                        break;
                    }
                    // Rust's unicode escape accepts at most six hex
                    // digits; reject the seventh instead of parsing it.
                    if !next.is_ascii_hexdigit() || digits.len() >= 6 {
                        return None;
                    }
                    digits.push(next);
                }
                let code = u32::from_str_radix(&digits, 16).ok()?;
                parsed.push(char::from_u32(code)?);
            }
            _ => return None,
        }
    }
    if parsed.len() > MAX_LITERAL_BYTES {
        return None;
    }
    Some(parsed)
}

fn parse_char_literal(text: &str) -> Option<char> {
    let trimmed = text.trim();
    let inner = trimmed.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut chars = inner.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match first {
        // The common escape grammar, mirroring `parse_str_literal`;
        // anything else fails closed.
        '\\' => match inner.chars().nth(1)? {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            '0' => Some('\0'),
            '\\' => Some('\\'),
            '\'' => Some('\''),
            _ => None,
        },
        plain => Some(plain),
    }
}

fn parse_index_literal(text: &str) -> Option<usize> {
    let trimmed = text.trim().replace('_', "");
    if trimmed.is_empty() || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    trimmed.parse().ok()
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(pairs: &[(&str, &str)]) -> ExactInputs {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn matches_unsupported(outcome: &EvalOutcome, needle: &str) -> bool {
        matches!(outcome, EvalOutcome::Unsupported { earliest_edge } if earliest_edge.contains(needle))
    }

    /// Assertion wrapper: compares equal only to the expected
    /// `TypedValue`, and prints the actual outcome on failure.
    #[derive(Debug)]
    struct ExactAssert(Result<TypedValue, String>);

    impl PartialEq<TypedValue> for ExactAssert {
        fn eq(&self, other: &TypedValue) -> bool {
            matches!(&self.0, Ok(value) if value == other)
        }
    }

    fn exact(initializer: &str, table: &[(&str, &str)]) -> ExactAssert {
        let outcome = match evaluate_initializer(initializer, &inputs(table)) {
            EvalOutcome::Exact { value, .. } => Ok(value),
            other => Err(format!(
                "expected exact value for `{initializer}`, got {other:?}"
            )),
        };
        ExactAssert(outcome)
    }

    /// Assertion wrapper for the named unsupported edge.
    #[derive(Debug)]
    struct EdgeAssert(Result<String, String>);

    impl PartialEq<&str> for EdgeAssert {
        fn eq(&self, other: &&str) -> bool {
            matches!(&self.0, Ok(edge) if edge == other)
        }
    }

    fn unsupported_edge(initializer: &str, table: &[(&str, &str)]) -> EdgeAssert {
        let outcome = match evaluate_initializer(initializer, &inputs(table)) {
            EvalOutcome::Unsupported { earliest_edge } => Ok(earliest_edge),
            other => Err(format!(
                "expected unsupported for `{initializer}`, got {other:?}"
            )),
        };
        EdgeAssert(outcome)
    }

    #[test]
    fn integer_arithmetic_over_exact_indices_resolves_with_precedence() {
        let table = &[("input", "\"abcde\""), ("n", "6")];
        assert_eq!(exact("2 + 2", &[]), TypedValue::Index(4));
        assert_eq!(exact("input.len() % 2", table), TypedValue::Index(1));
        assert_eq!(exact("2 + 3 * 4", &[]), TypedValue::Index(14));
        assert_eq!(exact("(2 + 3) * 4", &[]), TypedValue::Index(20));
        assert_eq!(exact("n - 1 - 2", table), TypedValue::Index(3));
        assert_eq!(exact("n / 4", table), TypedValue::Index(1));
        assert_eq!(exact("\"a-b\".len() + 1", &[]), TypedValue::Index(4));
        assert_eq!(exact("(input).len()", table), TypedValue::Index(5));
    }

    #[test]
    fn integer_arithmetic_fails_closed() {
        let table = &[("input", "\"abcde\""), ("n", "6")];
        for expression in [
            "n - 7",
            "n / 0",
            "n % 0",
            "-1 + n",
            "n + -1",
            "2147483648 + 1",
            "input + 1",
            "2u8 + 2",
            "n + missing",
            "n *",
        ] {
            assert!(
                matches!(
                    evaluate_initializer(expression, &inputs(table)),
                    EvalOutcome::Unsupported { .. }
                ),
                "{expression}"
            );
        }
    }

    #[test]
    fn char_and_word_counts_resolve_from_a_text_literal() {
        let table = &[("input", "\"héllo wörld  x\"")];
        assert_eq!(
            exact("input.split_whitespace().count()", table),
            TypedValue::Index(3)
        );
        assert_eq!(exact("input.chars().count()", table), TypedValue::Index(14));
        assert!(matches!(
            evaluate_initializer("input.chars().count().count()", &inputs(table)),
            EvalOutcome::Unsupported { .. }
        ));
    }

    #[test]
    fn literals_and_identifiers_resolve_exactly() {
        assert_eq!(exact("\"done\"", &[]), TypedValue::Str("done".into()));
        assert_eq!(exact("'x'", &[]), TypedValue::Char('x'));
        assert_eq!(exact("10", &[]), TypedValue::Index(10));
        assert_eq!(exact("true", &[]), TypedValue::Bool(true));
        assert_eq!(exact("delim", &[("delim", "'.'")]), TypedValue::Char('.'));
        assert_eq!(
            unsupported_edge("mystery", &[]),
            "identifier `mystery` is not one of the exact inputs"
        );
    }

    #[test]
    fn if_else_reads_the_arm_a_boolean_input_selects() {
        let arms = [("KIB", "1_024"), ("KB", "1_000")];
        let with = |flag: &'static str| {
            let mut table = vec![("si", flag)];
            table.extend(arms);
            table
        };
        let selected = "if si { KIB } else { KB }";
        assert_eq!(exact(selected, &with("true")), TypedValue::Index(1_024));
        assert_eq!(exact(selected, &with("false")), TypedValue::Index(1_000));
        let negated = "if !si { KB } else { KIB }";
        assert_eq!(exact(negated, &with("true")), TypedValue::Index(1_024));
        assert_eq!(exact(negated, &with("false")), TypedValue::Index(1_000));
        assert_eq!(
            exact("if si { 7 } else { 9 }", &with("false")),
            TypedValue::Index(9)
        );
    }

    #[test]
    fn if_else_arms_may_hold_braces_in_char_and_string_literals() {
        let table = [("flag", "true")];
        assert_eq!(
            exact("if flag { '}' } else { 'x' }", &table),
            TypedValue::Char('}')
        );
        assert_eq!(
            exact("if flag { '{' } else { 'x' }", &table),
            TypedValue::Char('{')
        );
        assert_eq!(
            exact("if flag { \"}\" } else { \"x\" }", &table),
            TypedValue::Str("}".into())
        );
        assert_eq!(
            exact("if !flag { '{' } else { 'x' }", &table),
            TypedValue::Char('x')
        );
    }

    #[test]
    fn if_else_fails_closed_off_its_exact_shape() {
        let table = [("si", "true"), ("KIB", "1_024"), ("n", "3")];
        // An arm whose constant is not an exact input names that identifier.
        assert_eq!(
            unsupported_edge("if si { KIB } else { KB }", &[("si", "false")]),
            "identifier `KB` is not one of the exact inputs"
        );
        // The condition must be one boolean input, not a computed test.
        assert_eq!(
            unsupported_edge("if n { KIB } else { 0 }", &table),
            "if condition `n` is not a boolean input"
        );
        assert_eq!(
            unsupported_edge("if flag { KIB } else { 0 }", &table),
            "identifier `flag` is not one of the exact inputs"
        );
        // No `else`, trailing text, and a compound condition are not read.
        for shape in [
            "if si { KIB }",
            "if si { KIB } else { 0 } + 1",
            "if si && n { KIB } else { 0 }",
        ] {
            assert!(
                matches!(
                    evaluate_initializer(shape, &inputs(&table)),
                    EvalOutcome::Unsupported { .. }
                ),
                "`{shape}` must not evaluate"
            );
        }
    }

    #[test]
    fn find_family_covers_present_absent_empty_and_multibyte() {
        let table = &[("input", "\"a.b\""), ("delim", "'.'")];
        assert_eq!(
            exact("input.rfind(delim)", table),
            TypedValue::OptIndex(Some(1))
        );
        assert_eq!(
            exact("input.find(delim)", table),
            TypedValue::OptIndex(Some(1))
        );
        let empty = &[("input", "\"\""), ("delim", "'.'")];
        assert_eq!(
            exact("input.rfind(delim)", empty),
            TypedValue::OptIndex(None)
        );
        let multi = &[("input", "\"aéb\""), ("delim", "'é'")];
        assert_eq!(
            exact("input.rfind(delim)", multi),
            TypedValue::OptIndex(Some(1))
        );
        let absent = &[("input", "\"ab\""), ("delim", "'x'")];
        assert_eq!(
            exact("input.rfind(delim)", absent),
            TypedValue::OptIndex(None)
        );
    }

    #[test]
    fn the_3215_equality_chain_resolves_exactly() {
        let table = &[("input", "\"ab\""), ("delim", "'x'")];
        assert_eq!(
            exact("input.rfind(delim).map_or(1, |idx| idx)", table),
            TypedValue::Index(1)
        );
        assert_eq!(exact("delim.len_utf8()", table), TypedValue::Index(1));
        let present = &[("input", "\"a.b\""), ("delim", "'.'")];
        assert_eq!(
            exact("input.rfind(delim).map_or(0, |idx| idx)", present),
            TypedValue::Index(1)
        );
    }

    #[test]
    fn map_or_fails_closed_on_non_identity_closures() {
        let table = &[("input", "\"ab\""), ("delim", "'x'")];
        assert_eq!(
            unsupported_edge("input.rfind(delim).map_or(0, |idx| idx + 1)", table),
            "map_or closure body is not the identity (only `|p| p` is evaluated)"
        );
        assert_eq!(
            unsupported_edge("input.rfind(delim).map_or_else(|| 0, |idx| idx)", table),
            "method `.map_or_else(|| 0, |idx| idx)` is not an evaluated operation family"
        );
    }

    #[test]
    fn boolean_and_strip_families_resolve_exactly() {
        let table = &[("input", "\"alpha beta\"")];
        assert_eq!(
            exact("input.starts_with(\"alpha\")", table),
            TypedValue::Bool(true)
        );
        assert_eq!(
            exact("input.ends_with(\"omega\")", table),
            TypedValue::Bool(false)
        );
        assert_eq!(
            exact("input.contains(\"ph\")", table),
            TypedValue::Bool(true)
        );
        assert_eq!(
            exact("input.strip_prefix(\"alpha \")", table),
            TypedValue::OptStr(Some("beta".into()))
        );
        assert_eq!(
            exact("input.strip_prefix(\"omega\")", table),
            TypedValue::OptStr(None)
        );
        assert_eq!(
            exact("input.strip_suffix(\"beta\")", table),
            TypedValue::OptStr(Some("alpha ".into()))
        );
        assert_eq!(exact("input.len()", table), TypedValue::Index(10));
    }

    #[test]
    fn chars_next_family_covers_empty_ascii_and_multibyte() {
        // `chars().next()` over a string input evaluates through the
        // chain as a char step.
        let ascii = &[("input", "\"ab\"")];
        assert_eq!(
            exact("input.chars().next()", ascii),
            TypedValue::OptChar(Some('a'))
        );
        let multi = &[("input", "\"éx\"")];
        assert_eq!(
            exact("input.chars().next_back()", multi),
            TypedValue::OptChar(Some('x'))
        );
        let empty = &[("input", "\"\"")];
        assert_eq!(
            exact("input.chars().next()", empty),
            TypedValue::OptChar(None)
        );
    }

    #[test]
    fn checked_arithmetic_handles_success_and_overflow() {
        assert_eq!(
            exact("end.checked_add(1)", &[("end", "1")]),
            TypedValue::OptIndex(Some(2))
        );
        // Overflow and underflow both collapse to the `None` arm.
        assert_eq!(
            exact("end.checked_add(1)", &[("end", &usize::MAX.to_string())]),
            TypedValue::OptIndex(None)
        );
        assert_eq!(
            exact("end.checked_sub(2)", &[("end", "1")]),
            TypedValue::OptIndex(None)
        );
    }

    #[test]
    fn slicing_honors_utf8_boundaries() -> Result<(), String> {
        // `é` occupies bytes 1..3: 3 is the first boundary after it.
        let table = &[("input", "\"aéb\""), ("end", "3")];
        assert_eq!(exact("&input[..end]", table), TypedValue::Str("aé".into()));
        let bad = &[("input", "\"aéb\""), ("end", "2")];
        match evaluate_initializer("&input[..end]", &inputs(bad)) {
            EvalOutcome::InvalidBoundary { reason } => {
                assert!(reason.contains("not a valid UTF-8 boundary"), "{reason}");
            }
            other => return Err(format!("expected invalid boundary, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn member_lookalikes_and_dynamic_inputs_fail_closed() {
        let table = &[("input", "\"ab\"")];
        assert_eq!(
            unsupported_edge("input.rfind(needle)", table),
            "identifier `needle` is not one of the exact inputs"
        );
        // A custom type with the same method name still evaluates only
        // when the receiver type matches the implemented family; a
        // non-string receiver fails closed.
        assert!(matches_unsupported(
            &evaluate_initializer("count.rfind(delim)", &inputs(table)),
            "not one of the exact inputs"
        ));
    }

    // #3295 review F1: string escapes follow Rust's grammar exactly;
    // a misparsed escape must never produce a wrong exact value.
    #[test]
    fn string_escapes_evaluate_exactly() -> Result<(), String> {
        // The row values are the SOURCE TEXT of test-call literals
        // (backslash escapes as written), exactly what scalar_values
        // extracts from a call like `f("a\nb")`.
        assert_eq!(
            exact("input.len()", &[("input", "\"a\\nb\"")]),
            TypedValue::Index(3)
        );
        assert_eq!(
            exact("input.find(\"x\")", &[("input", "\"\\tx\"")]),
            TypedValue::OptIndex(Some(1))
        );
        assert_eq!(
            exact("input.len()", &[("input", "\"\\u{41}\"")]),
            TypedValue::Index(1)
        );
        assert_eq!(
            exact("input.find(\"y\")", &[("input", "\"x\\ny\"")]),
            TypedValue::OptIndex(Some(2))
        );
        // An unknown escape is not a string literal at all: the input
        // fails to parse and the chain fails closed instead of
        // guessing a value.
        assert!(matches_unsupported(
            &evaluate_initializer("input.len()", &inputs(&[("input", "\"a\\qb\"")])),
            "not a supported operation chain"
        ));
        Ok(())
    }

    #[test]
    fn map_or_default_arm_keeps_its_provenance_step() -> Result<(), String> {
        match evaluate_initializer(
            "input.rfind(delim).map_or(7, |idx| idx)",
            &inputs(&[("input", "\"ab\""), ("delim", "'x'")]),
        ) {
            EvalOutcome::Exact { value, provenance } => {
                assert_eq!(value, TypedValue::Index(7));
                assert_eq!(provenance.len(), 2, "rfind + the default arm");
                assert_eq!(provenance[1].operation, "map_or");
            }
            other => return Err(format!("expected exact, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn provenance_records_every_step() -> Result<(), String> {
        match evaluate_initializer(
            "input.rfind(delim).map_or(0, |idx| idx)",
            &inputs(&[("input", "\"a.b\""), ("delim", "'.'")]),
        ) {
            EvalOutcome::Exact { provenance, .. } => {
                assert_eq!(provenance.len(), 2);
                assert_eq!(provenance[0].operation, "rfind");
                assert_eq!(provenance[1].operation, "map_or");
            }
            other => return Err(format!("expected exact, got {other:?}")),
        }
        Ok(())
    }

    // #9329: the integer product reader stays a separate, closed entry.
    #[test]
    fn integer_product_reads_only_factors_and_checks_the_type_bound() {
        let inputs: ExactInputs = [("size", "3"), ("KIB", "1024"), ("name", "x1")]
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        let max = u128::from(u64::MAX);
        assert_eq!(
            evaluate_integer_product("size * KIB", &inputs, max),
            Some(3072)
        );
        assert_eq!(
            evaluate_integer_product(" 4 * 1024 ", &inputs, max),
            Some(4096)
        );
        assert_eq!(evaluate_integer_product("size", &inputs, max), Some(3));
        // The bound is the result type's maximum, checked at every step.
        assert_eq!(evaluate_integer_product("size * KIB", &inputs, 3071), None);
        let huge: ExactInputs = [("n", u64::MAX.to_string())]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(evaluate_integer_product("n * 2", &huge, max), None);
        assert_eq!(evaluate_integer_product("n * 1", &huge, max), Some(max));
        // Nothing but `*` over literals and bound plain decimals.
        for expression in [
            "",
            "size *",
            "* size",
            "size * * KIB",
            "size + KIB",
            "size - 1",
            "(size) * 2",
            "size * missing",
            "name * 2",
            "size as u64",
            "size * 1u64",
            "size * -1",
            "size.pow(2)",
            "size ** 2",
            "1_024 * 2",
            "0x10 * 2",
        ] {
            assert_eq!(
                evaluate_integer_product(expression, &inputs, max),
                None,
                "{expression:?}"
            );
        }
    }
}
