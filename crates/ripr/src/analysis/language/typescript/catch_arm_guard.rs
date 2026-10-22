//! A changed line inside a `catch` arm that no related test can reach
//! (RIPR-SPEC-0246 rule 5, #9437).
//!
//! `catch { return fallback; }` runs only when the guarded call throws or
//! rejects. When no related test file holds any signal that could make a call
//! throw or reject, the changed line never runs under the suite, so an exact
//! oracle on the happy path passes on both versions. The rule only withdraws
//! credit, and only when both facts are positively established; anything it
//! cannot read keeps the ladder verdict.

use super::ts_mask_literals_and_comments;

/// Lower-case fragments that may make a call throw or reject, explicitly or
/// through bad input. Deliberately generous: any hit keeps the ladder verdict.
const THROW_SIGNALS: [&str; 20] = [
    "reject",
    "throw",
    "error",
    "exception",
    "fail",
    "abort",
    "timeout",
    "catch",
    "boom",
    "invalid",
    "bad",
    "malformed",
    "corrupt",
    "parse",
    "missing",
    "nonexist",
    "null",
    "undefined",
    "empty",
    "nan",
];

/// `true` when the text of a related test file holds a signal that could make
/// a guarded call throw or reject. Comments and string contents count too.
pub(crate) fn may_throw_or_reject(test_file_text: &str) -> bool {
    let lower = test_file_text.to_ascii_lowercase();
    THROW_SIGNALS.iter().any(|signal| lower.contains(signal))
}

/// One open `{` block while scanning.
struct Open {
    at: usize,
    /// `catch` clause (with the try body it guards) or `.catch(` handler
    /// (with the receiver text it guards).
    catch_guarding: Option<String>,
    is_try: bool,
}

/// `true` when the changed line is located exactly in the owner's source and
/// sits inside a `catch (...) { }` clause or a `.catch(` handler body, and the
/// code that catch guards is plain: it calls only owner parameters and holds no
/// `new`, member access, indexing, `throw` or immediately invoked expression.
/// A guarded call that can fail by itself (`JSON.parse`, a null dereference)
/// keeps the ladder verdict.
pub(crate) fn changed_line_in_catch_arm(
    owner_source: &str,
    start_line: usize,
    changed_line: usize,
    line_text: &str,
    params: &[String],
) -> bool {
    let wanted = line_text.trim();
    if wanted.is_empty()
        || wanted.starts_with('}')
        || owner_source.contains('\r')
        || !owner_source.is_ascii()
    {
        return false;
    }
    let lines: Vec<&str> = owner_source.lines().collect();
    let Some(index) = changed_line
        .checked_sub(start_line)
        .filter(|at| lines.get(*at).is_some_and(|line| line.trim() == wanted))
    else {
        return false;
    };
    let target: usize = lines.iter().take(index).map(|line| line.len() + 1).sum();
    let masked = ts_mask_literals_and_comments(owner_source);
    if masked.len() != owner_source.len() {
        return false;
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut header_start = 0usize;
    let mut last_closed_try: Option<String> = None;
    for (at, c) in masked.char_indices() {
        if at >= target {
            break;
        }
        match c {
            '{' => {
                let header = masked[header_start..at].trim();
                let after_brace = masked[..header_start].trim_end().ends_with('}');
                let catch_guarding = catch_clause(header, after_brace, last_closed_try.take())
                    .or_else(|| dot_catch_receiver(header));
                stack.push(Open {
                    at,
                    catch_guarding,
                    is_try: header == "try",
                });
                header_start = at + 1;
            }
            '}' => {
                last_closed_try = stack
                    .pop()
                    .filter(|open| open.is_try)
                    .map(|open| masked[open.at + 1..at].to_string());
                header_start = at + 1;
            }
            ';' => header_start = at + 1,
            _ => {}
        }
    }
    let mut guarded = stack
        .iter()
        .filter_map(|open| open.catch_guarding.as_deref());
    let Some(first) = guarded.next() else {
        return false;
    };
    std::iter::once(first)
        .chain(guarded)
        .all(|text| is_plain_guarded_code(text, params))
}

/// A `catch` clause header that directly follows a closed `try` block: the
/// try body it guards.
fn catch_clause(header: &str, after_brace: bool, closed_try: Option<String>) -> Option<String> {
    let rest = header.strip_prefix("catch")?;
    if !after_brace
        || !rest
            .chars()
            .next()
            .is_none_or(|c| c == '(' || c.is_whitespace())
    {
        return None;
    }
    closed_try
}

/// The text before a `{` opens the body of a function passed to `.catch(`:
/// the receiver the handler guards (the header text before `.catch(`).
fn dot_catch_receiver(header: &str) -> Option<String> {
    let at = header.rfind(".catch(")?;
    let after = &header[at + ".catch(".len()..];
    let mut depth = 1i32;
    for c in after.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        // The `.catch(` call closed before the brace: not its handler body.
        if depth == 0 {
            return None;
        }
    }
    (after.contains("=>") || after.contains("function")).then(|| header[..at].to_string())
}

/// `true` when every statement of masked guarded code is exactly
/// `[return] [await] param(<identifier or literal args>)`. Any operator, member
/// access, nested call or other form (`0 in read()`, `read() + 1`, `new X`,
/// `JSON.parse(s)`) can fail on its own and keeps the ladder verdict.
fn is_plain_guarded_code(text: &str, params: &[String]) -> bool {
    let mut statements = 0usize;
    for statement in text.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if !is_plain_statement(statement, params) {
            return false;
        }
        statements += 1;
    }
    statements > 0
}

fn strip_word<'a>(text: &'a str, word: &str) -> &'a str {
    match text.strip_prefix(word) {
        Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
        _ => text,
    }
}

fn is_plain_statement(statement: &str, params: &[String]) -> bool {
    let rest = strip_word(statement, "return");
    let rest = strip_word(rest, "await");
    let name_len = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(rest.len());
    let (name, after) = rest.split_at(name_len);
    if name.is_empty() || !params.iter().any(|p| p == name) {
        return false;
    }
    let Some(args) = after
        .trim_start()
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
    else {
        return false;
    };
    args.chars().all(|c| {
        c.is_alphanumeric() || c.is_whitespace() || matches!(c, '_' | '$' | ',' | '"' | '\'' | '`')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "export async function f(read) {\n  try {\n    return await read();\n  } catch {\n    return 1;\n  }\n}\n";

    fn p() -> Vec<String> {
        vec!["read".to_string()]
    }

    fn at(src: &str, line: usize, text: &str) -> bool {
        changed_line_in_catch_arm(src, 1, line, text, &p())
    }

    #[test]
    fn catch_clause_body_is_a_catch_arm() {
        assert!(at(OWNER, 5, "    return 1;"));
        assert!(at(
            "function f(read) {\n  try {\n    read();\n  } catch (e) {\n    b();\n  }\n}\n",
            5,
            "    b();"
        ));
    }

    #[test]
    fn try_body_and_trailing_code_are_not_catch_arms() {
        assert!(!at(OWNER, 3, "    return await read();"));
        let after = "function f(read) {\n  try {\n    read();\n  } catch {\n    b();\n  }\n  return 2;\n}\n";
        assert!(!at(after, 7, "  return 2;"));
    }

    #[test]
    fn dot_catch_handler_body_is_a_catch_arm() {
        let src = "function f(read) {\n  return read().catch((e) => {\n    return 1;\n  });\n}\n";
        assert!(at(src, 3, "    return 1;"));
        let then =
            "function f(read) {\n  return read().catch(h).then((v) => {\n    return 1;\n  });\n}\n";
        assert!(!at(then, 3, "    return 1;"));
    }

    #[test]
    fn a_method_named_catch_is_not_a_catch_clause() {
        let src =
            "class A {\n  run(read) {\n    read();\n  }\n  catch(x) {\n    return 1;\n  }\n}\n";
        assert!(!at(src, 6, "    return 1;"));
    }

    #[test]
    fn a_misplaced_or_closing_line_is_refused() {
        assert!(!at(OWNER, 4, "    return 1;"));
        assert!(!at(OWNER, 4, "  } catch {"));
        assert!(!at(OWNER, 6, "  }"));
    }

    #[test]
    fn guarded_code_that_can_fail_by_itself_is_refused() {
        for body in [
            "return JSON.parse(s);",
            "return new URL(s);",
            "return s.length;",
            "return other(read());",
            "return 0 in read();",
            "return read() + 1;",
            "return await read() instanceof A;",
            "return -read();",
            "return list[0];",
            "return (() => 1)();",
            "throw 1;",
        ] {
            let src = format!(
                "function f(read, s) {{\n  try {{\n    {body}\n  }} catch {{\n    return 1;\n  }}\n}}\n"
            );
            assert!(
                !changed_line_in_catch_arm(
                    &src,
                    1,
                    5,
                    "    return 1;",
                    &["read".into(), "s".into()]
                ),
                "{body}"
            );
        }
    }

    #[test]
    fn throw_signals_are_generous() {
        for text in [
            "mockRejectedValue(1)",
            "() => { throw 1 }",
            "Promise.reject(1)",
            "parse('not json')",
            "f(null)",
            "f(undefined)",
            "f(NaN)",
            "f('')empty",
        ] {
            assert!(may_throw_or_reject(text), "{text}");
        }
        assert!(!may_throw_or_reject("loadOrDefault(async () => 42, 0)"));
    }
}
