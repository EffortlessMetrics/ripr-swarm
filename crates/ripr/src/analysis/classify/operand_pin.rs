//! #7077: a constructed field pinned only where it equals one of its operands.
//!
//! `total_cents: subtotal + shipping` is credited by
//! `assert_eq!(q.total_cents, 9_000)`. When the same test also pins
//! `q.subtotal_cents` (initialized from `subtotal`) to `9_000`, the field
//! equals that operand for that input, so replacing the field with the
//! operand alone passes the test, whatever the operator. When every test
//! that pins the field is paired this way, the exact oracle does not
//! discriminate the change.

use super::super::rust_index::TestSummary;
use crate::domain::OracleKind;

/// Token carried in the discriminate summary when [`operand_only_pin`] holds.
pub(in crate::analysis) const FIELD_PINNED_EQUAL_TO_OPERAND: &str = "field_pinned_equal_to_operand";

/// The field whose pin cannot tell the changed initializer from one of its
/// operands, the sibling field that pins that operand, and the other operand
/// the pinning tests never vary.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::analysis) struct OperandOnlyPin {
    pub field: String,
    pub sibling: String,
    pub unseen_operand: String,
}

impl OperandOnlyPin {
    pub(in crate::analysis) fn summary(&self) -> String {
        format!(
            "Discriminator unconfirmed: every test that pins `{field}` also pins `{sibling}` to the same value ({FIELD_PINNED_EQUAL_TO_OPERAND}), so `{field}` equals that operand there and dropping `{unseen}` passes; pin `{field}` for an input where `{unseen}` changes the result",
            field = self.field,
            sibling = self.sibling,
            unseen = self.unseen_operand,
        )
    }
}

/// Whether the changed field initializer `expression` (`total_cents:
/// subtotal + shipping`) is pinned only by tests that pin a sibling field,
/// bound in the same struct literal of `owner_body` to one operand, to the
/// same expected value. At least one related test must pin the field, and
/// the receiver must be bound from a call to `owner_name`. Anything ripr
/// cannot read returns `None`, which leaves the finding as it was.
pub(in crate::analysis) fn operand_only_pin(
    expression: &str,
    owner_name: &str,
    owner_body: &str,
    tests: &[&TestSummary],
) -> Option<OperandOnlyPin> {
    let (field, left, right) = binary_field_initializer(expression)?;
    let siblings = sibling_initializers(owner_body, expression)?;
    let mut pinning_tests = 0usize;
    let mut paired: Option<OperandOnlyPin> = None;
    for test in tests {
        let pins = exact_field_pins(test);
        let field_pins = pins
            .iter()
            .filter(|(_, pinned_field, _)| *pinned_field == field)
            .collect::<Vec<_>>();
        if field_pins.is_empty() {
            continue;
        }
        pinning_tests += 1;
        let pair = field_pins.iter().find_map(|(receiver, _, value)| {
            if !bound_from_owner_call(test.body.as_str(), receiver, owner_name) {
                return None;
            }
            siblings.iter().find_map(|(sibling, operand)| {
                let unseen = if *operand == left {
                    right
                } else if *operand == right {
                    left
                } else {
                    return None;
                };
                pins.iter()
                    .any(|(other_receiver, other_field, other_value)| {
                        other_receiver == receiver && other_field == sibling && other_value == value
                    })
                    .then(|| OperandOnlyPin {
                        field: field.to_string(),
                        sibling: sibling.to_string(),
                        unseen_operand: unseen.to_string(),
                    })
            })
        });
        let pair = pair?;
        paired.get_or_insert(pair);
    }
    (pinning_tests > 0).then_some(paired).flatten()
}

/// `name: left <op> right` with plain identifier operands and one binary
/// operator, written with spaces around it.
fn binary_field_initializer(expression: &str) -> Option<(&str, &str, &str)> {
    let trimmed = expression.trim().trim_end_matches(',').trim();
    let (name, value) = trimmed.split_once(':')?;
    if value.starts_with(':') {
        return None;
    }
    let name = name.trim();
    let parts = value.split_whitespace().collect::<Vec<_>>();
    let [left, operator, right] = parts.as_slice() else {
        return None;
    };
    let binary = matches!(*operator, "+" | "-" | "*" | "/" | "%" | "|" | "^" | "&");
    (binary && is_identifier(name) && is_identifier(left) && is_identifier(right) && left != right)
        .then_some((name, *left, *right))
}

/// The other `field: operand` initializers of the struct literal that holds
/// `expression` in `body`, each with a plain identifier value. Shorthand
/// `operand,` names a field of the same name.
fn sibling_initializers<'a>(body: &'a str, expression: &str) -> Option<Vec<(&'a str, &'a str)>> {
    let at = body.find(expression.trim())?;
    if body[at + 1..].contains(expression.trim()) {
        return None;
    }
    let open = enclosing_open_brace(body, at)?;
    let close = matching_close_brace(body, open)?;
    let inner = &body[open + 1..close];
    let mut siblings = Vec::new();
    for item in top_level_items(inner) {
        let (name, value) = match item.split_once(':') {
            Some((name, value)) => (name.trim(), value.trim()),
            None => (item, item),
        };
        if is_identifier(name) && is_identifier(value) {
            siblings.push((name, value));
        }
    }
    Some(siblings)
}

/// The `{` that opens the innermost brace group containing `at`.
fn enclosing_open_brace(body: &str, at: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in body.as_bytes()[..at].iter().enumerate().rev() {
        match byte {
            b'}' => depth += 1,
            b'{' if depth == 0 => return Some(offset),
            b'{' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn matching_close_brace(body: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in body.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Comma-separated items of `text` at bracket depth zero, trimmed and
/// non-empty.
fn top_level_items(text: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (offset, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                items.push(text[start..offset].trim());
                start = offset + 1;
            }
            _ => {}
        }
    }
    items.push(text[start..].trim());
    items.into_iter().filter(|item| !item.is_empty()).collect()
}

/// `(receiver, field, expected)` for each exact `assert_eq!(receiver.field,
/// expected)` in `test`, either argument order, with numeric digit
/// separators removed from the expected side.
fn exact_field_pins(test: &TestSummary) -> Vec<(String, String, String)> {
    test.assertions
        .iter()
        .filter(|assertion| assertion.kind == OracleKind::ExactValue)
        .filter_map(|assertion| {
            let text = assertion.text.trim();
            let inner = text
                .strip_prefix("assert_eq!(")?
                .trim_end_matches(';')
                .strip_suffix(')')?;
            let items = top_level_items(inner);
            let [first, second] = items.as_slice() else {
                return None;
            };
            let (receiver, field, expected) = field_read(first)
                .map(|(receiver, field)| (receiver, field, *second))
                .or_else(|| {
                    field_read(second).map(|(receiver, field)| (receiver, field, *first))
                })?;
            if field_read(expected).is_some() {
                return None;
            }
            let expected = if expected
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'_')
            {
                expected.replace('_', "")
            } else {
                expected.to_string()
            };
            Some((receiver.to_string(), field.to_string(), expected))
        })
        .collect()
}

/// `q.total_cents` -> `("q", "total_cents")`.
fn field_read(text: &str) -> Option<(&str, &str)> {
    let (receiver, field) = text.trim().split_once('.')?;
    (is_identifier(receiver) && is_identifier(field)).then_some((receiver, field))
}

/// Whether `body` binds `receiver` straight from a call to `owner_name`:
/// `let q = quote(..)` or `let q = crate::quote(..)`.
fn bound_from_owner_call(body: &str, receiver: &str, owner_name: &str) -> bool {
    body.lines().any(|line| {
        let Some(rest) = line.trim().strip_prefix("let ") else {
            return false;
        };
        let Some((name, value)) = rest.split_once('=') else {
            return false;
        };
        let name = name.trim();
        let name = name.split(':').next().unwrap_or(name).trim();
        let callee = value.trim().split('(').next().unwrap_or_default().trim();
        name == receiver && (callee == owner_name || callee.rsplit("::").next() == Some(owner_name))
    })
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::OracleFact;
    use crate::analysis::rust_index::TestSummary;
    use crate::domain::OracleStrength;

    const OWNER: &str = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping = shipping_cents(subtotal);
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";

    fn test_with(body_lines: &[&str]) -> TestSummary {
        TestSummary {
            name: "quote_test".to_string(),
            body: body_lines.join("\n").into(),
            assertions: body_lines
                .iter()
                .filter(|line| line.trim_start().starts_with("assert"))
                .map(|line| OracleFact {
                    line: 1,
                    text: line.trim().to_string(),
                    kind: OracleKind::ExactValue,
                    strength: OracleStrength::Strong,
                    observed_tokens: Vec::new(),
                    ok_value_observed: None,
                })
                .collect(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body_lines.len(),
            calls: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn pin(tests: &[TestSummary]) -> Option<OperandOnlyPin> {
        let refs = tests.iter().collect::<Vec<_>>();
        operand_only_pin("total_cents: subtotal + shipping", "quote", OWNER, &refs)
    }

    #[test]
    fn a_total_pinned_equal_to_its_subtotal_names_the_unseen_operand() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9000);",
        ])]);

        assert_eq!(
            found,
            Some(OperandOnlyPin {
                field: "total_cents".to_string(),
                sibling: "subtotal_cents".to_string(),
                unseen_operand: "shipping".to_string(),
            })
        );
    }

    #[test]
    fn different_pinned_values_discriminate_the_operand() {
        let found = pin(&[test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.subtotal_cents, 1_000);",
            "assert_eq!(q.total_cents, 1_499);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn one_unpaired_pinning_test_keeps_the_credit() {
        let found = pin(&[
            test_with(&[
                "let q = quote(2_500, 4);",
                "assert_eq!(q.subtotal_cents, 9_000);",
                "assert_eq!(q.total_cents, 9_000);",
            ]),
            test_with(&[
                "let q = quote(1_000, 1);",
                "assert_eq!(q.total_cents, 1_499);",
            ]),
        ]);

        assert_eq!(found, None);
    }

    #[test]
    fn a_receiver_not_bound_from_the_owner_is_not_read() {
        let found = pin(&[test_with(&[
            "let q = other(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn no_test_pinning_the_field_is_not_this_rule() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn only_a_plain_binary_initializer_is_read() {
        assert_eq!(
            binary_field_initializer("total: a + b"),
            Some(("total", "a", "b"))
        );
        assert_eq!(binary_field_initializer("total: a + b + c"), None);
        assert_eq!(binary_field_initializer("total: f(a) + b"), None);
        assert_eq!(binary_field_initializer("total: a + a"), None);
        assert_eq!(binary_field_initializer("total: a::b + c"), None);
    }
}
