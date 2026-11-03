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
    let scope = outer_scope(bytes);
    dead_nested_fns(bytes, body.as_bytes(), &scope, &mut dead);
    dead_closures(bytes, &mut dead);
    dead_after_return(bytes, scope.statements, &mut dead);
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

fn dead_nested_fns(bytes: &[u8], raw: &[u8], scope: &Scope, dead: &mut Vec<(usize, usize)>) {
    // A leading fn header this reader did not parse may hide the test's own
    // name: judge no nested fn rather than blank the test.
    if scope.unrecognised_header {
        return;
    }
    let containers = container_bodies(bytes);
    for at in 0..bytes.len() {
        if !whole_word_at(bytes, at, "fn")
            || scope.wrapper == Some(at)
            || containers
                .iter()
                .any(|&(start, end)| start < at && at < end)
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
        // Mentions count in the raw text too: a fn named only in a string
        // (`#[serde(default = "make_default")]`) or a comment may be called
        // by name through the attribute, so it is not provably dead.
        if occurrences(raw, name)
            .into_iter()
            .all(|found| found == name_at || (open < found && found < end))
        {
            dead.push((at, end));
        }
    }
}

fn prev_non_ws(bytes: &[u8], at: usize) -> Option<usize> {
    bytes[..at]
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
}

/// Whether the `impl`/`trait`/`mod` keyword at `at` starts an item rather
/// than naming a type (`-> impl Trait`, `x: impl Trait`): it follows a
/// statement or item boundary, an attribute or visibility group, or one of
/// the item modifiers.
fn at_item_position(bytes: &[u8], at: usize) -> bool {
    let Some(prev) = prev_non_ws(bytes, at) else {
        return true;
    };
    if matches!(bytes[prev], b';' | b'{' | b'}' | b']' | b')') {
        return true;
    }
    let start = bytes[..=prev]
        .iter()
        .rposition(|byte| !is_ident_byte(*byte))
        .map_or(0, |found| found + 1);
    matches!(
        &bytes[start..=prev],
        b"pub" | b"unsafe" | b"auto" | b"default"
    )
}

/// What ends the header of the item at `at`.
enum ItemBody {
    Open(usize),
    Semicolon(usize),
    Missing,
}

/// The `{` that opens the body of the `impl`/`trait`/`mod` at `at`. A `;`
/// inside brackets (`G<[u8; 4]>`) does not end the header. A brace that
/// follows `<`, `,` or `=` while an angle bracket is open is a const generic
/// argument (`Foo<{ 1 }>`), not the body; outside angle brackets a `,` is a
/// where-clause separator (`where T: Clone,\n{`) and the brace is the body.
fn item_body(bytes: &[u8], at: usize) -> ItemBody {
    let mut depth = 0usize;
    let mut angle = 0usize;
    let mut index = at;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'<' if depth == 0 => angle += 1,
            // `->` and `=>` are not closing angle brackets.
            b'>' if depth == 0 && !(index > 0 && matches!(bytes[index - 1], b'-' | b'=')) => {
                angle = angle.saturating_sub(1);
            }
            b';' if depth == 0 => return ItemBody::Semicolon(index),
            b'{' if depth == 0 => {
                let const_argument = angle > 0
                    && prev_non_ws(bytes, index)
                        .is_some_and(|prev| matches!(bytes[prev], b'<' | b',' | b'='));
                if !const_argument {
                    return ItemBody::Open(index);
                }
                match close_of(bytes, index) {
                    Some(end) => {
                        index = end;
                        continue;
                    }
                    None => return ItemBody::Missing,
                }
            }
            _ => {}
        }
        index += 1;
    }
    ItemBody::Missing
}

/// Body ranges `(keyword, end)` of item-level `impl`, `trait` and `mod`
/// blocks. Methods in `impl`/`trait` blocks (`Drop::drop`, `Iterator::next`,
/// `From::from`) are called implicitly and never named, and a `#[test]` in a
/// nested `mod` is run by the harness, so an unmentioned name proves nothing
/// about a fn there. A body that cannot be delimited extends to the end of the
/// text, so every doubt keeps the code live.
fn container_bodies(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for at in 0..bytes.len() {
        let keyword = ["impl", "trait", "mod"]
            .iter()
            .any(|word| whole_word_at(bytes, at, word));
        if !keyword || !at_item_position(bytes, at) {
            continue;
        }
        match item_body(bytes, at) {
            ItemBody::Open(open) => {
                ranges.push((at, close_of(bytes, open).unwrap_or(bytes.len())));
            }
            ItemBody::Missing => ranges.push((at, bytes.len())),
            ItemBody::Semicolon(_) => {}
        }
    }
    ranges
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

/// Where the test's statements are, and what its leading fn header looked like.
struct Scope {
    /// The inside of a wrapping `{ .. }` or of the test's own `fn name() { .. }`
    /// (with its attributes), else the whole text.
    statements: (usize, usize),
    /// The wrapping `fn` keyword's offset: the test itself, never a nested fn.
    wrapper: Option<usize>,
    /// The text starts like a fn header (modifiers or `fn`) that was not
    /// parsed as the test's own fn.
    unrecognised_header: bool,
}

fn outer_scope(bytes: &[u8]) -> Scope {
    let plain = Scope {
        statements: (0, bytes.len()),
        wrapper: None,
        unrecognised_header: false,
    };
    let mut at = skip_ws(bytes, 0);
    while bytes.get(at..at + 2) == Some(b"#[") {
        let Some(end) = close_of(bytes, at + 1) else {
            return plain;
        };
        at = skip_ws(bytes, end);
    }
    if bytes.get(at) == Some(&b'{') {
        return match close_of(bytes, at) {
            Some(end) if skip_ws(bytes, end) == bytes.len() => Scope {
                statements: (at + 1, end - 1),
                ..plain
            },
            _ => plain,
        };
    }
    let header_start = at;
    loop {
        if whole_word_at(bytes, at, "pub") {
            at = skip_ws(bytes, at + 3);
            if bytes.get(at) == Some(&b'(') {
                match close_of(bytes, at) {
                    Some(end) => at = skip_ws(bytes, end),
                    None => break,
                }
            }
        } else if ["const", "async", "unsafe"]
            .iter()
            .any(|word| whole_word_at(bytes, at, word))
        {
            at = skip_ws(bytes, at + ident_at(bytes, at).len());
        } else if whole_word_at(bytes, at, "extern") {
            at = skip_ws(bytes, at + 6);
            if bytes.get(at) == Some(&b'"') {
                match bytes[at + 1..].iter().position(|byte| *byte == b'"') {
                    Some(offset) => at = skip_ws(bytes, at + 1 + offset + 1),
                    None => break,
                }
            }
        } else {
            break;
        }
    }
    if !whole_word_at(bytes, at, "fn") {
        return Scope {
            unrecognised_header: at != header_start,
            ..plain
        };
    }
    let open = fn_body_open(bytes, at + 2);
    match open.and_then(|open| close_of(bytes, open).map(|end| (open, end))) {
        Some((open, end)) if skip_ws(bytes, end) == bytes.len() => Scope {
            statements: (open + 1, end - 1),
            wrapper: Some(at),
            ..plain
        },
        // The text starts with a fn but has more after it (a trailing item on
        // the closing line): which fn is the test is unknown.
        _ => Scope {
            unrecognised_header: true,
            ..plain
        },
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
            blank_statements(bytes, end, limit, dead);
            return;
        }
    }
}

/// The keyword of an item starting at `at` (`fn`, `impl`, `struct`, ..), after
/// attributes and modifiers. Items are hoisted: an item after a `return` is
/// still in scope for the code before it, so it is never dead.
fn item_keyword(bytes: &[u8], mut at: usize) -> Option<&'static str> {
    const KEYWORDS: [&str; 12] = [
        "fn",
        "impl",
        "trait",
        "struct",
        "enum",
        "union",
        "const",
        "static",
        "mod",
        "macro_rules",
        "use",
        "type",
    ];
    loop {
        if bytes.get(at..at + 2) == Some(b"#[") {
            at = skip_ws(bytes, close_of(bytes, at + 1)?);
        } else if whole_word_at(bytes, at, "pub") {
            at = skip_ws(bytes, at + 3);
            if bytes.get(at) == Some(&b'(') {
                at = skip_ws(bytes, close_of(bytes, at)?);
            }
        } else if ["unsafe", "async", "default"]
            .iter()
            .any(|word| whole_word_at(bytes, at, word))
        {
            at = skip_ws(bytes, at + ident_at(bytes, at).len());
        } else if whole_word_at(bytes, at, "extern") {
            at = skip_ws(bytes, at + 6);
            if bytes.get(at) == Some(&b'"') {
                let offset = bytes[at + 1..].iter().position(|byte| *byte == b'"')?;
                at = skip_ws(bytes, at + 1 + offset + 1);
            }
        } else if whole_word_at(bytes, at, "const") {
            let next = skip_ws(bytes, at + 5);
            if ["fn", "unsafe", "async", "extern"]
                .iter()
                .any(|word| whole_word_at(bytes, next, word))
            {
                at = next;
            } else {
                return Some("const");
            }
        } else {
            return KEYWORDS
                .iter()
                .copied()
                .find(|word| whole_word_at(bytes, at, word));
        }
    }
}

/// The offset just past the item that starts at `at`, or `limit`.
fn item_end(bytes: &[u8], at: usize, keyword: &str, limit: usize) -> usize {
    if matches!(keyword, "use" | "const" | "static" | "type") {
        return statement_end(bytes, at, true).map_or(limit, |end| end.min(limit));
    }
    // The same header scan as `item_body`, so a const generic brace
    // (`G<{ N }>`, `-> Foo<{ 3 }>`) is not taken for the body.
    match item_body(bytes, at) {
        ItemBody::Open(open) => close_of(bytes, open).map_or(limit, |end| end.min(limit)),
        ItemBody::Semicolon(index) => (index + 1).min(limit),
        ItemBody::Missing => limit,
    }
}

/// Marks `from..limit` dead except the items declared in it.
fn blank_statements(bytes: &[u8], from: usize, limit: usize, dead: &mut Vec<(usize, usize)>) {
    let mut cursor = from;
    let mut depth = 0usize;
    let mut at = from;
    while at < limit {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        let boundary = prev_non_ws(bytes, at)
            .is_none_or(|prev| prev < from || matches!(bytes[prev], b';' | b'{' | b'}' | b']'));
        if depth == 0
            && boundary
            && let Some(keyword) = item_keyword(bytes, at)
        {
            let end = item_end(bytes, at, keyword, limit).max(at + 1);
            if cursor < at {
                dead.push((cursor, at));
            }
            cursor = end;
            at = end;
            continue;
        }
        at += 1;
    }
    if cursor < limit {
        dead.push((cursor, limit));
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
    fn a_test_fn_with_any_modifier_is_the_wrapper_not_a_nested_fn() {
        for body in [
            "pub(crate) fn t() { m.to_string(); }",
            "pub(in crate::a) fn t() { m.to_string(); }",
            "const fn t() { m.to_string(); }",
            "extern \"C\" fn t() { m.to_string(); }",
            "#[test]\npub const unsafe extern \"C\" fn t() { m.to_string(); }",
            "#[test]\npub(crate) async fn t() { m.to_string(); }",
            // A leading header this reader cannot parse judges no nested fn.
            "const X: u32 = 1; fn unused() { m.to_string(); }",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        // The same wrapper still has its own dead code judged.
        let out = live("pub(crate) fn t() { a.to_string(); return; b.to_string(); }");
        assert!(
            out.contains("a.to_string") && !out.contains("b.to_string"),
            "{out}"
        );
    }

    #[test]
    fn impl_headers_with_brackets_or_const_braces_still_shield_their_methods() {
        for body in [
            "a(); impl Drop for G<[u8; 4]> { fn drop(&mut self) { m.to_string(); } }",
            "a(); impl Iterator for S where X: Y<[u8; 2]> { fn next(&mut self) -> Option<u8> { m.to_string(); None } }",
            "a(); impl Foo<{ 1 }> for G { fn go(&self) { m.to_string(); } }",
            "a(); impl<const N: usize> Foo<{ N }> for G { fn go(&self) { m.to_string(); } }",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        let out =
            live("a(); impl Foo<{ 1 }> for G { fn go(&self) {} } fn unused() { m.to_string(); }");
        assert!(!out.contains("m.to_string"), "{out}");
    }

    #[test]
    fn items_after_a_return_are_hoisted_and_stay_live() {
        for body in [
            "let s = helper(); return; fn helper() -> String { m.to_string() }",
            "a(); return; impl Drop for G { fn drop(&mut self) { m.to_string(); } }",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        let out = live("helper(); return; b.to_string(); fn helper() { c.to_string(); }");
        assert!(
            !out.contains("b.to_string") && out.contains("c.to_string"),
            "{out}"
        );
        let out = live("a(); return; #[derive(Debug)] struct S; b.to_string();");
        assert!(
            out.contains("struct S") && !out.contains("b.to_string"),
            "{out}"
        );
    }

    #[test]
    fn hoisted_items_with_const_generic_braces_after_a_return_stay_live() {
        for body in [
            "a(); return; impl Drop for G<{ N }> { fn drop(&mut self) { m.to_string(); } }",
            "let s = h(); return; fn h() -> Foo<{ 3 }> { m.to_string() }",
            "a(); return; impl<const N: usize> Tr<{ N }, [u8; 2]> for G { fn go(&self) { m.to_string(); } }",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        let out = live("a(); return; b.to_string(); impl Foo<{ 1 }> for G {}");
        assert!(
            out.contains("impl Foo") && !out.contains("b.to_string"),
            "{out}"
        );
    }

    #[test]
    fn rustfmt_where_clauses_with_trailing_commas_do_not_hide_the_body() {
        for body in [
            "a(); impl<T> Drop for G<T>\nwhere\n    T: Clone,\n{\n    fn drop(&mut self) { m.to_string(); }\n}",
            "a(); trait Tr<T>\nwhere\n    T: Clone,\n{\n    fn go(&self) { m.to_string(); }\n}",
            "a(); impl<F> Run for G<F>\nwhere\n    F: Fn() -> u8,\n{\n    fn run(&self) { m.to_string(); }\n}",
            "a(); return; impl<T> Drop for G<T>\nwhere\n    T: Clone,\n{\n    fn drop(&mut self) { m.to_string(); }\n}",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        let out = live(
            "a(); impl<T> Drop for G<T>\nwhere\n    T: Clone,\n{\n    fn drop(&mut self) {}\n}\nfn unused() { m.to_string(); }",
        );
        assert!(!out.contains("m.to_string"), "{out}");
    }

    #[test]
    fn a_nested_fn_named_only_in_a_string_or_comment_stays_live() {
        for body in [
            "#[serde(default = \"make_default\")] struct S; fn make_default() { m.to_string(); }",
            "a(); // keep make_default\nfn make_default() { m.to_string(); }",
        ] {
            assert_eq!(live(body), body, "{body}");
        }
        let out = live("a(); fn unmentioned() { m.to_string(); }");
        assert!(!out.contains("m.to_string"), "{out}");
    }

    #[test]
    fn trailing_items_after_the_test_fn_judge_no_nested_fn() {
        let body = "#[test] fn t() { m.to_string(); } fn other() { c.to_string(); }";
        assert_eq!(live(body), body);
    }

    #[test]
    fn a_nested_mod_is_not_judged() {
        let body = "a(); mod tests { #[test] fn t() { m.to_string(); } }";
        assert_eq!(live(body), body);
    }

    #[test]
    fn length_and_lines_are_preserved() {
        let body = "a.to_string();\nif false {\n  é.to_string();\n}\nb.to_string();";
        let out = live(body);
        assert_eq!(out.len(), body.len());
        assert_eq!(out.matches('\n').count(), body.matches('\n').count());
    }
}
