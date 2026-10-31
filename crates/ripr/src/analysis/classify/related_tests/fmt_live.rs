//! Live-code view of a test body for formatting dispatch (RIPR-SPEC-0297).
//!
//! A formatting call only reaches a changed `Display`/`Debug` `fmt` when the
//! test can run it. This module blanks the regions of a test body that a
//! syntactic reading shows can never run, so a `to_string()` there is not
//! credited as reach:
//!
//! - the block of a literal `if false { .. }` / `while false { .. }` (an
//!   `else` branch stays live);
//! - the statement under `#[cfg(any())]` or `#[cfg(false)]`;
//! - a nested `fn` whose name is never mentioned outside its own body;
//! - a `let name = |..| ..;` closure whose binding is never mentioned again
//!   (or is `_`), so it is never called;
//! - statements after a top-level `return ..;`.
//!
//! A condition that is merely unknown (`if cond { .. }`, a `const` flag, an
//! empty `for` range) is not here: it may run, so it stays reached. Every
//! shape that does not match exactly is left live, so the pass can only
//! remove credit from code that provably does not run.

use super::is_ident_byte;
use crate::analysis::extract::mask_comments_and_strings;

/// `body` with every provably dead region replaced by spaces (newlines kept),
/// so byte offsets and line structure are unchanged.
pub(super) fn live_body(body: &str) -> String {
    let masked = mask_comments_and_strings(body);
    if masked.len() != body.len() {
        return body.to_string();
    }
    let bytes = masked.as_bytes();
    let mut dead: Vec<(usize, usize)> = Vec::new();
    dead_conditions(bytes, &mut dead);
    dead_cfg_statements(bytes, &mut dead);
    let (scope, wrapper) = outer_scope(bytes);
    dead_nested_fns(bytes, wrapper, &mut dead);
    dead_closures(bytes, &mut dead);
    dead_after_return(bytes, scope, &mut dead);
    if dead.is_empty() {
        return body.to_string();
    }
    let mut out = body.as_bytes().to_vec();
    for (start, end) in dead {
        for byte in out.iter_mut().take(end).skip(start) {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| body.to_string())
}

fn whole_word_at(bytes: &[u8], at: usize, word: &str) -> bool {
    let end = at + word.len();
    bytes.get(at..end) == Some(word.as_bytes())
        && (at == 0 || !is_ident_byte(bytes[at - 1]))
        && bytes.get(end).is_none_or(|byte| !is_ident_byte(*byte))
}

fn skip_ws(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// The offset just past the group that closes the bracket at `open`.
fn close_of(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes.get(open..)?.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The offset just past the next `;` at bracket depth 0 from `from`. With
/// `braces` false a `{` at depth 0 ends the search unresolved.
fn statement_end(bytes: &[u8], from: usize, braces: bool) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes.get(from..)?.iter().enumerate() {
        match byte {
            b'{' if !braces && depth == 0 => return None,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b';' if depth == 0 => return Some(from + offset + 1),
            _ => {}
        }
    }
    None
}

fn dead_conditions(bytes: &[u8], dead: &mut Vec<(usize, usize)>) {
    for at in 0..bytes.len() {
        let keyword = if whole_word_at(bytes, at, "if") {
            "if"
        } else if whole_word_at(bytes, at, "while") {
            "while"
        } else {
            continue;
        };
        let cond = skip_ws(bytes, at + keyword.len());
        if !whole_word_at(bytes, cond, "false") {
            continue;
        }
        let open = skip_ws(bytes, cond + "false".len());
        if bytes.get(open) == Some(&b'{')
            && let Some(end) = close_of(bytes, open)
        {
            dead.push((open, end));
        }
    }
}

fn dead_cfg_statements(bytes: &[u8], dead: &mut Vec<(usize, usize)>) {
    let mut at = 0usize;
    while let Some(found) = find_from(bytes, at, b"#[") {
        at = found + 2;
        let Some(end) = close_of(bytes, found + 1) else {
            continue;
        };
        let attribute: String = bytes[found + 2..end - 1]
            .iter()
            .filter(|byte| !byte.is_ascii_whitespace())
            .map(|byte| *byte as char)
            .collect();
        if attribute != "cfg(any())" && attribute != "cfg(false)" {
            continue;
        }
        let item = skip_ws(bytes, end);
        let stop = if bytes.get(item) == Some(&b'{') {
            close_of(bytes, item)
        } else {
            statement_end(bytes, item, false)
        };
        if let Some(stop) = stop {
            dead.push((found, stop));
        }
    }
}

fn find_from(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

fn ident_at(bytes: &[u8], at: usize) -> &str {
    let end = bytes[at..]
        .iter()
        .position(|byte| !is_ident_byte(*byte))
        .map_or(bytes.len(), |offset| at + offset);
    std::str::from_utf8(&bytes[at..end]).unwrap_or("")
}

/// Whole-word occurrences of `name` in `bytes` as start offsets.
fn occurrences(bytes: &[u8], name: &str) -> Vec<usize> {
    (0..bytes.len())
        .filter(|&at| whole_word_at(bytes, at, name))
        .collect()
}

fn dead_nested_fns(bytes: &[u8], wrapper: Option<usize>, dead: &mut Vec<(usize, usize)>) {
    for at in 0..bytes.len() {
        if !whole_word_at(bytes, at, "fn") || wrapper == Some(at) || inside_impl_or_trait(bytes, at)
        {
            continue;
        }
        let name_at = skip_ws(bytes, at + 2);
        let name = ident_at(bytes, name_at);
        if name.is_empty() || name_at == at + 2 {
            continue;
        }
        let Some(open) = fn_body_open(bytes, name_at + name.len()) else {
            continue;
        };
        let Some(end) = close_of(bytes, open) else {
            continue;
        };
        if occurrences(bytes, name)
            .into_iter()
            .all(|found| found == name_at || (open < found && found < end))
        {
            dead.push((at, end));
        }
    }
}

/// Whether `at` sits inside the body of an item-level `impl` or `trait`
/// block. Those methods (`Drop::drop`, `Iterator::next`, `Deref::deref`,
/// `From::from`) are called implicitly, never by name, so an unmentioned name
/// proves nothing about them. A `-> impl Trait` in a fn header is not an item
/// `impl`: only a header that starts with the keyword counts.
fn inside_impl_or_trait(bytes: &[u8], at: usize) -> bool {
    let mut headers: Vec<usize> = Vec::new();
    let mut header_start = 0usize;
    for (index, byte) in bytes.iter().enumerate().take(at) {
        match byte {
            b'{' => {
                headers.push(header_start);
                header_start = index + 1;
            }
            b'}' => {
                headers.pop();
                header_start = index + 1;
            }
            b';' => header_start = index + 1,
            _ => {}
        }
    }
    // `headers` holds the start of the header of each still-open block; a
    // header ends at its own `{`.
    headers.iter().any(|&start| {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'{')
            .map_or(bytes.len(), |offset| start + offset);
        starts_with_item_keyword(&bytes[start..end])
    })
}

fn starts_with_item_keyword(header: &[u8]) -> bool {
    let mut at = skip_ws(header, 0);
    loop {
        if header.get(at..at + 2) == Some(b"#[") {
            match close_of(header, at + 1) {
                Some(end) => at = skip_ws(header, end),
                None => return false,
            }
            continue;
        }
        if whole_word_at(header, at, "pub") {
            at = skip_ws(header, at + 3);
            if header.get(at) == Some(&b'(') {
                match close_of(header, at) {
                    Some(end) => at = skip_ws(header, end),
                    None => return false,
                }
            }
            continue;
        }
        if whole_word_at(header, at, "unsafe") {
            at = skip_ws(header, at + 6);
            continue;
        }
        return whole_word_at(header, at, "impl") || whole_word_at(header, at, "trait");
    }
}

/// The `{` opening a fn body: the first `{` at depth 0 after the name, unless
/// a `;` (a bodyless declaration) comes first.
fn fn_body_open(bytes: &[u8], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes.get(from..)?.iter().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.checked_sub(1)?,
            b'{' if depth == 0 => return Some(from + offset),
            b';' if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

fn dead_closures(bytes: &[u8], dead: &mut Vec<(usize, usize)>) {
    for at in 0..bytes.len() {
        if !whole_word_at(bytes, at, "let") {
            continue;
        }
        let mut name_at = skip_ws(bytes, at + 3);
        if whole_word_at(bytes, name_at, "mut") {
            name_at = skip_ws(bytes, name_at + 3);
        }
        let name = ident_at(bytes, name_at);
        if name.is_empty() {
            continue;
        }
        let eq = skip_ws(bytes, name_at + name.len());
        if bytes.get(eq) != Some(&b'=') || bytes.get(eq + 1) == Some(&b'=') {
            continue;
        }
        let mut value = skip_ws(bytes, eq + 1);
        if whole_word_at(bytes, value, "move") {
            value = skip_ws(bytes, value + 4);
        }
        if bytes.get(value) != Some(&b'|') {
            continue;
        }
        let Some(end) = statement_end(bytes, value, true) else {
            continue;
        };
        if name == "_" || occurrences(bytes, name).len() == 1 {
            dead.push((at, end));
        }
    }
}

/// The statements of the test: the inside of a wrapping `{ .. }` or of the
/// test's own `fn name() { .. }` (with its attributes), else the whole text.
/// The second value is the wrapping `fn` keyword's offset, which is the test
/// itself and never a nested fn.
fn outer_scope(bytes: &[u8]) -> ((usize, usize), Option<usize>) {
    let whole = (0, bytes.len());
    let mut at = skip_ws(bytes, 0);
    while bytes.get(at..at + 2) == Some(b"#[") {
        let Some(end) = close_of(bytes, at + 1) else {
            return (whole, None);
        };
        at = skip_ws(bytes, end);
    }
    if bytes.get(at) == Some(&b'{') {
        return match close_of(bytes, at) {
            Some(end) if skip_ws(bytes, end) == bytes.len() => ((at + 1, end - 1), None),
            _ => (whole, None),
        };
    }
    while ["pub", "async", "unsafe"]
        .iter()
        .any(|word| whole_word_at(bytes, at, word))
    {
        at = skip_ws(bytes, at + ident_at(bytes, at).len());
    }
    if !whole_word_at(bytes, at, "fn") {
        return (whole, None);
    }
    let open = fn_body_open(bytes, at + 2);
    match open.and_then(|open| close_of(bytes, open).map(|end| (open, end))) {
        Some((open, end)) if skip_ws(bytes, end) == bytes.len() => ((open + 1, end - 1), Some(at)),
        _ => (whole, None),
    }
}

fn dead_after_return(bytes: &[u8], scope: (usize, usize), dead: &mut Vec<(usize, usize)>) {
    let (first, limit) = scope;
    let mut depth = 0usize;
    for at in first..limit {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth != 0 || !whole_word_at(bytes, at, "return") {
            continue;
        }
        let at_statement_start = bytes[..at]
            .iter()
            .rev()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_none_or(|byte| matches!(byte, b';' | b'{' | b'}'));
        if !at_statement_start {
            continue;
        }
        if let Some(end) = statement_end(bytes, at, true)
            && end <= limit
        {
            dead.push((end, limit));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::live_body;

    fn live(body: &str) -> String {
        live_body(body)
    }

    #[test]
    fn straight_line_code_is_untouched() {
        let body = "let m = Merge { n: 1 };\nassert_eq!(m.to_string(), \"x\");";
        assert_eq!(live(body), body);
    }

    #[test]
    fn conditional_and_loop_bodies_with_unknown_conditions_stay_live() {
        for body in [
            "if cond { m.to_string(); }",
            "const OFF: bool = false; if OFF { m.to_string(); }",
            "for _ in 0..0 { m.to_string(); }",
            "if false || cond { m.to_string(); }",
            "if cfg!(any()) { m.to_string(); }",
            "let f = || m.to_string(); f();",
            "fn go() -> String { m.to_string() } go();",
            "if c { return; } m.to_string();",
            "let f = || return 1; f(); m.to_string();",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
    }

    #[test]
    fn a_literal_false_block_is_dead_but_its_else_is_live() {
        let out = live("if false { a.to_string(); } else { b.to_string(); }");
        assert!(!out.contains("a.to_string"), "{out}");
        assert!(out.contains("b.to_string"), "{out}");
        assert!(!live("while false { m.to_string(); }").contains("to_string"));
        assert!(!live("if x { } else if false { m.to_string(); }").contains("to_string"));
    }

    #[test]
    fn disabled_cfg_statements_are_dead() {
        assert!(!live("#[cfg(any())]\n{ m.to_string(); }").contains("to_string"));
        assert!(!live("#[cfg(false)] m.to_string();").contains("to_string"));
        let kept = live("#[cfg(any())] a.to_string(); b.to_string();");
        assert!(
            !kept.contains("a.to_string") && kept.contains("b.to_string"),
            "{kept}"
        );
        assert!(live("#[cfg(unix)] m.to_string();").contains("to_string"));
    }

    #[test]
    fn an_unmentioned_nested_fn_or_closure_is_dead() {
        assert!(!live("a(); fn unused() -> String { m.to_string() }").contains("m.to_string"));
        assert!(!live("let _f = || m.to_string();").contains("to_string"));
        assert!(!live("let _ = move || { m.to_string() };").contains("to_string"));
        assert!(live("let f = || m.to_string(); let s = f();").contains("to_string"));
        assert!(live("fn used() -> String { m.to_string() } used();").contains("to_string"));
    }

    #[test]
    fn statements_after_a_top_level_return_are_dead() {
        let out = live("a.to_string(); return; b.to_string();");
        assert!(
            out.contains("a.to_string") && !out.contains("b.to_string"),
            "{out}"
        );
        let wrapped = live("{ a.to_string(); return; b.to_string(); }");
        assert!(
            wrapped.contains("a.to_string") && !wrapped.contains("b.to_string"),
            "{wrapped}"
        );
        assert_eq!(
            live("{ if c { return; } a.to_string(); }"),
            "{ if c { return; } a.to_string(); }"
        );
    }

    #[test]
    fn the_tests_own_fn_wrapper_is_not_a_nested_fn() {
        for body in [
            "#[test]\nfn straight() {\n    assert_eq!(m.to_string(), \"x\");\n}",
            "fn straight() { m.to_string(); }",
            "pub async fn straight() { m.to_string(); }",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        let out = live("#[test]\nfn t() {\n    a.to_string();\n    return;\n    b.to_string();\n}");
        assert!(
            out.contains("a.to_string") && !out.contains("b.to_string"),
            "{out}"
        );
        let nested =
            live("#[test]\nfn t() {\n    fn unused() { m.to_string(); }\n    a.to_string();\n}");
        assert!(
            !nested.contains("m.to_string") && nested.contains("a.to_string"),
            "{nested}"
        );
    }

    #[test]
    fn methods_of_a_local_impl_or_trait_are_implicitly_called_and_stay_live() {
        for body in [
            "a(); impl Drop for Guard { fn drop(&mut self) { m.to_string(); } }",
            "a(); unsafe impl Send for Guard { fn poke(&self) { m.to_string(); } }",
            "a(); impl<T> From<T> for Guard { fn from(t: T) -> Self { m.to_string(); Guard } }",
            "a(); trait Tr { fn go(&self) { m.to_string(); } }",
            "a(); pub(crate) trait Tr { fn go(&self) { m.to_string(); } }",
            "#[test]\nfn t() {\n    a();\n    impl Drop for Guard { fn drop(&mut self) { m.to_string(); } }\n}",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        // A free nested fn next to an impl is still judged, and an
        // `-> impl Trait` return type is not an item impl.
        let out =
            live("a(); impl Drop for G { fn drop(&mut self) {} } fn unused() { m.to_string(); }");
        assert!(!out.contains("m.to_string"), "{out}");
        let out = live("a(); fn unused() -> impl Fn() { fn inner() { m.to_string(); } || {} }");
        assert!(!out.contains("m.to_string"), "{out}");
    }

    #[test]
    fn length_and_lines_are_preserved() {
        let body = "a.to_string();\nif false {\n  é.to_string();\n}\nb.to_string();";
        let out = live(body);
        assert_eq!(out.len(), body.len());
        assert_eq!(out.matches('\n').count(), body.matches('\n').count());
    }
}
