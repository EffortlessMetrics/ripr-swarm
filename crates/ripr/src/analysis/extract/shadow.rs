//! Shared test-body shadow detection for seam-callee identity.
//!
//! Both consumers — `analysis::classify::related_tests` (the
//! `SeamCalleeCall` test relation) and the guarded Result match scanner
//! (`analysis::extract::oracles::scan`) — must agree on when a same-named
//! local definition or binding in a test body impersonates a callee, so
//! the helpers live here as one authority (#3714, #3709). Callers pass a
//! comment-and-string-masked body (`mask_comments_and_strings`): masking
//! preserves byte layout, so body-relative line math stays exact against
//! the original text, and shadow-shaped text inside comments, strings, or
//! char literals never defeats a real call (#3728 rounds 3-5).

/// #3714 round-2 review (devin hIL0i): the body-relative line index of the
/// FIRST `let` binding whose binding pattern names `callee` (bounded) — this
/// covers `let mut`, `let ref`, typed bindings, and destructuring patterns
/// (round-2 review, devin hDROE).
///
/// The caller passes a comment-and-string-masked body, so shadow-shaped text
/// inside comments or string literals never defeats a real call (#3728
/// round-3 review, coderabbit + devin).
///
/// A `let` binding only shadows uses at or after its own line, so the caller
/// defeats a captured call only when this shadow precedes it. `CallFact`
/// carries no column, so a call on the shadow's own line — e.g. a binding
/// initializer `let x = x(..)`, which resolves in the preceding scope — is
/// indistinguishable from a post-binding call on the same line and is
/// conservatively defeated (#3728 review; under-credit only,
/// column-precise attribution rides the same #3727 follow-up).
/// Nested-block bindings still defeat following calls in this
/// whole-body approximation (under-credit only: relations may be dropped,
/// never fabricated); full lexical scopes are the parser-backed follow-up
/// tracked on #3727.
pub(crate) fn test_body_let_shadow_line(body: &str, callee: &str) -> Option<usize> {
    if callee.is_empty() {
        return None;
    }
    let mut search = 0usize;
    while let Some(offset) = body[search..].find("let ") {
        let start = search + offset;
        search = start + 4;
        let before_ok = start == 0
            || !(body.as_bytes()[start - 1].is_ascii_alphanumeric()
                || body.as_bytes()[start - 1] == b'_');
        if !before_ok {
            continue;
        }
        // The binding pattern runs from the `let` to the initializer's
        // `=` (depth zero): `let mut x = ..`,
        // `let ref x = ..`, `let x: T = ..`, and
        // `let (a, x) = ..` are all covered by the pattern region. A
        // depth-zero `;` first means this declaration has no initializer
        // (`let flag;`): the scan must stop there rather than borrow a
        // LATER binding's `=`, which would move the reported shadow line
        // earlier and defeat calls between the two declarations
        // (#3728 round-5 review, devin).
        let region = &body[start + "let ".len()..];
        let bytes = region.as_bytes();
        let mut depth = 0isize;
        let mut in_string = false;
        let mut escaped = false;
        let mut pattern_end = None;
        for (offset, byte) in bytes.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if *byte == b'\\' {
                    escaped = true;
                } else if *byte == b'"' {
                    in_string = false;
                }
                continue;
            }
            match byte {
                b'"' => in_string = true,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b';' if depth == 0 => break,
                b'=' if depth == 0 => {
                    pattern_end = Some(offset);
                    break;
                }
                _ => {}
            }
        }
        let Some(pattern_end) = pattern_end else {
            continue;
        };
        // Whole-word containment only: a binding whose name merely BEGINS
        // with the callee (`let try_parse_summary_result = ..`) is a
        // different binding, not a shadow (#3714 round-2 review,
        // coderabbit).
        let pattern = region[..pattern_end].trim();
        if pattern_contains_word(pattern, callee) {
            let body_line = body[..start].bytes().filter(|byte| *byte == b'\n').count();
            return Some(body_line);
        }
    }
    None
}

/// #3714 round-1 review (devin hC): a same-named local definition in the
/// test body impersonates the seam callee — the captured `CallFact` name
/// alone cannot distinguish a real seam-callee call from a call of a
/// same-named helper defined in the test itself.
///
/// #3714 round-2 review (devin hIL0i): a `fn <callee>` item is hoisted and
/// in scope for the WHOLE test body, so one same-named local fn defeats
/// every captured call (unlike `let` bindings, which are positional — see
/// [`test_body_let_shadow_line`]).
///
/// The caller passes a comment-and-string-masked body, so a mentioned
/// `fn <callee>` shape inside a comment or string never defeats a real
/// call (#3728 round-3 review, coderabbit + devin).
///
/// Residual (documented): same-named definitions elsewhere in the test's
/// package and qualified paths remain indistinguishable at name level; the
/// `SeamCalleeCall` relation carries no variant claim, so the defeat gap can
/// only over-relate (weakly), never over-credit variant identity.
pub(crate) fn test_body_defines_callee_fn(body: &str, callee: &str) -> bool {
    if callee.is_empty() {
        return false;
    }
    let mut search = 0usize;
    while let Some(offset) = body[search..].find("fn ") {
        let start = search + offset;
        let before_ok = start == 0
            || !(body.as_bytes()[start - 1].is_ascii_alphanumeric()
                || body.as_bytes()[start - 1] == b'_');
        if before_ok {
            let name_start = start + "fn ".len();
            let name = body[name_start..].trim_start();
            if let Some(after_name) = name.strip_prefix(callee)
                && after_name
                    .chars()
                    .next()
                    .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            {
                return true;
            }
        }
        search = start + 3;
    }
    false
}

/// Combined defeat at a use site: a hoisted `fn <callee>` defeats every
/// line; a `let` binding defeats at or after its own line. `body_line` is
/// the body-relative line of the use (call site or match scrutinee).
/// Callers pass a comment-and-string-masked body.
pub(crate) fn body_shadows_callee_at_line(body: &str, callee: &str, body_line: usize) -> bool {
    test_body_defines_callee_fn(body, callee)
        || test_body_let_shadow_line(body, callee).is_some_and(|shadow| shadow <= body_line)
}

/// Whole-word containment: `word` delimited by non-identifier characters on
/// both sides.
fn pattern_contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let mut search = 0usize;
    while let Some(offset) = text[search..].find(word) {
        let start = search + offset;
        let end = start + word.len();
        let before_ok =
            start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let after_ok =
            end >= bytes.len() || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');
        if before_ok && after_ok {
            return true;
        }
        search = end;
    }
    false
}
