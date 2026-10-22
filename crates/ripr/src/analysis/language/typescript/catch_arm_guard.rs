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

/// Lower-case fragments that may make a call throw or reject. Deliberately
/// generous: any hit keeps the ladder verdict.
const THROW_SIGNALS: [&str; 9] = [
    "reject",
    "throw",
    "error",
    "exception",
    "fail",
    "abort",
    "timeout",
    "catch",
    "boom",
];

/// `true` when the text of a related test file holds a signal that could make
/// a guarded call throw or reject. Comments and string contents count too.
pub(crate) fn may_throw_or_reject(test_file_text: &str) -> bool {
    let lower = test_file_text.to_ascii_lowercase();
    THROW_SIGNALS.iter().any(|signal| lower.contains(signal))
}

/// `true` when the changed line is located exactly in the owner's source and
/// sits inside a `catch (...) { }` block or a `.catch(` handler callback body.
pub(crate) fn changed_line_in_catch_arm(
    owner_source: &str,
    start_line: usize,
    changed_line: usize,
    line_text: &str,
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
    let target: usize = lines[..index].iter().map(|line| line.len() + 1).sum();
    let masked = ts_mask_literals_and_comments(owner_source);
    if masked.len() != owner_source.len() {
        return false;
    }
    let mut stack: Vec<bool> = Vec::new();
    let mut header_start = 0usize;
    for (at, c) in masked.char_indices() {
        if at >= target {
            break;
        }
        match c {
            '{' => {
                stack.push(is_catch_header(&masked[header_start..at]));
                header_start = at + 1;
            }
            '}' => {
                stack.pop();
                header_start = at + 1;
            }
            ';' => header_start = at + 1,
            _ => {}
        }
    }
    stack.contains(&true)
}

/// The text before a `{` opens a `catch` clause or the body of a function
/// passed to `.catch(`.
fn is_catch_header(header: &str) -> bool {
    let header = header.trim();
    if let Some(rest) = header.strip_prefix("catch")
        && rest
            .chars()
            .next()
            .is_none_or(|c| c == '(' || c.is_whitespace())
    {
        return true;
    }
    let Some(at) = header.rfind(".catch(") else {
        return false;
    };
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
            return false;
        }
    }
    after.contains("=>") || after.contains("function")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "export async function f(read) {\n  try {\n    return await read();\n  } catch {\n    return 1;\n  }\n}\n";

    #[test]
    fn catch_clause_body_is_a_catch_arm() {
        assert!(changed_line_in_catch_arm(OWNER, 1, 5, "    return 1;"));
        assert!(changed_line_in_catch_arm(
            "function f() {\n  try {\n    a();\n  } catch (e) {\n    b();\n  }\n}\n",
            1,
            5,
            "    b();"
        ));
    }

    #[test]
    fn try_body_and_trailing_code_are_not_catch_arms() {
        assert!(!changed_line_in_catch_arm(
            OWNER,
            1,
            3,
            "    return await read();"
        ));
        let after =
            "function f() {\n  try {\n    a();\n  } catch {\n    b();\n  }\n  return 2;\n}\n";
        assert!(!changed_line_in_catch_arm(after, 1, 7, "  return 2;"));
    }

    #[test]
    fn dot_catch_handler_body_is_a_catch_arm() {
        let src = "function f(p) {\n  return p.catch((e) => {\n    return 1;\n  });\n}\n";
        assert!(changed_line_in_catch_arm(src, 1, 3, "    return 1;"));
        let then = "function f(p) {\n  return p.catch(h).then((v) => {\n    return 1;\n  });\n}\n";
        assert!(!changed_line_in_catch_arm(then, 1, 3, "    return 1;"));
    }

    #[test]
    fn a_misplaced_line_is_refused() {
        assert!(!changed_line_in_catch_arm(OWNER, 1, 4, "    return 1;"));
    }

    #[test]
    fn throw_signals_are_generous() {
        assert!(may_throw_or_reject("mockRejectedValue(1)"));
        assert!(may_throw_or_reject("() => { throw 1 }"));
        assert!(may_throw_or_reject("Promise.reject(1)"));
        assert!(!may_throw_or_reject("loadOrDefault(async () => 42, 0)"));
    }
}
