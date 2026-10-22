//! RIPR-SPEC-0243 rule 19: a method call replaced by `void <local>;` and
//! counted by a `node:test` mock.
//!
//! `sink.write(line);` becoming `void line;` removes the call and keeps the
//! local alive for the linter. When a related test installs
//! `const write = mock.method(sink, "write", ...)` and asserts a positive
//! `write.mock.callCount()` (or `write.mock.calls.length`) with a strong
//! equality, the removed call lowers that count, so the assertion sees the
//! change. The `void <local>;` line has no probe shape of its own, so this
//! module reads the pair of lines and the test; it never credits by itself.

use super::*;

/// The removed `receiver.method(...)` statement of `old_line`, when
/// `line_text` is exactly `void <identifier>;` and `old_line` is one
/// optionally awaited, two-segment method-call statement.
pub(crate) fn removed_method_call(
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<(String, String)> {
    let discarded = line_text
        .trim()
        .strip_prefix("void ")?
        .strip_suffix(';')?
        .trim();
    if !is_plain_identifier(discarded) {
        return None;
    }
    let old = old_line_text?.trim();
    let old = old.strip_prefix("await ").unwrap_or(old);
    let call = old.strip_suffix(';').unwrap_or(old).trim_end();
    let open = call.find('(')?;
    // The first `(` must close at the end: one call, not `a.b(x) + c.d(y)`.
    let mut depth = 0usize;
    for (at, ch) in call.char_indices().skip(open) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && at + 1 != call.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let (receiver, method) = call[..open].split_once('.')?;
    (is_plain_identifier(receiver) && is_plain_identifier(method) && receiver != "this")
        .then(|| (receiver.to_string(), method.to_string()))
}

fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Whether some oracle-eligible related test spies on `receiver.method` with
/// `mock.method(receiver, "method", ...)` bound to a local and asserts a
/// positive call count on that local with a strong assertion.
pub(crate) fn call_count_pins_removed_call(
    receiver: &str,
    method: &str,
    candidates: &[TypeScriptRelatedCandidate<'_>],
) -> bool {
    candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            spy_bindings(&candidate.test.body_text, receiver, method)
                .iter()
                .any(|binding| {
                    candidate.test.assertions.iter().any(|assertion| {
                        assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
                            && matches!(assertion.oracle_kind, OracleKind::ExactValue)
                            && observes_call_count(assertion, binding)
                            && assertion
                                .expected_value_or_variant
                                .as_deref()
                                .and_then(|value| value.trim().parse::<u64>().ok())
                                .is_some_and(|count| count > 0)
                    })
                })
        })
}

fn observes_call_count(assertion: &TypeScriptAssertion, binding: &str) -> bool {
    let observed: String = assertion
        .observed_expression
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    observed == format!("{binding}.mock.callCount()")
        || observed == format!("{binding}.mock.calls.length")
}

/// Locals bound to `mock.method(receiver, "method", ...)` or
/// `t.mock.method(receiver, "method", ...)` by a `const` / `let` in `body`.
fn spy_bindings(body: &str, receiver: &str, method: &str) -> Vec<String> {
    let mut bindings = Vec::new();
    // Find the call in code only: a string or comment that spells it binds
    // nothing. The mask keeps offsets, so arguments are read from `body`.
    let masked = ts_mask_literals_and_comments(body);
    if masked.len() != body.len() {
        return bindings;
    }
    let mut search_from = 0;
    while let Some(found) = masked[search_from..].find("mock.method(") {
        let at = search_from + found;
        search_from = at + "mock.method(".len();
        let args = &body[search_from..];
        let Some(first) = args.strip_prefix(receiver) else {
            continue;
        };
        let Some(second) = first.trim_start().strip_prefix(',') else {
            continue;
        };
        let second = second.trim_start();
        let quoted_method = [format!("\"{method}\""), format!("'{method}'")];
        if !quoted_method.iter().any(|quoted| {
            second
                .strip_prefix(quoted.as_str())
                .is_some_and(|rest| rest.trim_start().starts_with([',', ')']))
        }) {
            continue;
        }
        // `const write = ` (optionally through a `t.` / `ctx.` receiver).
        let mut before = body[..at].trim_end();
        if let Some(stripped) = before.strip_suffix('.') {
            before = stripped.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
        }
        let Some(before) = before.trim_end().strip_suffix('=') else {
            continue;
        };
        let before = before.trim_end();
        // Step by whole characters: a byte offset after a multi-byte
        // identifier character would not be a char boundary.
        let name_start = before
            .char_indices()
            .rev()
            .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '$'))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let name = &before[name_start..];
        let keyword = before[..name_start].trim_end();
        if is_plain_identifier(name) && (keyword.ends_with("const") || keyword.ends_with("let")) {
            bindings.push(name.to_string());
        }
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_call_needs_void_local_and_one_method_call() {
        assert_eq!(
            removed_method_call("  void line;", Some("  sink.write(line);")),
            Some(("sink".to_string(), "write".to_string()))
        );
        assert_eq!(
            removed_method_call("void line;", Some("await sink.write(line);")),
            Some(("sink".to_string(), "write".to_string()))
        );
        // Not a bare discard, or not a single two-segment call.
        assert_eq!(
            removed_method_call("line;", Some("sink.write(line);")),
            None
        );
        assert_eq!(
            removed_method_call("void sink.write(line);", Some("sink.write(line);")),
            None
        );
        assert_eq!(
            removed_method_call("void line;", Some("a.b(x) + c.d(y);")),
            None
        );
        assert_eq!(removed_method_call("void line;", Some("a.b.c(x);")), None);
        assert_eq!(removed_method_call("void line;", Some("this.b(x);")), None);
        assert_eq!(removed_method_call("void line;", None), None);
    }

    #[test]
    fn spy_binding_reads_mock_method_on_the_removed_receiver_only() {
        let body = "const write = mock.method(sink, \"write\", () => undefined);";
        assert_eq!(spy_bindings(body, "sink", "write"), vec!["write"]);
        let via_context = "const w = t.mock.method(sink, 'write');";
        assert_eq!(spy_bindings(via_context, "sink", "write"), vec!["w"]);
        assert!(spy_bindings(body, "sink", "flush").is_empty());
        assert!(spy_bindings(body, "other", "write").is_empty());
        // An unbound spy cannot be counted.
        assert!(spy_bindings("mock.method(sink, \"write\");", "sink", "write").is_empty());
        // A non-ASCII local name must not split a character.
        assert!(
            spy_bindings(
                "const naïve = mock.method(sink, \"write\");",
                "sink",
                "write"
            )
            .is_empty()
        );
        assert!(
            spy_bindings("const é = mock.method(sink, \"write\");", "sink", "write").is_empty()
        );
        assert!(
            spy_bindings(
                "const ünï_x = mock.method(sink, \"write\");",
                "sink",
                "write"
            )
            .is_empty()
        );
        // A spy spelled inside a string or a comment binds nothing.
        let phantom = "const note = 'const write = mock.method(sink, \"write\")';\n// const write = mock.method(sink, \"write\");\n";
        assert!(spy_bindings(phantom, "sink", "write").is_empty());
    }
}
