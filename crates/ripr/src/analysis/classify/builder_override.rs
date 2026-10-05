//! Builder override: a test that overwrites a changed field before reading it.
//!
//! A `return_value` probe on a struct literal (`RetryBuilder { attempts: 4,
//! delay_ms: 100 }`) is confirmed by `reveal` when an assertion names one of
//! the literal's tokens, typically a field read (`retry.attempts`). When the
//! test sets that same field on the owner's result before reading it
//! (`RetryBuilder::new().attempts(5).build()`), the changed initializer never
//! reaches the assertion: the read observes the test's own value. The shared
//! field token is then coincidence, not observation (RIPR-SPEC-0040 builder
//! override outcomes).
//!
//! The decision is proof-only and fail-closed toward the existing behavior:
//! a field counts as overridden only when the assertion's read is tied to the
//! owner call through the assertion's own chain or through `let` bindings,
//! and a setter-shaped call (`.field(arg)`, `.with_field(arg)`,
//! `.set_field(arg)`) or an assignment (`.field = ..`) on that value is
//! visible. Anything the lexical view cannot tie to the owner leaves the
//! token confirming exactly as before.

/// Upper bound on `let` hops from the assertion's receiver back to the owner
/// call (`retry` <- `builder.build()` <- `RetryBuilder::new()`).
const MAX_BINDING_HOPS: usize = 4;

/// Field names a struct-literal expression initializes:
/// `RetryBuilder { attempts: 4, delay_ms: 100 }` -> `["attempts", "delay_ms"]`.
/// Shorthand initializers (`attempts,`) count; a `..base` spread does not.
/// Empty for anything that is not `Path { .. }`.
pub(super) fn struct_literal_fields(expression: &str) -> Vec<String> {
    let Some(open) = expression.find('{') else {
        return Vec::new();
    };
    let head = expression[..open].trim_end();
    if !head
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Vec::new();
    }
    let mut depth = 0usize;
    let mut close = None;
    for (index, ch) in expression[open..].char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(open + index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    for part in split_top_level_commas(&expression[open + 1..close]) {
        let part = part.trim();
        if part.is_empty() || part.starts_with("..") {
            continue;
        }
        let name = match part.find(':') {
            Some(colon) if part[colon..].starts_with("::") => continue,
            Some(colon) => part[..colon].trim(),
            None => part,
        };
        if is_identifier(name) {
            fields.push(name.to_string());
        }
    }
    fields
}

/// The subset of `fields` that `assertion` reads (`.field`, not a method
/// call) on a value the test overwrote that field on after obtaining it from
/// the `owner` call.
pub(super) fn overridden_fields_read(
    body: &str,
    assertion: &str,
    owner: &str,
    fields: &[String],
) -> Vec<String> {
    let prior = body
        .find(assertion)
        .map_or(body, |position| &body[..position]);
    fields
        .iter()
        .filter(|field| {
            field_reads(assertion, field)
                .into_iter()
                .any(|before| read_is_overridden(prior, before, owner, field))
        })
        .cloned()
        .collect()
}

/// The text before each `.field` read in `assertion` that is not a method
/// call (`.field(`).
fn field_reads<'a>(assertion: &'a str, field: &str) -> Vec<&'a str> {
    let read = format!(".{field}");
    assertion
        .match_indices(&read)
        .filter(|(start, matched)| {
            let rest = &assertion[start + matched.len()..];
            !rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                && !rest.trim_start().starts_with('(')
        })
        .map(|(start, _)| &assertion[..start])
        .collect()
}

fn read_is_overridden(prior: &str, before: &str, owner: &str, field: &str) -> bool {
    // Chain inside the assertion: `RetryBuilder::new().attempts(5).build().attempts`.
    if let Some(after_owner) = after_owner_call(before, owner) {
        return has_setter(after_owner, field);
    }
    let receiver_start = before
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map_or(0, |index| index + 1);
    let receiver = &before[receiver_start..];
    if !is_identifier(receiver) || before[..receiver_start].ends_with('.') {
        return false;
    }
    let mut chased = vec![receiver.to_string()];
    let mut current = receiver.to_string();
    for _ in 0..MAX_BINDING_HOPS {
        let Some(initializer) = let_initializer(prior, &current) else {
            return false;
        };
        if let Some(after_owner) = after_owner_call(initializer, owner) {
            return has_setter(after_owner, field)
                || chased
                    .iter()
                    .any(|binding| mutates_binding(prior, binding, field));
        }
        if has_setter(initializer, field) {
            // `let retry = builder.attempts(5).build();` — still requires the
            // root to reach the owner below before it counts.
            let Some(root) = chain_root(initializer) else {
                return false;
            };
            return reaches_owner(prior, root, owner, chased.len());
        }
        let Some(root) = chain_root(initializer) else {
            return false;
        };
        current = root.to_string();
        chased.push(current.clone());
    }
    false
}

/// Whether binding `root` (or what it is bound from) is initialized from the
/// `owner` call within the remaining hop budget.
fn reaches_owner(prior: &str, root: &str, owner: &str, used: usize) -> bool {
    let mut current = root.to_string();
    for _ in used..MAX_BINDING_HOPS {
        let Some(initializer) = let_initializer(prior, &current) else {
            return false;
        };
        if after_owner_call(initializer, owner).is_some() {
            return true;
        }
        let Some(next) = chain_root(initializer) else {
            return false;
        };
        current = next.to_string();
    }
    false
}

/// Text after the first call of `owner` (a whole-identifier `owner(`) in
/// `text`.
fn after_owner_call<'a>(text: &'a str, owner: &str) -> Option<&'a str> {
    let call = format!("{owner}(");
    text.match_indices(&call).find_map(|(start, matched)| {
        let preceded_by_ident = text[..start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        (!preceded_by_ident).then(|| &text[start + matched.len()..])
    })
}

/// A setter-shaped call that writes `field` with an argument:
/// `.field(5)`, `.with_field(5)`, `.set_field(5)`. A zero-argument
/// `.field()` is a getter and does not count.
fn has_setter(text: &str, field: &str) -> bool {
    [
        field.to_string(),
        format!("with_{field}"),
        format!("set_{field}"),
    ]
    .iter()
    .any(|name| {
        let call = format!(".{name}(");
        text.match_indices(&call)
            .any(|(start, matched)| !text[start + matched.len()..].trim_start().starts_with(')'))
    })
}

/// `text` opens with a setter call on the value before it
/// (`.attempts(5)..`), so the write applies to that value and not to some
/// other receiver later in the statement.
fn starts_with_setter(text: &str, field: &str) -> bool {
    let Some(rest) = text.strip_prefix('.') else {
        return false;
    };
    let end = rest
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    (name == field || name == format!("with_{field}") || name == format!("set_{field}"))
        && rest[end..]
            .strip_prefix('(')
            .is_some_and(|args| !args.trim_start().starts_with(')'))
}

/// A later statement mutating `binding`'s `field`: a setter call on it
/// (`builder.attempts(5)`, `builder = builder.attempts(5)`) or a direct
/// assignment (`builder.attempts = 5`).
fn mutates_binding(prior: &str, binding: &str, field: &str) -> bool {
    let assignment = format!("{binding}.{field}");
    prior.split(';').any(|statement| {
        let receiver_calls = statement.match_indices(binding).any(|(start, matched)| {
            let preceded_by_ident = statement[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
            !preceded_by_ident && starts_with_setter(&statement[start + matched.len()..], field)
        });
        let assigns = statement
            .match_indices(&assignment)
            .any(|(start, matched)| {
                let preceded_by_ident = statement[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
                let rest = statement[start + matched.len()..].trim_start();
                !preceded_by_ident && rest.starts_with('=') && !rest.starts_with("==")
            });
        receiver_calls || assigns
    })
}

/// The initializer of the last `let [mut] binding = ..;` in `prior`.
fn let_initializer<'a>(prior: &'a str, binding: &str) -> Option<&'a str> {
    prior
        .match_indices("let ")
        .filter(|(start, _)| {
            !prior[..*start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
        .filter_map(|(start, _)| {
            let statement = prior[start + 4..].split(';').next().unwrap_or_default();
            let rest = statement.trim_start();
            let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
            let rest = rest.strip_prefix(binding)?;
            if rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                return None;
            }
            let rest = rest.trim_start();
            let rest = match rest.strip_prefix(':') {
                Some(typed) => &typed[typed.find('=')?..],
                None => rest,
            };
            let initializer = rest.strip_prefix('=')?;
            (!initializer.starts_with('=')).then_some(initializer.trim())
        })
        .last()
}

/// The root binding of a method chain initializer (`builder` in
/// `builder.attempts(5).build()`); `None` unless it is `ident.`.
fn chain_root(initializer: &str) -> Option<&str> {
    let end = initializer.find('.')?;
    let root = initializer[..end].trim();
    is_identifier(root).then_some(root)
}

fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        match ch {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
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

    fn fields() -> Vec<String> {
        struct_literal_fields("RetryBuilder { attempts: 4, delay_ms: 100 }")
    }

    #[test]
    fn struct_literal_fields_names_initializers() {
        assert_eq!(
            fields(),
            vec!["attempts".to_string(), "delay_ms".to_string()]
        );
        assert_eq!(
            struct_literal_fields("Self { attempts, delay: Duration::from_millis(1), ..base }"),
            vec!["attempts".to_string(), "delay".to_string()]
        );
        assert!(struct_literal_fields("attempts + 1").is_empty());
        assert!(struct_literal_fields("if x { 1 } else { 2 }").is_empty());
    }

    #[test]
    fn setter_chained_in_the_binding_overrides_the_read_field() {
        let body =
            "let retry = RetryBuilder::new().attempts(5).build();\nassert_eq!(retry.attempts, 5);";
        assert_eq!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 5);", "new", &fields()),
            vec!["attempts".to_string()]
        );
    }

    #[test]
    fn default_read_without_setter_is_not_overridden() {
        let body = "let retry = RetryBuilder::new().build();\nassert_eq!(retry.attempts, 3);";
        assert!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 3);", "new", &fields())
                .is_empty()
        );
    }

    #[test]
    fn setter_on_another_field_leaves_the_read_field_observed() {
        let body =
            "let retry = RetryBuilder::new().delay_ms(5).build();\nassert_eq!(retry.attempts, 3);";
        assert!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 3);", "new", &fields())
                .is_empty()
        );
    }

    #[test]
    fn setter_on_an_unrelated_value_does_not_override() {
        let body = "let other = Other::make().attempts(9);\nlet retry = RetryBuilder::new().build();\nassert_eq!(retry.attempts, 3);";
        assert!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 3);", "new", &fields())
                .is_empty()
        );
    }

    #[test]
    fn getter_call_is_not_a_setter() {
        let body =
            "let retry = RetryBuilder::new().attempts().build();\nassert_eq!(retry.attempts, 3);";
        assert!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 3);", "new", &fields())
                .is_empty()
        );
    }

    #[test]
    fn override_through_intermediate_builder_binding() {
        let body = "let mut builder = RetryBuilder::new();\nbuilder = builder.with_attempts(5);\nlet retry = builder.build();\nassert_eq!(retry.attempts, 5);";
        assert_eq!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 5);", "new", &fields()),
            vec!["attempts".to_string()]
        );
        let assigned = "let mut builder = RetryBuilder::new();\nbuilder.attempts = 5;\nlet retry = builder.build();\nassert_eq!(retry.attempts, 5);";
        assert_eq!(
            overridden_fields_read(assigned, "assert_eq!(retry.attempts, 5);", "new", &fields()),
            vec!["attempts".to_string()]
        );
        let untouched = "let builder = RetryBuilder::new();\nlet retry = builder.build();\nassert_eq!(retry.attempts, 3);";
        assert!(
            overridden_fields_read(
                untouched,
                "assert_eq!(retry.attempts, 3);",
                "new",
                &fields()
            )
            .is_empty()
        );
    }

    #[test]
    fn chain_inside_the_assertion_is_checked() {
        let overridden = "assert_eq!(RetryBuilder::new().attempts(5).build().attempts, 5);";
        assert_eq!(
            overridden_fields_read(overridden, overridden, "new", &fields()),
            vec!["attempts".to_string()]
        );
        let default = "assert_eq!(RetryBuilder::new().build().attempts, 3);";
        assert!(overridden_fields_read(default, default, "new", &fields()).is_empty());
    }

    #[test]
    fn receiver_not_bound_from_the_owner_is_undetermined() {
        let body = "let retry = fixture().attempts(5).build();\nassert_eq!(retry.attempts, 5);";
        assert!(
            overridden_fields_read(body, "assert_eq!(retry.attempts, 5);", "new", &fields())
                .is_empty()
        );
    }
}
