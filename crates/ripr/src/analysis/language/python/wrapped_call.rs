//! Opaque-wrapper detection for owner calls inside an assertion (#6585).
//!
//! `assert always_true(parse("1")) == True` and `assert bool(parse("1")) ==
//! True` name the owner, but the asserted value is the wrapper's result,
//! which may ignore the owner's output. The module-call credit branch
//! already requires the owner call to be an asserted operand
//! (`oracle_operand_calls`); this rule gives the name, alias and
//! changed-sink credit branches the same guard. Value-preserving builtins
//! (`str`, `len`, `sorted`, `round`, ...) stay transparent, so
//! `assert str(parse("1")) == "2"` still credits.
//!
//! The rule only refuses credit when the assertion calls the owner and every
//! such call sits inside a non-transparent wrapper. An assertion that does
//! not call the owner by name keeps its existing verdict.

use super::related_tests::{is_python_identifier_char, oracle_operand_depth};

/// Callees whose result still depends on the argument's value, so a strong
/// comparison on the result can notice a changed argument.
const TRANSPARENT_WRAPPERS: &[&str] = &[
    "str",
    "int",
    "float",
    "complex",
    "list",
    "tuple",
    "set",
    "frozenset",
    "dict",
    "sorted",
    "reversed",
    "len",
    "sum",
    "min",
    "max",
    "abs",
    "round",
    "repr",
    "bytes",
    "bytearray",
    "Decimal",
    "Fraction",
    "approx",
    "isclose",
    "enumerate",
    "zip",
    "map",
    "filter",
    "iter",
    "next",
    "format",
    "ascii",
    "hex",
    "bin",
    "oct",
    "ord",
    "chr",
    "divmod",
    "pow",
    "loads",
    "dumps",
];

/// True when the assertion `text` calls one of `names` and every such call is
/// nested in a call that is not a known value-preserving wrapper.
pub(super) fn owner_call_only_inside_opaque_wrapper(text: &str, names: &[String]) -> bool {
    let Some(operand_depth) = oracle_operand_depth(text) else {
        return false;
    };
    let mut stack: Vec<Option<&str>> = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut calls = 0usize;
    let mut opaque = 0usize;
    for (idx, ch) in text.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '#' => break,
            '(' => {
                stack.push(callee_before(text, idx));
            }
            '[' | '{' => stack.push(None),
            ')' | ']' | '}' => {
                stack.pop();
            }
            _ if is_call_of(text, idx, names) => {
                let wrapped = stack
                    .iter()
                    .skip(operand_depth)
                    .any(|callee| callee.is_some_and(|name| !is_transparent(name)));
                // `other.parse(...)` may be a foreign receiver the bare owner
                // token cannot tell from the owner. Unwrapped, it neither
                // counts as an owner call nor lifts the guard; wrapped, it
                // is refused with the rest.
                let receiver_qualified = text[..idx].ends_with('.');
                if wrapped || !receiver_qualified {
                    calls += 1;
                }
                if wrapped {
                    opaque += 1;
                }
            }
            _ => {}
        }
    }
    calls > 0 && calls == opaque
}

fn is_transparent(callee: &str) -> bool {
    let last = callee.rsplit('.').next().unwrap_or(callee);
    TRANSPARENT_WRAPPERS.contains(&last)
}

/// Python keywords that may precede a grouping `(`. They are not callees.
const GROUPING_KEYWORDS: &[&str] = &[
    "assert", "not", "and", "or", "in", "is", "if", "else", "elif", "return", "yield", "await",
    "lambda", "for", "while", "with", "del", "print",
];

/// Stack marker for a call whose callee is an expression, not a name:
/// `(always_true)(...)`, `table[0](...)`, `make()(...)`. It cannot be shown
/// to preserve its argument, so it counts as opaque.
const EXPRESSION_CALLEE: &str = "<expression>";

/// The callee of the `(` at `open`: the dotted identifier before it, the
/// expression marker when a `)` or `]` precedes it, and `None` for a grouping
/// paren (start of text, an operator, a comma, or a keyword such as `assert`).
fn callee_before(text: &str, open: usize) -> Option<&str> {
    let head = text[..open].trim_end();
    if head.ends_with(')') || head.ends_with(']') {
        return Some(EXPRESSION_CALLEE);
    }
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_python_identifier_char(*ch) || *ch == '.')
        .last()
        .map(|(start, _)| start)?;
    let callee = &head[start..];
    (!callee.is_empty() && !GROUPING_KEYWORDS.contains(&callee)).then_some(callee)
}

/// Whether one of `names` starts at `idx` as a call (`name(`), not as the
/// tail of a longer identifier.
fn is_call_of(text: &str, idx: usize, names: &[String]) -> bool {
    if text[..idx]
        .chars()
        .next_back()
        .is_some_and(is_python_identifier_char)
    {
        return false;
    }
    let rest = &text[idx..];
    names.iter().any(|name| {
        !name.is_empty()
            && rest
                .strip_prefix(name.as_str())
                .is_some_and(|after| after.trim_start().starts_with('('))
    })
}

#[cfg(test)]
mod tests {
    use super::owner_call_only_inside_opaque_wrapper;

    fn names() -> Vec<String> {
        vec!["parse".to_string()]
    }

    #[test]
    fn opaque_wrapper_refuses_credit_only_when_every_call_is_wrapped() {
        for (text, expected) in [
            ("assert always_true(parse('1')) == True", true),
            ("assert bool(parse('1')) == True", true),
            ("assert isinstance(parse('1'), int)", true),
            ("self.assertTrue(always_true(parse('1')))", true),
            ("assert always_true(mod.parse('1')) == True", true),
            // Operand positions and value-preserving wrappers keep credit.
            ("assert parse('1') == 2", false),
            ("assert str(parse('1')) == '2'", false),
            ("assert len(sorted(parse('1'))) == 2", false),
            ("assert [parse('1')] == [2]", false),
            ("self.assertEqual(parse('1'), 2)", false),
            ("self.assertEqual(round(parse('1'), 2), 2)", false),
            // One direct call is enough to keep the verdict.
            ("assert parse('1') == bool(parse('2'))", false),
            // Grouping parens, including after `assert`, are not wrappers.
            ("assert (parse('1')) == 2", false),
            ("assert not (parse('1')) == 2", false),
            // A parenthesized or subscripted callee cannot be shown transparent.
            ("assert (always_true)(parse('1')) == True", true),
            ("assert fns[0](parse('1')) == True", true),
            // A receiver-qualified call may be foreign: it does not lift the
            // guard for a wrapped bare call, nor count when unwrapped.
            ("assert always_true(parse('1')) == other.parse('2')", true),
            ("assert other.parse('2') == 2", false),
            // No call of the owner in the assertion: nothing to refuse.
            ("assert always_true(other('1')) == True", false),
            ("assert always_true('parse(1)') == True", false),
            ("assert unparse('1') == 2", false),
        ] {
            assert_eq!(
                owner_call_only_inside_opaque_wrapper(text, &names()),
                expected,
                "{text}"
            );
        }
    }
}
