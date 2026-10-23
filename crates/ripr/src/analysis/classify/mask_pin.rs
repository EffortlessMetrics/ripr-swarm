//! #9444: a masked integer cast pinned only where the mask drops nothing.
//!
//! `(n & 0xff) as u8` is credited by `assert_eq!(low_byte(0x1234), 0x34)`,
//! yet the pin only fixes the bits of the input the result keeps. A mask
//! bit the pinned input never sets cannot be told from a narrower mask:
//! `(n & 0x7f) as u8` also returns `0x34`. Clearing a mask bit changes the
//! result exactly when the input sets that bit, so the exact oracle
//! discriminates the mask only when the literal inputs together set every
//! mask bit the cast keeps.

use super::super::rust_index::{RustIndex, TestSummary};
use super::operand_pin::{
    has_control_flow, is_identifier, matching_paren, top_level_items, whole_word_count,
};
use crate::domain::{OracleKind, RelationReason};

/// Token carried in the discriminate summary when [`unexercised_mask_bits`] holds.
pub(in crate::analysis) const MASK_BITS_UNEXERCISED: &str = "mask_bits_unexercised";

/// The mask bits no pinned input sets.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::analysis) struct UnexercisedMask {
    pub mask: String,
    pub bits: u64,
}

impl UnexercisedMask {
    pub(in crate::analysis) fn summary(&self) -> String {
        format!(
            "Discriminator unconfirmed: no pinned input sets mask bits {bits:#x} of `{mask}` ({MASK_BITS_UNEXERCISED}), so a narrower mask returns the same value; pin an input that sets them",
            bits = self.bits,
            mask = self.mask,
        )
    }
}

/// Whether the changed expression `(param & MASK) as <int>` is pinned only
/// by literal inputs that leave some kept mask bit clear. Every related test
/// that names the owner must be a straight-line run of exact
/// `assert_eq!(owner(LITERAL), LITERAL)` pins; any other way a test reaches
/// the owner returns `None`, as does a mask the cast keeps entirely, which
/// leaves the finding as it was.
pub(in crate::analysis) fn unexercised_mask_bits(
    expression: &str,
    owner_name: &str,
    owner_body: &str,
    tests: &[(&TestSummary, RelationReason)],
    index: &RustIndex,
) -> Option<UnexercisedMask> {
    let (param, mask_text, width) = masked_cast(expression)?;
    if !owner_has_param(owner_body, param) || operand_rebound(owner_body, param) {
        return None;
    }
    let mask = integer_literal(mask_text)?;
    let kept = mask & width_mask(width);
    if kept == 0 {
        return None;
    }
    // A same-name function elsewhere could be the one the tests call.
    if index
        .functions()
        .iter()
        .filter(|function| function.name == owner_name)
        .count()
        != 1
    {
        return None;
    }
    let mut set_bits = 0u64;
    let mut pinned = false;
    for (test, reason) in tests {
        if whole_word_count(test.body.as_str(), owner_name) == 0 {
            // A test that never names the owner is related by proximity
            // alone, unless it reaches the owner through a helper.
            if matches!(reason, RelationReason::HelperOwnerCall) {
                return None;
            }
            continue;
        }
        if has_control_flow(test.body.as_str()) {
            return None;
        }
        let mut seen = 0usize;
        for assertion in &test.assertions {
            if whole_word_count(&assertion.text, owner_name) == 0 {
                continue;
            }
            if assertion.kind != OracleKind::ExactValue {
                return None;
            }
            set_bits |= pinned_input(&assertion.text, owner_name)?;
            seen += 1;
            pinned = true;
        }
        if seen != whole_word_count(test.body.as_str(), owner_name) {
            return None;
        }
    }
    let missing = kept & !set_bits;
    (pinned && missing != 0).then(|| UnexercisedMask {
        mask: mask_text.trim().to_string(),
        bits: missing,
    })
}

/// `(param & MASK) as <int>` with a plain parameter and nothing else.
fn masked_cast(expression: &str) -> Option<(&str, &str, u32)> {
    let text = expression.trim().trim_end_matches(';').trim();
    let inner = text.strip_prefix('(')?;
    let close = matching_paren(text, 0)?;
    let rest = text[close + 1..].trim().strip_prefix("as ")?.trim();
    let width = int_width(rest)?;
    let (param, mask) = inner[..close - 1].split_once(" & ")?;
    let param = param.trim();
    is_identifier(param).then_some((param, mask.trim(), width))
}

/// Whether the owner body past its signature may rewrite or shadow `param`
/// before the mask (`let n = n | 0x80;`, `n += 1`, a closure or pattern
/// binding): the pinned input then no longer reaches the mask unchanged.
/// Any binding construct or closure bar refuses, as does an assignment.
fn operand_rebound(owner_body: &str, param: &str) -> bool {
    let Some(open) = owner_body.find('(') else {
        return true;
    };
    let Some(close) = matching_paren(owner_body, open) else {
        return true;
    };
    let body = &owner_body[close + 1..];
    if body.contains('|')
        || ["let", "match", "for", "while", "loop", "if", "mut"]
            .iter()
            .any(|word| whole_word_count(body, word) > 0)
    {
        return true;
    }
    body.match_indices(param).any(|(at, _)| {
        let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
        if body[..at].chars().next_back().is_some_and(ident)
            || body[at + param.len()..].chars().next().is_some_and(ident)
        {
            return false;
        }
        let after = body[at + param.len()..].trim_start();
        (after.starts_with('=') && !after.starts_with("=="))
            || ["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="]
                .iter()
                .any(|op| after.starts_with(op))
    })
}

fn owner_has_param(owner_body: &str, param: &str) -> bool {
    let Some(open) = owner_body.find('(') else {
        return false;
    };
    let Some(close) = matching_paren(owner_body, open) else {
        return false;
    };
    top_level_items(&owner_body[open + 1..close])
        .iter()
        .any(|item| {
            item.split_once(':')
                .is_some_and(|(name, _)| name.trim() == param)
        })
}

fn int_width(ty: &str) -> Option<u32> {
    match ty {
        "u8" | "i8" => Some(8),
        "u16" | "i16" => Some(16),
        "u32" | "i32" => Some(32),
        "u64" | "i64" => Some(64),
        _ => None,
    }
}

fn width_mask(width: u32) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

/// An unsigned integer literal: decimal, `0x`, `0b` or `0o`, with `_`
/// separators and an optional type suffix.
fn integer_literal(text: &str) -> Option<u64> {
    let text = text.trim().replace('_', "");
    let (digits, radix) = if let Some(rest) = text.strip_prefix("0x") {
        (rest, 16)
    } else if let Some(rest) = text.strip_prefix("0b") {
        (rest, 2)
    } else if let Some(rest) = text.strip_prefix("0o") {
        (rest, 8)
    } else {
        (text.as_str(), 10)
    };
    let end = digits
        .find(|ch: char| !ch.is_digit(radix))
        .unwrap_or(digits.len());
    let (number, suffix) = digits.split_at(end);
    if number.is_empty() || !(suffix.is_empty() || int_width(suffix).is_some()) {
        return None;
    }
    u64::from_str_radix(number, radix).ok()
}

/// The literal input of one `assert_eq!(owner(LITERAL), LITERAL)`, either
/// argument order, with exactly two arguments.
fn pinned_input(text: &str, owner_name: &str) -> Option<u64> {
    let inner = text
        .trim()
        .strip_prefix("assert_eq!(")?
        .trim_end_matches(';')
        .strip_suffix(')')?;
    let items = top_level_items(inner);
    let [first, second] = items.as_slice() else {
        return None;
    };
    let (call, expected) = if first.starts_with(owner_name) {
        (*first, *second)
    } else {
        (*second, *first)
    };
    integer_literal(expected)?;
    let args = call.strip_prefix(owner_name)?.strip_prefix('(')?;
    let args = args.strip_suffix(')')?;
    let args = top_level_items(args);
    let [arg] = args.as_slice() else {
        return None;
    };
    integer_literal(arg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::{FunctionSourceRole, OracleFact, OwnedRustIndex};
    use crate::analysis::rust_index::{FileFacts, FunctionFact};
    use crate::domain::{OracleStrength, SymbolId};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const OWNER: &str = "pub fn low_byte(n: u32) -> u8 {\n    (n & 0xff) as u8\n}";

    fn index() -> RustIndex {
        let path = PathBuf::from("src/lib.rs");
        let function = FunctionFact {
            id: SymbolId("lib::low_byte".to_string()),
            name: "low_byte".to_string(),
            file: path.clone(),
            start_line: 1,
            end_line: 3,
            body: OWNER.into(),
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
        RustIndex::from_owned(OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path,
                    functions: vec![function.clone()],
                    source: "".into(),
                    ..FileFacts::default()
                },
            )]),
            functions: vec![function],
            ..Default::default()
        })
    }

    fn test_with(body_lines: &[&str]) -> TestSummary {
        TestSummary {
            name: "low_byte_test".to_string(),
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

    fn unexercised(expression: &str, tests: &[TestSummary]) -> Option<UnexercisedMask> {
        let refs = tests
            .iter()
            .map(|test| (test, RelationReason::DirectOwnerCall))
            .collect::<Vec<_>>();
        unexercised_mask_bits(expression, "low_byte", OWNER, &refs, &index())
    }

    #[test]
    fn a_pin_that_leaves_mask_bits_clear_names_them() {
        let tests = [test_with(&["assert_eq!(low_byte(0x1234), 0x34);"])];
        // 0x34 sets 0b0011_0100; the 0xff mask keeps 0b1100_1011 unexercised.
        let found = unexercised("(n & 0xff) as u8", &tests);
        assert_eq!(found.as_ref().map(|mask| mask.bits), Some(0xcb));
        assert!(
            found
                .iter()
                .all(|mask| mask.summary().contains(MASK_BITS_UNEXERCISED))
        );
    }

    #[test]
    fn inputs_that_together_set_every_mask_bit_keep_the_credit() {
        let tests = [
            test_with(&["assert_eq!(low_byte(0x0f), 0x0f);"]),
            test_with(&["assert_eq!(low_byte(0xf0), 0xf0);"]),
        ];
        assert_eq!(unexercised("(n & 0xff) as u8", &tests), None);
        let one = [test_with(&["assert_eq!(low_byte(0xffff), 0xff);"])];
        assert_eq!(unexercised("(n & 0xff) as u8", &one), None);
    }

    #[test]
    fn mask_bits_the_cast_drops_are_not_required() {
        let tests = [test_with(&["assert_eq!(low_byte(0xff), 0xff);"])];
        assert_eq!(unexercised("(n & 0x1ff) as u8", &tests), None);
    }

    #[test]
    fn a_non_literal_input_or_conditional_pin_keeps_the_credit() {
        let variable = [test_with(&["assert_eq!(low_byte(word), 0x34);"])];
        assert_eq!(unexercised("(n & 0xff) as u8", &variable), None);
        let gated = [test_with(&[
            "if cfg!(debug_assertions) {",
            "assert_eq!(low_byte(0x1234), 0x34);",
            "}",
        ])];
        assert_eq!(unexercised("(n & 0xff) as u8", &gated), None);
    }

    #[test]
    fn an_unread_owner_call_keeps_the_credit() {
        let tests = [test_with(&[
            "assert_eq!(low_byte(0x1234), 0x34);",
            "let other = low_byte(7);",
        ])];
        assert_eq!(unexercised("(n & 0xff) as u8", &tests), None);
    }

    #[test]
    fn other_expression_shapes_are_not_read() {
        let tests = [test_with(&["assert_eq!(low_byte(0x1234), 0x34);"])];
        assert_eq!(unexercised("n as u8", &tests), None);
        assert_eq!(unexercised("(m & 0xff) as u8", &tests), None);
        assert_eq!(unexercised("(n & 0xff) as f32", &tests), None);
        assert_eq!(unexercised("(n & MASK) as u8", &tests), None);
    }

    #[test]
    fn an_owner_that_rebinds_the_operand_keeps_the_credit() {
        let tests = [test_with(&["assert_eq!(low_byte(0x1234), 0xb4);"])];
        for body in [
            "pub fn low_byte(n: u32) -> u8 {\n    let n = n | 0x80;\n    (n & 0xff) as u8\n}",
            "pub fn low_byte(n: u32) -> u8 {\n    let n = n << 4;\n    (n & 0xff) as u8\n}",
            "pub fn low_byte(n: u32) -> u8 {\n    let g = |n: u32| n | 0x80;\n    (g(n) & 0xff) as u8\n}",
            "pub fn low_byte(n: u32) -> u8 {\n    n += 1;\n    (n & 0xff) as u8\n}",
            "pub fn low_byte(n: u32) -> u8 {\n    n = n | 0x80;\n    (n & 0xff) as u8\n}",
        ] {
            let refs = tests
                .iter()
                .map(|test| (test, RelationReason::DirectOwnerCall))
                .collect::<Vec<_>>();
            assert_eq!(
                unexercised_mask_bits("(n & 0xff) as u8", "low_byte", body, &refs, &index()),
                None,
                "{body}"
            );
        }
    }
}
