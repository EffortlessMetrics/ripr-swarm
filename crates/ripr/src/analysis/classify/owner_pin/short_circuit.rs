//! RIPR-SPEC-0197 rule 3 short-circuit tails (#7969 grid cell): a bool
//! owner whose whole body is one `&&` chain or one `||` chain of
//! comparisons over its parameters.
//!
//! Rule 3 refuses `&&`/`||` because a pin may never evaluate the changed
//! part. `x == 0 && y == 1` pinned only at `(1, 5)` returns `false` from
//! `x == 0` alone; the changed `y == 1` never runs. This gate admits the
//! chain only when ripr can show that some pinned call evaluates every
//! operand: the test passes literal arguments (directly, through a
//! constant-row table, or through a check helper's calls), and on one of
//! those runs every operand but the last takes the value that does not
//! short-circuit (`true` for `&&`, `false` for `||`).
//!
//! Each operand is a bool parameter after any number of `!`, or one
//! comparison of two atoms with no leading `!` (`!x == 0` is `(!x) == 0`).
//! An atom is a parameter, a plain integer or bool literal, an unescaped
//! char literal, or a parameter `%` or `/` a nonzero integer literal. These operands cannot call, panic or
//! write, so skipping one changes nothing but the value. A chain mixing
//! `&&` and `||`, a call, an index, a field, a cast, a parenthesis or any
//! other text is not read (fail closed).

use super::closure_adaptor::{call_count, literal_boundary, top_level_arguments};
use super::whole_word_offsets;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::TestSummary;
use crate::analysis::syntax::{HelperLoan, constant_table_column_around};

/// The admitted chain: its operator, the owner's parameters in order, and
/// its operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) struct ShortCircuitTail {
    /// `&&` or `||`.
    operator: &'static str,
    parameters: Vec<String>,
    operands: Vec<Operand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Operand {
    negations: usize,
    kind: OperandKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OperandKind {
    /// A bool parameter.
    Flag(usize),
    Compare {
        left: Atom,
        operator: &'static str,
        right: Atom,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Atom {
    Parameter(usize),
    Literal(String),
    /// A parameter `%` or `/` a nonzero integer literal.
    Arithmetic {
        parameter: usize,
        operator: char,
        divisor: i128,
    },
}

const COMPARISONS: &[&str] = &["==", "!=", "<=", ">=", "<", ">"];

impl ShortCircuitTail {
    /// The chain when `changed` (the owner's whole tail) is one, and
    /// `function` (the owner's text from `fn`) declares plain identifier
    /// parameters with no receiver.
    pub(in crate::analysis::classify) fn parse(function: &str, changed: &str) -> Option<Self> {
        let parameters = parameter_names(function)?;
        // The chain must be the whole body: an earlier statement could
        // rebind a parameter (`let x = x + 1;`), and then the call's
        // arguments are not the values the chain reads.
        if !is_whole_body(function, changed) {
            return None;
        }
        // Split where the masked text holds the operator, so a `&&` inside
        // a string or comment is never one; read operands from the original
        // text, which masking keeps aligned byte for byte.
        let masked = mask_comments_and_strings(changed);
        if masked.len() != changed.len() || masked.contains("//") || masked.contains("/*") {
            return None;
        }
        let operator = match (masked.contains("&&"), masked.contains("||")) {
            (true, false) => "&&",
            (false, true) => "||",
            _ => return None,
        };
        let mut operands = Vec::new();
        let mut start = 0;
        for (at, _) in masked.match_indices(operator) {
            operands.push(parse_operand(&changed[start..at], &parameters)?);
            start = at + operator.len();
        }
        operands.push(parse_operand(&changed[start..], &parameters)?);
        (operands.len() >= 2).then_some(Self {
            operator,
            parameters,
            operands,
        })
    }

    /// Whether some run of the pinned call `call_text` (of `name`)
    /// evaluates every operand. Every argument the chain reads must be
    /// readable on that run; an unreadable run never counts.
    pub(in crate::analysis::classify) fn pinned_call_evaluates_every_operand(
        &self,
        test: &TestSummary,
        loan: Option<&HelperLoan>,
        call_text: &str,
        call_line: usize,
        name: &str,
    ) -> bool {
        let Some(arguments) = call_arguments(call_text, name) else {
            return false;
        };
        if arguments.len() != self.parameters.len() {
            return false;
        }
        let Some(runs) = argument_runs(test, loan, &arguments, call_line, name) else {
            return false;
        };
        let continues = self.operator == "&&";
        // A run counts only when every argument reads, the last operand's
        // included: the spec's run check reads the whole call.
        runs.iter().any(|values| {
            values.iter().all(Option::is_some)
                && self.operands[..self.operands.len() - 1]
                    .iter()
                    .all(|operand| operand.evaluate(values) == Some(continues))
        })
    }
}

impl Operand {
    fn evaluate(&self, values: &[Option<String>]) -> Option<bool> {
        let value = match &self.kind {
            OperandKind::Flag(parameter) => match values.get(*parameter)?.as_deref()? {
                "true" => true,
                "false" => false,
                _ => return None,
            },
            OperandKind::Compare {
                left,
                operator,
                right,
            } => compare(&left.value(values)?, operator, &right.value(values)?)?,
        };
        Some(if self.negations.is_multiple_of(2) {
            value
        } else {
            !value
        })
    }
}

impl Atom {
    fn value(&self, values: &[Option<String>]) -> Option<Value> {
        match self {
            Self::Parameter(parameter) => Value::read(values.get(*parameter)?.as_deref()?),
            Self::Literal(literal) => Value::read(literal),
            Self::Arithmetic {
                parameter,
                operator,
                divisor,
            } => {
                let Value::Integer(value) = Value::read(values.get(*parameter)?.as_deref()?)?
                else {
                    return None;
                };
                Some(Value::Integer(match operator {
                    '%' => value.checked_rem(*divisor)?,
                    _ => value.checked_div(*divisor)?,
                }))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Integer(i128),
    Bool(bool),
    Char(String),
}

impl Value {
    /// A plain integer, bool or char literal; floats are not read, so no
    /// rounding question arises.
    fn read(text: &str) -> Option<Self> {
        let text = literal_boundary(text)?;
        match text.as_str() {
            "true" => return Some(Self::Bool(true)),
            "false" => return Some(Self::Bool(false)),
            _ => {}
        }
        if let Some(body) = text
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            // Only an unescaped char compares by its text: `'\n'` and a
            // decoded escape could name one char two ways.
            let mut chars = body.chars();
            return match (chars.next(), chars.next()) {
                (Some(character), None) if character != '\\' => {
                    Some(Self::Char(character.to_string()))
                }
                _ => None,
            };
        }
        if text.contains('.') {
            return None;
        }
        text.replace('_', "").parse().ok().map(Self::Integer)
    }
}

fn compare(left: &Value, operator: &str, right: &Value) -> Option<bool> {
    let ordering = match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Char(left), Value::Char(right)) if matches!(operator, "==" | "!=") => {
            left.cmp(right)
        }
        _ => return None,
    };
    Some(match operator {
        "==" => ordering.is_eq(),
        "!=" => ordering.is_ne(),
        "<" => ordering.is_lt(),
        "<=" => ordering.is_le(),
        ">" => ordering.is_gt(),
        ">=" => ordering.is_ge(),
        _ => return None,
    })
}

fn parse_operand(text: &str, parameters: &[String]) -> Option<Operand> {
    let mut text = text.trim();
    let mut negations = 0;
    while let Some(rest) = text.strip_prefix('!') {
        if rest.starts_with('=') {
            return None;
        }
        negations += 1;
        text = rest.trim_start();
    }
    for operator in COMPARISONS {
        if let Some(at) = text.find(operator) {
            // `!x == 0` is `(!x) == 0`: the `!` binds to the atom, and on an
            // integer it is a bitwise NOT. Only a bare bool parameter takes
            // a leading `!`.
            if negations > 0 {
                return None;
            }
            let left = parse_atom(&text[..at], parameters)?;
            let right = parse_atom(&text[at + operator.len()..], parameters)?;
            // `a == b` of two literals, or no parameter at all, reads nothing
            // the test chooses.
            if matches!((&left, &right), (Atom::Literal(_), Atom::Literal(_))) {
                return None;
            }
            return Some(Operand {
                negations,
                kind: OperandKind::Compare {
                    left,
                    operator,
                    right,
                },
            });
        }
    }
    let parameter = parameters.iter().position(|name| name == text)?;
    Some(Operand {
        negations,
        kind: OperandKind::Flag(parameter),
    })
}

fn parse_atom(text: &str, parameters: &[String]) -> Option<Atom> {
    let text = text.trim();
    for operator in ['%', '/'] {
        if let Some((left, right)) = text.split_once(operator) {
            let parameter = parameters.iter().position(|name| name == left.trim())?;
            let right = right.trim();
            if !right
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'_')
            {
                return None;
            }
            let divisor: i128 = right.replace('_', "").parse().ok()?;
            return (divisor != 0).then_some(Atom::Arithmetic {
                parameter,
                operator,
                divisor,
            });
        }
    }
    if let Some(parameter) = parameters.iter().position(|name| name == text) {
        return Some(Atom::Parameter(parameter));
    }
    // Masked strings and floats are not read; `Value::read` refuses them.
    let literal = literal_boundary(text)?;
    Value::read(&literal).map(|_| Atom::Literal(literal))
}

/// Whether `changed` is all the owner's body holds between its braces,
/// ignoring comments and whitespace.
fn is_whole_body(function: &str, changed: &str) -> bool {
    let masked = mask_comments_and_strings(function);
    let Some(close) = masked.rfind('}') else {
        return false;
    };
    let Some(open) = super::body_block_open(&masked) else {
        return false;
    };
    let compact = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    open < close
        && compact(&masked[open + 1..close]) == compact(&mask_comments_and_strings(changed))
}

/// The owner's parameter names, when each is a plain identifier pattern
/// and there is no `self` receiver.
fn parameter_names(function: &str) -> Option<Vec<String>> {
    let masked = mask_comments_and_strings(function);
    let name_end = masked.find("fn ")? + 3;
    let open = name_end + masked[name_end..].find('(')?;
    if masked[name_end..open].contains('<') {
        return None;
    }
    let close = super::matching_close(&masked, open, b'(', b')')?;
    let inside = masked[open + 1..close].trim();
    if inside.is_empty() {
        return None;
    }
    top_level_arguments(inside)
        .into_iter()
        .filter(|parameter| !parameter.trim().is_empty())
        .map(|parameter| {
            let (pattern, _) = parameter.split_once(':')?;
            let pattern = pattern.trim();
            (super::is_plain_identifier(pattern) && pattern != "self" && pattern != "_")
                .then(|| pattern.to_string())
        })
        .collect()
}

/// The top-level argument texts of the pinned call, which must be the only
/// call of `name` in `call_text`.
fn call_arguments(call_text: &str, name: &str) -> Option<Vec<String>> {
    if call_count(call_text, name) != 1 {
        return None;
    }
    let masked = mask_comments_and_strings(call_text);
    let open = whole_word_offsets(&masked, name)
        .into_iter()
        .map(|offset| offset + name.len())
        .find(|end| masked[*end..].starts_with('('))?;
    let close = super::matching_close(&masked, open, b'(', b')')?;
    let inside = call_text[open + 1..close].trim();
    if inside.is_empty() {
        return Some(Vec::new());
    }
    Some(top_level_arguments(inside))
}

/// One value per argument for each run of the pinned call: `None` for an
/// argument ripr cannot read on that run. A literal argument has its value
/// on every run. A name is a check helper's parameter, read at each of the
/// test's eager calls of the helper, or else a column of a constant-row
/// `for` table, read per row. `None` when the runs cannot be aligned.
fn argument_runs(
    test: &TestSummary,
    loan: Option<&HelperLoan>,
    arguments: &[String],
    call_line: usize,
    name: &str,
) -> Option<Vec<Vec<Option<String>>>> {
    enum Source {
        Literal(String),
        Column(Vec<String>),
        Unread,
    }
    let mut sources = Vec::new();
    let mut rows: Option<usize> = None;
    for argument in arguments {
        let argument = argument.trim();
        let source = if let Some(literal) = literal_boundary(argument) {
            Source::Literal(literal)
        } else if super::is_plain_identifier(argument) {
            let column = match loan {
                Some(loan) => helper_column(test, loan, argument),
                None => call_offset(test, call_line, name)
                    .and_then(|at| constant_table_column_around(&test.body, argument, at)),
            };
            match column {
                Some(column) => {
                    if rows.is_some_and(|rows| rows != column.len()) {
                        return None;
                    }
                    rows = Some(column.len());
                    Source::Column(column)
                }
                None => Source::Unread,
            }
        } else {
            Source::Unread
        };
        sources.push(source);
    }
    let rows = rows.unwrap_or(1);
    Some(
        (0..rows)
            .map(|row| {
                sources
                    .iter()
                    .map(|source| match source {
                        Source::Literal(literal) => Some(literal.clone()),
                        Source::Column(column) => {
                            column.get(row).and_then(|cell| literal_boundary(cell))
                        }
                        Source::Unread => None,
                    })
                    .collect()
            })
            .collect(),
    )
}

/// The texts `parameter` of the check helper takes at each of the test's
/// eager calls of it, or `None` when any call is unreadable.
/// The byte offset in `test.body` of the owner call on absolute line
/// `call_line`, so a table column counts only for a call inside its loop.
fn call_offset(test: &TestSummary, call_line: usize, name: &str) -> Option<usize> {
    let index = call_line.checked_sub(test.start_line)?;
    let mut offset = 0;
    for (number, line) in test.body.split_inclusive('\n').enumerate() {
        if number == index {
            let column = line
                .find(&format!("{name}("))
                .unwrap_or(line.len() - line.trim_start().len());
            return Some(offset + column);
        }
        offset += line.len();
    }
    None
}

fn helper_column(test: &TestSummary, loan: &HelperLoan, parameter: &str) -> Option<Vec<String>> {
    let index = loan.parameters.iter().position(|name| name == parameter)?;
    if loan.call_lines.is_empty() {
        return None;
    }
    loan.call_lines
        .iter()
        .map(|line| {
            let text = test.body.lines().nth(line.checked_sub(test.start_line)?)?;
            call_arguments(text, &loan.name)?.get(index).cloned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail(signature: &str, changed: &str) -> Option<ShortCircuitTail> {
        ShortCircuitTail::parse(&format!("{signature}\n    {changed}\n}}"), changed)
    }

    #[test]
    fn reads_homogeneous_comparison_chains_over_parameters() {
        for (function, changed) in [
            ("fn is_origin(x: i32, y: i32) -> bool {", "y == 0 && x == 0"),
            (
                "fn is_weekend(day: u8) -> bool {",
                "day % 7 == 6 || day % 7 == 0",
            ),
            ("fn f(a: bool, b: u8) -> bool {", "!a && b >= 3 && 10 > b"),
            ("fn f(c: char, n: i64) -> bool {", "c == 'x' || n / 2 != -1"),
        ] {
            assert!(tail(function, changed).is_some(), "{changed}");
        }
    }

    #[test]
    fn refuses_anything_that_can_call_panic_or_mix_operators() {
        for (function, changed) in [
            // Mixed operators need precedence.
            ("fn f(a: bool, b: bool, c: bool) -> bool {", "a || b && c"),
            // A call, index, field, cast, parenthesis or method.
            ("fn f(a: bool, b: u8) -> bool {", "a || check(b)"),
            ("fn f(a: bool, xs: &[u8]) -> bool {", "a && xs[0] == 1"),
            ("fn f(&self, a: bool) -> bool {", "a && self.on"),
            ("fn f(a: bool, b: u8) -> bool {", "a && b as u32 == 1"),
            ("fn f(a: bool, b: u8) -> bool {", "a && (b == 1)"),
            ("fn f(a: bool, s: &str) -> bool {", "a && s.is_empty()"),
            // Arithmetic that can overflow, or a division by a variable.
            ("fn f(a: u8, b: u8) -> bool {", "a == 0 || a + b == 3"),
            ("fn f(a: u8, b: u8) -> bool {", "b != 0 && a / b == 3"),
            ("fn f(a: u8) -> bool {", "a != 1 && a % 0 == 3"),
            // A constant, a float, two literals, a macro.
            ("fn f(a: u8) -> bool {", "a == LIMIT && a > 1"),
            ("fn f(a: f64) -> bool {", "a > 1.5 && a < 2.0"),
            ("fn f(a: u8) -> bool {", "1 == 1 && a > 1"),
            ("fn f(a: u8) -> bool {", "a > 1 && matches!(a, 3)"),
            // `!` binds to the atom, not the comparison.
            ("fn f(x: i32, y: i32) -> bool {", "!x == 0 && y == 0"),
            // An escaped char could name one char two ways.
            ("fn f(c: char, y: u8) -> bool {", "c == '\\n' || y == 0"),
            // A receiver or a pattern parameter.
            ("fn f(self, a: u8) -> bool {", "a > 1 && a < 3"),
            ("fn f((a, b): (u8, u8)) -> bool {", "a > 1 && b < 3"),
            // Generic signatures are not read.
            ("fn f<T: Into<u8>>(a: u8) -> bool {", "a > 1 && a < 3"),
            // A statement before the chain could rebind a parameter.
            (
                "fn f(x: i32, y: i32) -> bool {\n    let x = x + 1;",
                "x == 0 && y == 1",
            ),
        ] {
            assert!(tail(function, changed).is_none(), "{changed}");
        }
    }

    #[test]
    fn evaluates_operands_on_literal_values() -> Result<(), String> {
        let chain = tail("fn f(x: i32, y: i32) -> bool {", "x == 0 && y == 1").ok_or("chain")?;
        let value = |x: &str, y: &str| {
            chain.operands[0].evaluate(&[Some(x.to_string()), Some(y.to_string())])
        };
        assert_eq!(value("0", "9"), Some(true));
        assert_eq!(value("1", "9"), Some(false));
        assert_eq!(
            chain.operands[0].evaluate(&[None, Some("1".to_string())]),
            None
        );
        let weekend =
            tail("fn f(day: u8) -> bool {", "day % 7 == 6 || day % 7 == 0").ok_or("weekend")?;
        assert_eq!(
            weekend.operands[0].evaluate(&[Some("13".to_string())]),
            Some(true)
        );
        assert_eq!(
            weekend.operands[0].evaluate(&[Some("7".to_string())]),
            Some(false)
        );
        let negated = tail("fn f(a: bool, b: u8) -> bool {", "!!!a || b > 1").ok_or("negated")?;
        assert_eq!(
            negated.operands[0].evaluate(&[Some("true".to_string()), None]),
            Some(false)
        );
        Ok(())
    }
}
