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
                calls += 1;
                if stack
                    .iter()
                    .skip(operand_depth)
                    .any(|callee| callee.is_some_and(|name| !is_transparent(name)))
                {
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

/// The dotted identifier immediately before the `(` at `open`, if any.
fn callee_before(text: &str, open: usize) -> Option<&str> {
    let head = text[..open].trim_end();
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_python_identifier_char(*ch) || *ch == '.')
        .last()
        .map(|(start, _)| start)?;
    let callee = &head[start..];
    (!callee.is_empty()).then_some(callee)
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
