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
use crate::domain::{OracleKind, RelationReason};

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
/// subtotal + shipping`) is pinned only where it equals one operand: some
/// sibling field of the same struct literal in `owner_body` is bound to that
/// operand, and every exact pin on the field in every related test sits
/// beside a pin of that sibling, on the same receiver, to the same literal.
/// Each receiver must be bound once, straight from a call to `owner_name`.
/// At least one related test must pin the field. Any mention of the field
/// that is not such a pin (a custom message, another assertion macro, a
/// binding read out of the result) returns `None`, as does anything else
/// ripr cannot read, which leaves the finding as it was.
pub(in crate::analysis) fn operand_only_pin(
    expression: &str,
    owner_name: &str,
    owner_body: &str,
    tests: &[(&TestSummary, RelationReason)],
) -> Option<OperandOnlyPin> {
    let (field, left, right) = binary_field_initializer(expression)?;
    let siblings = sibling_initializers(owner_body, expression)?;
    let mut tested = Vec::new();
    for (test, reason) in tests {
        let pins = exact_field_pins(test);
        let field_pins = pins
            .iter()
            .filter(|(_, pinned_field, _)| pinned_field == field)
            .collect::<Vec<_>>();
        let unread_mention = test.assertions.iter().any(|assertion| {
            whole_word_count(&assertion.text, field) > 0
                && !(assertion.kind == OracleKind::ExactValue
                    && exact_field_pins_of(&assertion.text)
                        .is_some_and(|(_, pinned_field, _)| pinned_field == field))
        });
        if unread_mention || whole_word_count(test.body.as_str(), field) != field_pins.len() {
            return None;
        }
        if field_pins.is_empty() {
            // A test that runs the owner and asserts without naming the field
            // may still observe it: whole-struct equality, a snapshot, or a
            // helper (`assert_quote(&q, 1_499)`), including a helper that
            // calls the owner for it (`assert_eq!(make_quote(), expected)`).
            let reaches_owner = matches!(
                reason,
                RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall
            ) || whole_word_count(test.body.as_str(), owner_name) > 0
                || test
                    .assertions
                    .iter()
                    .any(|assertion| whole_word_count(&assertion.text, owner_name) > 0);
            if reaches_owner && !test.assertions.is_empty() {
                return None;
            }
            continue;
        }
        // A pin under a condition, a loop, a closure or after an early exit
        // may never run (`if false { assert_eq!(q.subtotal_cents, 9_000) }`),
        // so a pinning test with any such construct is not read.
        if has_control_flow(test.body.as_str()) {
            return None;
        }
        if field_pins.iter().any(|(receiver, _, _)| {
            !bound_once_from_owner_call(test.body.as_str(), receiver, owner_name)
        }) {
            return None;
        }
        // The same test may also observe the whole result beside its pins
        // (`assert_eq!(q, expected_quote())`, `assert!(q.is_valid())`), which
        // can see the operand the pins leave out. A plain read of another
        // field (`assert_eq!(q.tier, Tier::Gold)`) cannot.
        let whole_result_check = test.assertions.iter().any(|assertion| {
            whole_word_count(&assertion.text, owner_name) > 0
                || field_pins
                    .iter()
                    .any(|(receiver, _, _)| uses_more_than_field_reads(&assertion.text, receiver))
        });
        if whole_result_check {
            return None;
        }
        tested.push((
            pins.clone(),
            field_pins.into_iter().cloned().collect::<Vec<_>>(),
        ));
    }
    if tested.is_empty() {
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
        tested
            .iter()
            .all(|(pins, field_pins)| {
                field_pins.iter().all(|(receiver, _, value)| {
                    pins.iter()
                        .any(|(other_receiver, other_field, other_value)| {
                            other_receiver == receiver
                                && other_field == sibling
                                && other_value == value
                        })
                })
            })
            .then(|| OperandOnlyPin {
                field: field.to_string(),
                sibling: sibling.to_string(),
                unseen_operand: unseen.to_string(),
            })
    })
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
/// expected)` in `test`, either argument order.
fn exact_field_pins(test: &TestSummary) -> Vec<(String, String, String)> {
    test.assertions
        .iter()
        .filter(|assertion| assertion.kind == OracleKind::ExactValue)
        .filter_map(|assertion| exact_field_pins_of(&assertion.text))
        .collect()
}

/// One `assert_eq!(receiver.field, expected)` with exactly two arguments
/// and a literal expected side: an integer (digit separators removed), a
/// string, a char or a bool. A non-literal expected side (`it.next()`,
/// `expected`) may differ between two pins with the same text.
fn exact_field_pins_of(text: &str) -> Option<(String, String, String)> {
    let inner = text
        .trim()
        .strip_prefix("assert_eq!(")?
        .trim_end_matches(';')
        .strip_suffix(')')?;
    let items = top_level_items(inner);
    let [first, second] = items.as_slice() else {
        return None;
    };
    let (receiver, field, expected) = field_read(first)
        .map(|(receiver, field)| (receiver, field, *second))
        .or_else(|| field_read(second).map(|(receiver, field)| (receiver, field, *first)))?;
    let expected = literal_value(expected)?;
    Some((receiver.to_string(), field.to_string(), expected))
}

fn literal_value(text: &str) -> Option<String> {
    let text = text.trim();
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.is_empty()
        && digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_')
    {
        return Some(text.replace('_', ""));
    }
    let quoted = text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')));
    (quoted || text == "true" || text == "false").then(|| text.to_string())
}

/// Occurrences of `word` in `text` not joined to an identifier character on
/// either side.
fn whole_word_count(text: &str, word: &str) -> usize {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(word)
        .filter(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(is_ident)
                && !text[at + word.len()..].chars().next().is_some_and(is_ident)
        })
        .count()
}

/// Whether `text` uses `receiver` other than as a plain field read
/// (`q.total_cents`): the whole value, a method call, or a reference.
fn uses_more_than_field_reads(text: &str, receiver: &str) -> bool {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(receiver).any(|(at, _)| {
        if text[..at].chars().next_back().is_some_and(is_ident) {
            return false;
        }
        let rest = &text[at + receiver.len()..];
        if rest.chars().next().is_some_and(is_ident) {
            return false;
        }
        let Some(after_dot) = rest.strip_prefix('.') else {
            return true;
        };
        let name_len = after_dot
            .find(|ch: char| !is_ident(ch))
            .unwrap_or(after_dot.len());
        name_len == 0 || after_dot[name_len..].trim_start().starts_with(['(', ':'])
    })
}

/// `q.total_cents` -> `("q", "total_cents")`.
fn field_read(text: &str) -> Option<(&str, &str)> {
    let (receiver, field) = text.trim().split_once('.')?;
    (is_identifier(receiver) && is_identifier(field)).then_some((receiver, field))
}

/// Whether `body` binds `receiver` exactly once, straight from a call to
/// `owner_name`: `let q = quote(..)` or `let q = crate::quote(..)`. A second
/// `let q` may shadow the owner's value (`let q = q.with_coupon(..)`).
fn bound_once_from_owner_call(body: &str, receiver: &str, owner_name: &str) -> bool {
    let bindings = body
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("let ")?;
            let (name, value) = rest.split_once('=')?;
            let name = name.trim();
            let name = name.split(':').next().unwrap_or(name).trim();
            (name == receiver).then_some(value)
        })
        .collect::<Vec<_>>();
    let [value] = bindings.as_slice() else {
        return false;
    };
    let callee = value.trim().split('(').next().unwrap_or_default().trim();
    callee == owner_name || callee.rsplit("::").next() == Some(owner_name)
}

/// Whether `body` holds a construct that may skip or repeat a statement:
/// a condition, a match, a loop, a closure, an early exit or `?`. Comments
/// and strings count too, which only refuses more.
fn has_control_flow(body: &str) -> bool {
    [
        "if", "match", "for", "while", "loop", "return", "break", "continue",
    ]
    .iter()
    .any(|word| whole_word_count(body, word) > 0)
        || body.contains(['?', '|'])
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
        let refs = tests
            .iter()
            .map(|test| (test, RelationReason::DirectOwnerCall))
            .collect::<Vec<_>>();
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

    /// Review of #7077: a second receiver in the same test that pins the
    /// field to a different value discriminates the dropped operand.
    #[test]
    fn a_second_receiver_pinning_the_field_keeps_the_credit() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "let r = quote(1_000, 1);",
            "assert_eq!(r.total_cents, 1_499);",
        ])]);

        assert_eq!(found, None);
    }

    /// Review of #7077: a pin ripr cannot parse (a custom message) still
    /// mentions the field, so the rule fails closed.
    #[test]
    fn an_unread_pin_on_the_field_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let with_message = test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499, \"shipping added\");",
        ]);
        let read_out = test_with(&[
            "let total = quote(1_000, 1).total_cents;",
            "assert_eq!(total, 1_499);",
        ]);

        assert_eq!(pin(&[paired.clone(), with_message]), None);
        assert_eq!(pin(&[paired, read_out]), None);
    }

    #[test]
    fn a_shadowed_receiver_is_not_read() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "let q = q.with_coupon(5);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn a_non_literal_expected_side_is_not_read() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, expected);",
            "assert_eq!(q.total_cents, expected);",
        ])]);

        assert_eq!(found, None);
    }

    /// One test sees the total equal the subtotal and another sees it equal
    /// the shipping: each dropped operand is caught by one of them.
    #[test]
    fn tests_pairing_different_operands_keep_the_credit() {
        let found = pin(&[
            test_with(&[
                "let q = quote(2_500, 4);",
                "assert_eq!(q.subtotal_cents, 9_000);",
                "assert_eq!(q.total_cents, 9_000);",
            ]),
            test_with(&[
                "let q = quote(0, 4);",
                "assert_eq!(499, q.shipping_cents);",
                "assert_eq!(499, q.total_cents);",
            ]),
        ]);

        assert_eq!(found, None);
    }

    /// Re-review of #7077: a test that runs the owner and observes the
    /// whole result never names the field, yet may see the dropped operand.
    #[test]
    fn a_whole_result_observer_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let whole = test_with(&["assert_eq!(quote(1_000, 1), expected_quote());"]);
        let unrelated = test_with(&["assert_eq!(discounted(10_000), 9_000);"]);

        assert_eq!(pin(&[paired.clone(), whole]), None);
        assert!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &[
                    (&paired, RelationReason::DirectOwnerCall),
                    (&unrelated, RelationReason::SameModule),
                ],
            )
            .is_some()
        );
    }

    /// Codex review of #7084: a sibling pin that may not run cannot show
    /// the field equals the operand.
    #[test]
    fn a_conditional_sibling_pin_keeps_the_credit() {
        let found = pin(&[test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499);",
            "if false { assert_eq!(q.subtotal_cents, 1_499); }",
        ])]);

        assert_eq!(found, None);
    }

    /// Codex review of #7084: a test related through a helper that calls
    /// the owner may check the whole result without naming the owner; one
    /// related only through proximity does not block the rule.
    #[test]
    fn a_helper_related_whole_result_check_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let through_helper = test_with(&["assert_eq!(make_quote(), expected());"]);
        let check = |reason| {
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &[
                    (&paired, RelationReason::DirectOwnerCall),
                    (&through_helper, reason),
                ],
            )
        };

        assert_eq!(check(RelationReason::HelperOwnerCall), None);
        assert!(check(RelationReason::SameModule).is_some());
    }

    /// CodeRabbit review of #7084: the pinning test itself may also check
    /// the whole result, which sees the operand the pins leave out.
    #[test]
    fn a_pinning_test_that_also_checks_the_whole_result_keeps_the_credit() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_eq!(q, expected_quote());",
        ])]);
        let method = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert!(q.is_consistent());",
        ])]);
        let other_field = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.tier, Tier::Gold);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
        assert_eq!(method, None);
        assert!(other_field.is_some());
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
