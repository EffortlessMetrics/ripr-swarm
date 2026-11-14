//! RIPR-SPEC-0158: a bounded string-parse evaluator for owner-call arguments.
//!
//! A test that hands the owner `level("12")` passes the number the file's
//! parse helper turns `"12"` into, not the text. The argument reader treats a
//! call as the call's operand (RIPR-SPEC-0158), so the boundary operand stays
//! `infection_unknown` even though the parse is a closed computation.
//!
//! This module folds exactly one shape and refuses everything else:
//!
//! - the argument is one whole call `name("DIGITS")`: one string literal of
//!   ASCII decimal digits, no sign, space, separator or escape;
//! - `name` is the one declaration of that name in the test's own file, a free
//!   function the test can see (see `FoldScope::sees_declaration`), not
//!   imported, not generic, not `async`/`const`/`unsafe`/`extern`, not
//!   attribute-gated, with exactly one `&str` parameter and a builtin integer
//!   return type;
//! - the body is nothing but `param.parse().unwrap()`,
//!   `param.parse::<T>().unwrap()` or the same with `.expect("..")`, where `T`
//!   is the return type;
//! - the test body does not bind the helper's name;
//! - the parsed value is representable in the return type and below the
//!   shared evaluator's `i32::MAX` bound.
//!
//! A `Result`-returning parser, a struct parser, a `trim`/`split`/`strip_*`
//! pipeline, a `unwrap_or` default, a second definition, a helper in another
//! file and a literal the parse would reject all keep the argument unread, so
//! the boundary stays `infection_unknown`. The step budget is the fixed shape
//! above: one parse, no loop, no recursion.

use super::{
    FoldScope, binding_pattern_ends, declaration_offsets, declared_in_associated_block,
    file_imports, is_decimal, is_integer_type,
};
use crate::analysis::extract::mask_comments_and_strings;

/// The evaluator's value bound, exclusive: the shared evaluator refuses values
/// at or above `i32::MAX`, so a parsed value never exceeds what it can fold.
const VALUE_BOUND: u128 = i32::MAX as u128;

/// `name("DIGITS")` as one whole argument: the helper name and the digits.
fn parse_call(argument: &str) -> Option<(&str, &str)> {
    let text = argument.trim();
    let open = text.find('(')?;
    let name = text[..open].trim_end();
    if !is_identifier(name) {
        return None;
    }
    let inner = text[open + 1..].strip_suffix(')')?.trim();
    let digits = inner.strip_prefix('"')?.strip_suffix('"')?;
    is_decimal(digits).then_some((name, digits))
}

fn is_identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// The largest value a narrow integer type holds; wider types are bounded by
/// [`VALUE_BOUND`] alone.
fn type_max(ty: &str) -> u128 {
    match ty {
        "u8" => u128::from(u8::MAX),
        "i8" => i8::MAX as u128,
        "u16" => u128::from(u16::MAX),
        "i16" => i16::MAX as u128,
        _ => u128::MAX,
    }
}

/// Whether only a visibility modifier stands between the previous item and
/// the `fn` at byte `at` of the masked file. An attribute, `async`, `const`,
/// `unsafe` or `extern` qualifier is not that, and the call may then not be
/// the plain function the scan read.
fn only_visibility_before(masked: &str, at: usize) -> bool {
    let before = &masked[..at];
    let start = before.rfind([';', '{', '}']).map_or(0, |index| index + 1);
    let qualifiers = before[start..].trim();
    qualifiers.is_empty()
        || qualifiers == "pub"
        || (qualifiers.starts_with("pub(")
            && qualifiers.ends_with(')')
            && !qualifiers.contains("#"))
}

/// The parameter, return type and brace-trimmed body of the free function
/// whose text starts `fn name` in `masked`.
fn read_function<'a>(masked: &'a str, name: &str) -> Option<(&'a str, &'a str, &'a str)> {
    let rest = masked.strip_prefix("fn")?.trim_start();
    let rest = rest.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix('(')?;
    let (parameters, rest) = rest.split_once(')')?;
    let rest = rest.trim_start().strip_prefix("->")?;
    let (return_type, rest) = rest.split_once('{')?;
    let mut depth = 1usize;
    let mut end = None;
    for (offset, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(offset);
                    break;
                }
            }
            _ => {}
        }
    }
    Some((parameters, return_type.trim(), rest[..end?].trim()))
}

/// The one `&str` parameter name of `parameters`.
fn str_parameter(parameters: &str) -> Option<&str> {
    let (name, ty) = parameters.trim().trim_end_matches(',').split_once(':')?;
    let name = name.trim();
    let ty: String = ty.split_whitespace().collect();
    (is_identifier(name) && name != "_" && ty == "&str").then_some(name)
}

/// Whether `body` is exactly `parameter.parse().unwrap()` (with an optional
/// turbofish naming `return_type`, and `.expect("..")` for `.unwrap()`).
/// `body` is masked source.
fn is_unwrapped_parse(body: &str, parameter: &str, return_type: &str) -> bool {
    let squeezed: String = body.split_whitespace().collect();
    // The mask blanks a whole string literal, quotes included, so an
    // `.expect("..")` message reads as an empty argument list.
    [
        format!("{parameter}.parse().unwrap()"),
        format!("{parameter}.parse::<{return_type}>().unwrap()"),
        format!("{parameter}.parse().expect()"),
        format!("{parameter}.parse::<{return_type}>().expect()"),
    ]
    .contains(&squeezed)
}

impl FoldScope<'_> {
    /// The decimal value the file's parse helper `name` returns for the
    /// string literal `digits`, when every condition in the module docs holds.
    fn parse_helper_value(self, name: &str, digits: &str) -> Option<String> {
        let source = self.file_source?;
        if file_imports(source, name) {
            return None;
        }
        let masked = mask_comments_and_strings(source);
        let [at] = declaration_offsets(&masked, "fn", name)[..] else {
            return None;
        };
        if declared_in_associated_block(&masked, at)
            || !self.sees_declaration(&masked, at)
            || !only_visibility_before(&masked, at)
        {
            return None;
        }
        let (parameters, return_type, body) = read_function(&masked[at..], name)?;
        let parameter = str_parameter(parameters)?;
        if !is_integer_type(return_type) || !is_unwrapped_parse(body, parameter, return_type) {
            return None;
        }
        let value: u128 = digits.parse().ok()?;
        (value < VALUE_BOUND && value <= type_max(return_type)).then(|| value.to_string())
    }
}

/// The value the whole argument `name("DIGITS")` folds to in a test whose
/// body is `test_body`; `None` for any other argument, or when the test binds
/// the helper's name itself.
pub(super) fn folded_parse_call(
    test_body: &str,
    argument: &str,
    scope: FoldScope<'_>,
) -> Option<String> {
    let (name, digits) = parse_call(argument)?;
    if binding_pattern_ends(test_body, name)?.0 != 0 {
        return None;
    }
    scope.parse_helper_value(name, digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "fn level(text: &str) -> u32 {\n    text.parse().unwrap()\n}\n";

    fn fold(file: &str, body: &str, argument: &str) -> Option<String> {
        folded_parse_call(body, argument, FoldScope::of_source(file, 1))
    }

    const BODY: &str = "fn t() {\n    let _ = bucket(level(\"10\"));\n}";

    #[test]
    fn a_unique_unwrapped_parse_helper_folds_a_digit_literal() {
        assert_eq!(fold(FILE, BODY, "level(\"10\")").as_deref(), Some("10"));
        assert_eq!(
            fold(FILE, BODY, " level ( \"007\" ) ").as_deref(),
            Some("7")
        );
        for helper in [
            "fn level(text: &str) -> u32 { text.parse::<u32>().unwrap() }\n",
            "pub fn level(text: &str) -> u32 { text.parse().expect(\"a number\") }\n",
            "pub(crate) fn level(text: &str) -> u32 { text.parse::<u32>().expect(\"n\") }\n",
            "fn level(text: &str) -> usize { text.parse().unwrap() }\n",
        ] {
            assert_eq!(
                fold(helper, BODY, "level(\"10\")").as_deref(),
                Some("10"),
                "{helper}"
            );
        }
    }

    #[test]
    fn a_helper_that_is_not_the_closed_unwrapped_parse_does_not_fold() {
        for helper in [
            // A second definition makes the callee ambiguous.
            "fn level(text: &str) -> u32 { text.parse().unwrap() }\nfn level(text: &str) -> u32 { 99 }\n",
            "fn level(text: &str) -> u32 { text.parse().unwrap() }\nmod other {\n    pub fn level(text: &str) -> u32 { 99 }\n}\n",
            // A shadowing copy in the test's own module, which the outer one would hide.
            "use super::*;\nfn level(text: &str) -> u32 { text.parse().unwrap() }\nmod inner {\n    use super::*;\n    fn level(text: &str) -> u32 { text.parse::<u32>().unwrap() + 5 }\n}\n",
            // The result is not a bare unwrapped parse.
            "fn level(text: &str) -> u32 { text.parse().unwrap_or(0) }\n",
            "fn level(text: &str) -> u32 { text.trim().parse().unwrap() }\n",
            "fn level(text: &str) -> u32 { text.parse().unwrap() + 1 }\n",
            "fn level(text: &str) -> u32 { text.parse::<u64>().unwrap() as u32 }\n",
            "fn level(text: &str) -> u32 { text.parse::<u64>().unwrap() }\n",
            "fn level(text: &str) -> u32 { other.parse().unwrap() }\n",
            "fn level(text: &str) -> u32 { text.parse().expect(&message()) }\n",
            "fn level(text: &str) -> u32 { let n = text.parse().unwrap(); n }\n",
            "fn level(text: &str) -> u32 { text.parse().unwrap(); }\n",
            "fn level(text: &str) -> u32 { if text.is_empty() { 0 } else { text.parse().unwrap() } }\n",
            // The signature is not one `&str` parameter and an integer result.
            "fn level(text: &str) -> Result<u32, std::num::ParseIntError> { text.parse() }\n",
            "fn level(text: &str) -> Option<u32> { text.parse().ok() }\n",
            "fn level(text: &str) -> String { text.parse().unwrap() }\n",
            "fn level(text: String) -> u32 { text.parse().unwrap() }\n",
            "fn level(text: &str, extra: u32) -> u32 { text.parse().unwrap() }\n",
            "fn level() -> u32 { 10 }\n",
            "fn level<T>(text: &str) -> u32 { text.parse().unwrap() }\n",
            "fn level(text: &str) { text.parse().unwrap() }\n",
            // A qualifier, attribute, import or associated block changes the callee.
            "async fn level(text: &str) -> u32 { text.parse().unwrap() }\n",
            "const fn level(text: &str) -> u32 { text.parse().unwrap() }\n",
            "unsafe fn level(text: &str) -> u32 { text.parse().unwrap() }\n",
            "#[cfg(feature = \"x\")]\nfn level(text: &str) -> u32 { text.parse().unwrap() }\n",
            "use other::level;\nfn level(text: &str) -> u32 { text.parse().unwrap() }\n",
            "impl Rates {\n    fn level(text: &str) -> u32 { text.parse().unwrap() }\n}\n",
            "// fn level(text: &str) -> u32 { text.parse().unwrap() }\n",
            "fn levels(text: &str) -> u32 { text.parse().unwrap() }\n",
        ] {
            assert_eq!(fold(helper, BODY, "level(\"10\")"), None, "{helper}");
        }
    }

    #[test]
    fn a_literal_the_parse_would_reject_or_the_evaluator_cannot_hold_does_not_fold() {
        for argument in [
            "level(\"-3\")",
            "level(\"+3\")",
            "level(\" 3\")",
            "level(\"3 \")",
            "level(\"1_0\")",
            "level(\"1.5\")",
            "level(\"0x10\")",
            "level(\"\")",
            "level(\"ten\")",
            "level(\"10\", \"11\")",
            "level(10)",
            "level(text)",
            "level(r\"10\")",
            "level(\"10\").max(3)",
            "level(\"10\") + 1",
            "other::level(\"10\")",
            "level(\"2147483647\")",
            "level(\"99999999999999999999999999999999999999999\")",
        ] {
            assert_eq!(fold(FILE, BODY, argument), None, "`{argument}`");
        }
        let narrow = "fn level(text: &str) -> u8 { text.parse().unwrap() }\n";
        assert_eq!(fold(narrow, BODY, "level(\"255\")").as_deref(), Some("255"));
        assert_eq!(
            fold(narrow, BODY, "level(\"256\")"),
            None,
            "unwrap would panic"
        );
    }

    #[test]
    fn a_helper_name_the_test_rebinds_or_cannot_see_does_not_fold() {
        let rebound =
            "fn t() {\n    let level = |s: &str| 3;\n    let _ = bucket(level(\"10\"));\n}";
        assert_eq!(fold(FILE, rebound, "level(\"10\")"), None);
        let nested = format!("{FILE}fn t() {{\n    fn level(s: &str) -> u32 {{ 3 }}\n}}\n");
        assert_eq!(fold(&nested, BODY, "level(\"10\")"), None);
        let sibling = "mod other {\n    fn level(text: &str) -> u32 { text.parse().unwrap() }\n}\nfn t() {}\n";
        assert_eq!(fold(sibling, BODY, "level(\"10\")"), None);
        assert_eq!(
            folded_parse_call(BODY, "level(\"10\")", FoldScope::default()),
            None,
            "a test file the index does not hold"
        );
    }
}
