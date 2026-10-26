//! A removed operand-type guard on `__eq__`, read through a test that compares
//! an instance with a foreign literal (#9619).
//!
//! `return isinstance(other, Money) and self.cents == other.cents` ->
//! `return self.cents == other.cents` changes nothing for another `Money`, so
//! only a comparison against a value of another type observes it. This pass
//! credits exactly one shape and leaves every other equality dunder to the
//! `dynamic_dispatch` abstention (#9421):
//!
//! - the edit deletes the leading `isinstance(other, <own class>) and` of the
//!   only statement of `__eq__`, with the rest of the line unchanged;
//! - the remaining expression is exactly `self.<a> == other.<b>`: no call, no
//!   boolean operator or conditional that could skip the read, and `<b>` is a
//!   name the class stores on its instances (`self.<b> = ..`) that no builtin
//!   literal type has, so a builtin operand raises `AttributeError` where the
//!   guard used to answer `False`;
//! - the class defines neither `__ne__` nor `__getattr__` and names no base
//!   class (`int`, `float` and `str` subclasses inherit a `__ne__` that never
//!   consults the edited `__eq__`), so `!=` runs the edited `__eq__` and no
//!   foreign operand answers the attribute read;
//! - a related test asserts `Class(...) != <int, float, str, bool or None
//!   literal>` (or `not ... ==`), and contains no `try`, `with` or `raises`
//!   that could catch the `AttributeError`, so the assertion fails after the
//!   edit.
//!
//! Anything else (a custom foreign class, a computed operand, an `==` read
//! that expects `False`) keeps the abstention.

use super::related_tests::{PythonRelatedCandidate, PythonRelationKind, dunder_method_class};
use super::{PythonAssertion, PythonOwner, PythonTest};

/// Public attributes the accepted literal types (`int`, `float`, `bool`,
/// `str`, `None`) carry; reading one of them off a foreign operand would not
/// raise, so it is never evidence. Dunder names are refused wholesale.
const BUILTIN_ATTRIBUTES: &[&str] = &[
    "capitalize",
    "casefold",
    "center",
    "count",
    "encode",
    "endswith",
    "expandtabs",
    "find",
    "format",
    "format_map",
    "index",
    "isalnum",
    "isalpha",
    "isascii",
    "isdecimal",
    "isdigit",
    "isidentifier",
    "islower",
    "isnumeric",
    "isprintable",
    "isspace",
    "istitle",
    "isupper",
    "join",
    "ljust",
    "lower",
    "lstrip",
    "maketrans",
    "partition",
    "removeprefix",
    "removesuffix",
    "replace",
    "rfind",
    "rindex",
    "rjust",
    "rpartition",
    "rsplit",
    "rstrip",
    "split",
    "splitlines",
    "startswith",
    "strip",
    "swapcase",
    "title",
    "translate",
    "upper",
    "zfill",
    "real",
    "imag",
    "numerator",
    "denominator",
    "conjugate",
    "as_integer_ratio",
    "bit_length",
    "bit_count",
    "to_bytes",
    "from_bytes",
    "is_integer",
    "hex",
    "fromhex",
    "from_number",
];

/// One assertion that fails once the guard is gone.
pub(super) struct TypeGuardCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
}

/// Whether `class Name(...)` lists any base other than `object`. An unknown
/// header is treated as based, which keeps the abstention.
fn class_has_base(source: &str, class: &str) -> bool {
    let header = format!("class {class}");
    let Some(at) = source.find(&header) else {
        return true;
    };
    let rest = source[at + header.len()..].trim_start();
    let Some(rest) = rest.strip_prefix('(') else {
        return !rest.starts_with(':');
    };
    match rest.split_once(')') {
        Some((bases, _)) => !matches!(bases.trim(), "" | "object"),
        None => true,
    }
}

pub(super) fn removed_type_guard_credit<'a>(
    owner: &PythonOwner,
    old_line_text: Option<&str>,
    line_text: &str,
    owner_source: &str,
    related_candidates: &[PythonRelatedCandidate<'a>],
) -> Option<TypeGuardCredit<'a>> {
    if owner.name != "__eq__" {
        return None;
    }
    let class = dunder_method_class(owner)?.rsplit('.').next()?;
    let remaining = removed_guard_remainder(old_line_text?, line_text, class)?;
    if !reads_only_custom_attributes(remaining, owner_source)
        || !owner_is_one_return(owner, owner_source)
        || owner_source.contains("def __ne__")
        || owner_source.contains("__getattr__")
        || class_has_base(owner_source, class)
    {
        return None;
    }
    related_candidates
        .iter()
        .filter(|candidate| candidate.relation == PythonRelationKind::ProtocolEntryCall)
        .filter(|candidate| test_cannot_catch_errors(candidate.test))
        .find_map(|candidate| {
            candidate
                .test
                .assertions
                .iter()
                .find(|assertion| compares_with_foreign_literal(&assertion.gate_text, class))
                .map(|assertion| TypeGuardCredit {
                    test: candidate.test,
                    assertion,
                })
        })
}

/// The expression left after `return isinstance(other, <class>) and` is
/// removed, when the new line is exactly that expression.
fn removed_guard_remainder<'a>(old: &str, new: &'a str, class: &str) -> Option<&'a str> {
    let rest = old
        .trim()
        .strip_prefix("return isinstance(other, ")?
        .split_once(") and ")?;
    let (guarded, rest) = rest;
    let remaining = new.trim().strip_prefix("return ")?.trim();
    (guarded.trim() == class && rest.trim() == remaining && !remaining.is_empty())
        .then_some(remaining)
}

/// The expression is exactly `self.<a> == other.<b>`, where `<b>` is stored on
/// the class's instances and no builtin literal has it. A call, boolean
/// operator or conditional could skip or satisfy the read, so none is allowed.
fn reads_only_custom_attributes(expression: &str, source: &str) -> bool {
    let Some((left, right)) = expression.split_once(" == ") else {
        return false;
    };
    let identifier = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
    };
    let Some(own) = left.trim().strip_prefix("self.") else {
        return false;
    };
    let Some(name) = right.trim().strip_prefix("other.") else {
        return false;
    };
    identifier(own)
        && identifier(name)
        && !name.starts_with("__")
        && !BUILTIN_ATTRIBUTES.contains(&name)
        && (source.contains(&format!("self.{name} =")) || source.contains(&format!("self.{name}=")))
}

/// A test that cannot swallow the `AttributeError` the assertion relies on.
fn test_cannot_catch_errors(test: &PythonTest) -> bool {
    ![
        "try:",
        "except",
        "with ",
        "raises",
        "suppress",
        "contextlib",
    ]
    .iter()
    .any(|marker| test.body_text.contains(marker))
}

/// `__eq__` is its `def` line plus the one `return` statement.
fn owner_is_one_return(owner: &PythonOwner, source: &str) -> bool {
    let body: Vec<&str> = source
        .lines()
        .skip(owner.start_line.saturating_sub(1))
        .take(owner.end_line.saturating_sub(owner.start_line) + 1)
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('@'))
        .collect();
    matches!(body.as_slice(), [def, ret] if def.starts_with("def __eq__") && ret.starts_with("return "))
}

/// `assert Class(..) != <literal>`, the sides in either order, or the negated
/// `==` spelling.
fn compares_with_foreign_literal(gate_text: &str, class: &str) -> bool {
    let text = gate_text.trim();
    let text = text.strip_prefix("assert ").unwrap_or(text).trim();
    let (negated, text) = match text.strip_prefix("not ") {
        Some(rest) => (true, rest.trim()),
        None => (false, text),
    };
    let operator = if negated { "==" } else { "!=" };
    let other = if negated { "!=" } else { "==" };
    if text.matches(operator).count() != 1 || text.contains(other) {
        return false;
    }
    let Some((left, right)) = text.split_once(operator) else {
        return false;
    };
    let (left, right) = (left.trim(), right.trim());
    (is_constructor(left, class) && is_foreign_literal(right))
        || (is_foreign_literal(left) && is_constructor(right, class))
}

/// `Class(<plain arguments>)`: one balanced call, no strings or brackets.
fn is_constructor(text: &str, class: &str) -> bool {
    let Some(arguments) = text
        .strip_prefix(class)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    arguments
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ' ' | ',' | '.' | '-'))
}

fn is_foreign_literal(text: &str) -> bool {
    let number = text.strip_prefix('-').unwrap_or(text);
    let quoted = text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')))
        && !text[1..text.len() - 1].contains(['"', '\'', '\\']);
    matches!(text, "None" | "True" | "False")
        || quoted
        || (!number.is_empty()
            && number.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
            && number.matches('.').count() <= 1
            && number.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_class_header_with_any_base_is_based() {
        assert!(!class_has_base("class Money:\n    pass", "Money"));
        assert!(!class_has_base("class Money():\n    pass", "Money"));
        assert!(!class_has_base("class Money(object):\n    pass", "Money"));
        assert!(class_has_base("class Money(int):\n    pass", "Money"));
        assert!(class_has_base(
            "class Money(Base, object):\n    pass",
            "Money"
        ));
        assert!(class_has_base("class Other:\n    pass", "Money"));
    }

    use super::*;

    #[test]
    fn guard_removal_needs_the_exact_one_line_shape() {
        let old = "        return isinstance(other, Money) and self.cents == other.cents";
        let new = "        return self.cents == other.cents";
        assert_eq!(
            removed_guard_remainder(old, new, "Money"),
            Some("self.cents == other.cents")
        );
        assert_eq!(removed_guard_remainder(old, new, "Cash"), None);
        assert_eq!(
            removed_guard_remainder(old, "        return self.cents == 1", "Money"),
            None
        );
    }

    #[test]
    fn only_a_plain_stored_attribute_comparison_counts() {
        let source = "class Money:\n    def __init__(self, cents):\n        self.cents = cents\n        self.count = cents\n        self.real = cents\n";
        assert!(reads_only_custom_attributes(
            "self.cents == other.cents",
            source
        ));
        // Attributes `int` or `str` literals also carry would not raise.
        for name in ["count", "real", "__hash__"] {
            assert!(
                !reads_only_custom_attributes(&format!("self.cents == other.{name}"), source),
                "{name}"
            );
        }
        // A name the class never stores on its instances (a property, a method).
        assert!(!reads_only_custom_attributes(
            "self.cents == other.total",
            source
        ));
        // Anything beyond the one comparison could skip or satisfy the read.
        for expression in [
            "False and other.cents",
            "self.cents == other.cents and other.cents",
            "self.cents == other.cents.upper()",
            "self.cents == (other.cents if self else 0)",
            "self.cents == getattr(other, 'cents')",
            "self.cents == 1",
            "other.cents == other",
        ] {
            assert!(
                !reads_only_custom_attributes(expression, source),
                "{expression}"
            );
        }
    }

    #[test]
    fn foreign_literal_comparisons_are_recognized_in_both_orders() {
        for text in [
            "assert Money(1) != 1",
            "assert 1 != Money(1)",
            "assert Money(1) != 'x'",
            "assert Money(1) != None",
            "assert not Money(1) == 1.5",
        ] {
            assert!(compares_with_foreign_literal(text, "Money"), "{text}");
        }
        for text in [
            "assert Money(1) == 1",
            "assert Money(1) != Money(2)",
            "assert Money(1) != [1]",
            "assert Money(1) != other",
            "assert Cash(1) != 1",
            "assert Money(1) != 1 != 2",
            "assert not Money(1) != 1",
        ] {
            assert!(!compares_with_foreign_literal(text, "Money"), "{text}");
        }
    }
}
