use super::{delimited_contents_at, enum_variant_values};
use crate::analysis::extract::mask_comments_and_strings;

pub(in crate::analysis) fn exact_error_variant(text: &str) -> Option<String> {
    let open = match find_err_segment(text, "Err(") {
        Some(start) => start + "Err".len(),
        // `Err::<T, E>(..)` constructs the same error: without this the
        // turbofish spelling escaped the RIPR-SPEC-0106 sibling-variant
        // binding, so any exact error pin could confirm it.
        None => turbofish_err_open(text)?,
    };
    let inner = delimited_contents_at(text, open)?;
    let inner = inner.trim();
    let outer_expression = inner
        .char_indices()
        .find(|(_, ch)| matches!(ch, '(' | '{' | '['))
        .map_or(inner, |(index, _)| &inner[..index])
        .trim();
    let values = enum_variant_values(outer_expression);
    (values.len() == 1).then(|| values[0].clone())
}

/// The exact error variant a changed line produces: an
/// `Err(Type::Variant)` construction ([`exact_error_variant`]) or a
/// statement-level `.ok_or(Type::Variant)?` / `.ok_or_else(|| ..)?`
/// conversion ([`question_mark_error_variant`], #6695).
///
/// The single identity owner for the RIPR-SPEC-0106 sibling-variant gate:
/// the diff-mode reveal gate, the repo-mode seam discriminator, and the
/// repo-mode oracle comparison all read it, so an `ok_or` line cannot be
/// variant-gated on one path and opaque on the other. Whether the `?`
/// returns from the owner (rather than a closure) is a body-level question
/// this line-level reader does not answer; flow owns it.
///
/// When both readers find a variant and they differ (a nested
/// `x.map(|_| Err(E::V)).ok_or(E::W)?`), the line names two errors and the
/// identity is opaque: `None` (fail-closed, PR #6786 review).
///
/// A line that spells `.ok_or` beside an `Err(..)` construction names its
/// error through two routes; unless every qualified variant on it is the
/// constructed one (`Err(E::V).or_else(|_| x.ok_or(E::W))?` is not), the
/// identity is opaque too.
pub(in crate::analysis) fn changed_error_variant(text: &str) -> Option<String> {
    match (exact_error_variant(text), question_mark_error_variant(text)) {
        (Some(constructed), Some(returned)) => (constructed == returned).then_some(constructed),
        (Some(constructed), None) if text.contains(".ok_or") => enum_variant_values(text)
            .iter()
            .all(|value| *value == constructed)
            .then_some(constructed),
        (constructed, returned) => constructed.or(returned),
    }
}

/// Byte offset of the first `pattern` (`Err(` or `Err::<`) whose `Err` is a
/// whole path segment of code: `MyErr::<E>(E::V)` constructs a custom type,
/// not a `Result::Err`, so a substring hit must not bind the error identity
/// (#7094 review). Any non-ASCII character before `Err` continues an
/// identifier (a combining mark in `My\u{301}Err`), and a spelling inside a
/// string literal or comment (`return Ok(0); // was Err(E::X)`) is not a
/// construction. The mask keeps byte offsets, so callers read the original
/// text at the returned offset.
/// `Result::Err(..)` and `std::result::Result::Err(..)` still match.
fn find_err_segment(text: &str, pattern: &str) -> Option<usize> {
    let code = mask_comments_and_strings(text);
    code.match_indices(pattern)
        .map(|(start, _)| start)
        .find(|start| {
            !code[..*start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || !ch.is_ascii())
        })
}

/// Whether `text` spells a `Result::Err` construction, `Err(..)` or
/// `Err::<..>(..)`, with `Err` as a whole path segment.
pub(in crate::analysis) fn spells_result_err(text: &str) -> bool {
    find_err_segment(text, "Err(").is_some() || turbofish_err_open(text).is_some()
}

/// Byte offset of the `(` that opens the argument of the first
/// `Err::<..>(` turbofish constructor, or `None`.
fn turbofish_err_open(text: &str) -> Option<usize> {
    let start = find_err_segment(text, "Err::<")?;
    let generics = start + "Err::".len();
    let mut depth = 0i32;
    for (offset, ch) in text[generics..].char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    let after = generics + offset + 1;
                    return text[after..].starts_with('(').then_some(after);
                }
            }
            _ => {}
        }
    }
    None
}

/// The error variant a statement-level `?` returns from an `Option`
/// conversion (#6695): `<receiver>.ok_or(Type::Variant)?` or
/// `<receiver>.ok_or_else(|| Type::Variant)?`, optionally bound
/// (`let d = ..;`). On `None` that `?` returns `Err(Type::Variant)` from
/// the enclosing function, so the variant is the changed error's identity.
///
/// Fail-closed (`None`) unless every condition holds:
/// - exactly one `.ok_or(`/`.ok_or_else(` call in the text, at delimiter
///   depth zero (not inside a call argument, closure body or block);
/// - its `?` immediately follows the call's closing paren and ends the
///   statement (only an optional `;` after it);
/// - no `|` sits at depth zero before the call (a closure head would make
///   the `?` return from the closure, not the function);
/// - the argument is a qualified variant path (`Type::Variant`, upper-case
///   final segment), optionally with one balanced payload, and for
///   `ok_or_else` it is the body of a parameterless `||` closure.
///
/// Whether the line itself sits inside a closure or async block of the
/// owner is a body-level question the caller decides (see
/// `flow::question_mark_returns_from_owner`).
pub(in crate::analysis) fn question_mark_error_variant(text: &str) -> Option<String> {
    let text = text.trim();
    let statement = text.strip_suffix(';').unwrap_or(text).trim_end();
    let body = statement.strip_suffix('?')?;
    let (call_at, name) = [".ok_or_else(", ".ok_or("]
        .iter()
        .find_map(|name| body.rfind(name).map(|at| (at, *name)))?;
    if body.matches(".ok_or(").count() + body.matches(".ok_or_else(").count() != 1 {
        return None;
    }
    let receiver = &body[..call_at];
    if receiver.trim().is_empty() || !depth_zero_without_pipe(receiver) {
        return None;
    }
    let open = call_at + name.len() - 1;
    let argument = delimited_contents_at(body, open)?;
    // The call's closing paren must be the last character before `?`.
    if open + argument.len() + 2 != body.len() {
        return None;
    }
    let argument = argument.trim();
    let value = if name == ".ok_or_else(" {
        argument.strip_prefix("||")?.trim()
    } else {
        argument
    };
    let head: String = value
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == ':')
        .collect();
    let payload = value[head.len()..].trim();
    let payload_ok = payload.is_empty()
        || (payload.starts_with('(')
            && delimited_contents_at(payload, 0)
                .is_some_and(|inner| inner.len() + 2 == payload.len()));
    let values = enum_variant_values(&head);
    (payload_ok && values.len() == 1 && values[0] == head).then_some(head)
}

/// Every delimiter opened in `text` is closed again and no `|` sits at depth
/// zero: the text after it starts back at statement level, outside any
/// closure head.
fn depth_zero_without_pipe(text: &str) -> bool {
    let mut depth = 0i32;
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            '|' if depth == 0 => return false,
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_error_variant_is_opaque_when_the_two_readers_disagree() {
        assert_eq!(
            changed_error_variant("let v = x.map(|_| Err(E::V)).ok_or(E::W)?;"),
            None
        );
        assert_eq!(
            changed_error_variant("let v = x.map(|_| Err(E::V)).ok_or(E::V)?;").as_deref(),
            Some("E::V")
        );
        assert_eq!(
            changed_error_variant("return Err(E::V);").as_deref(),
            Some("E::V")
        );
        assert_eq!(
            changed_error_variant("let v = x.ok_or(E::W)?;").as_deref(),
            Some("E::W")
        );
        // `.ok_or` beside an `Err(..)` with a different variant, where the
        // ok_or reader itself refuses the shape.
        assert_eq!(
            changed_error_variant("let v = Err(E::V).or_else(|_| x.ok_or(E::W))?;"),
            None
        );
        assert_eq!(
            changed_error_variant("let v = x.ok_or(E::W).and(Err(E::V))?;"),
            None
        );
        assert_eq!(
            changed_error_variant("let v = Err(E::V).or_else(|_| x.ok_or(E::V))?;").as_deref(),
            Some("E::V")
        );
    }

    #[test]
    fn err_constructor_must_be_a_whole_code_segment() {
        for text in [
            "return Err(E::V);",
            "return Result::Err(E::V);",
            "return Err::<i64, E>(E::V);",
            "return std::result::Result::Err::<i64, E>(E::V);",
        ] {
            assert!(spells_result_err(text), "{text}");
            assert_eq!(exact_error_variant(text).as_deref(), Some("E::V"), "{text}");
        }
        for text in [
            "return MyErr::<E>(E::V);",
            "return MyErr(E::V);",
            "return My\u{301}Err::<E>(E::V);",
            "return Ok(0); // was Err(E::V)",
            "return Ok(\"Err::<(), E>(E::V)\");",
            "return Ok(0); /* Err(E::V) */",
        ] {
            assert!(!spells_result_err(text), "{text}");
            assert_eq!(exact_error_variant(text), None, "{text}");
        }
        // The first code occurrence wins even after a commented one.
        assert_eq!(
            exact_error_variant("/* Err(E::W) */ return Err(E::V);").as_deref(),
            Some("E::V")
        );
    }

    #[test]
    fn question_mark_error_variant_reads_ok_or_and_ok_or_else() {
        for text in [
            "let d = digit(c).ok_or(CodeError::NotDigit)?;",
            "let d = digit(c).ok_or_else(|| CodeError::NotDigit)?;",
            "digit(c).ok_or( CodeError::NotDigit )?",
            "let x = s.chars().find(|c| c.is_ascii()).ok_or(CodeError::NotDigit)?;",
        ] {
            assert_eq!(
                question_mark_error_variant(text).as_deref(),
                Some("CodeError::NotDigit"),
                "{text}"
            );
        }
        assert_eq!(
            question_mark_error_variant("let n = v.first().ok_or(E::Wrap(1))?;").as_deref(),
            Some("E::Wrap")
        );
    }

    #[test]
    fn question_mark_error_variant_refuses_every_other_shape() {
        for text in [
            // No `?`: the error is a value, not a return.
            "let d = digit(c).ok_or(CodeError::NotDigit);",
            // `?` not on the ok_or call, or the statement continues.
            "let d = digit(c).ok_or(CodeError::NotDigit).map(f)?;",
            "let d = digit(c).ok_or(CodeError::NotDigit)?.value;",
            // Inside a call argument or a closure.
            "let v = Ok(digit(c).ok_or(CodeError::NotDigit)?);",
            "let f = |c| digit(c).ok_or(CodeError::NotDigit)?;",
            // Argument is not one qualified variant path.
            "let d = digit(c).ok_or(make_error())?;",
            "let d = digit(c).ok_or(NotDigit)?;",
            "let d = digit(c).ok_or(CodeError::NotDigit.into())?;",
            "let d = digit(c).ok_or_else(|c| CodeError::NotDigit)?;",
            "let d = digit(c).ok_or_else(make_error)?;",
            // Two conversions on one line.
            "let d = a.ok_or(E::A)?.b().ok_or(E::B)?;",
        ] {
            assert_eq!(question_mark_error_variant(text), None, "{text}");
        }
    }

    #[test]
    fn exact_error_variant_reads_first_variant_inside_result_error() {
        assert_eq!(
            exact_error_variant("return Err(AuthError::RevokedToken);").as_deref(),
            Some("AuthError::RevokedToken")
        );
    }

    #[test]
    fn exact_error_variant_reads_turbofish_and_qualified_constructors() {
        assert_eq!(
            exact_error_variant("return Err::<i64, PayError>(PayError::Insufficient);").as_deref(),
            Some("PayError::Insufficient")
        );
        assert_eq!(
            exact_error_variant("return Result::Err(PayError::Insufficient);").as_deref(),
            Some("PayError::Insufficient")
        );
        assert_eq!(
            exact_error_variant("return Err::<Vec<u8>, E>(E::Bad);").as_deref(),
            Some("E::Bad")
        );
        assert_eq!(exact_error_variant("let x = Err::<u8, E>;"), None);
    }

    #[test]
    fn exact_error_variant_returns_none_without_result_error() {
        assert_eq!(exact_error_variant("return Ok(value);"), None);
    }

    #[test]
    fn exact_error_variant_preserves_outer_nested_constructor() {
        assert_eq!(
            exact_error_variant("return Err(SomeError::Wrap(Inner::Value));").as_deref(),
            Some("SomeError::Wrap")
        );
    }
}
