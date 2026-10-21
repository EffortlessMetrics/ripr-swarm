use super::mask_comments_and_strings;
use crate::analysis::facts::{CallFact, CallText};

/// Most distinct call names kept per source line. Every call fact carries
/// the full line text, so one line with N distinct callees holds N views of
/// the same line. The shared allocation keeps the in-memory cost linear, but
/// serialized facts inline the line per call, so the count is bounded.
/// Calls past the budget are dropped, which removes reach evidence (fewer
/// related-test links, never a stronger finding) rather than inventing any.
pub(crate) const MAX_CALL_FACTS_PER_LINE: usize = 256;

/// Whether `line` holds [`MAX_CALL_FACTS_PER_LINE`] facts, so the budget may
/// have dropped a call there. A proof that every call on the line is one
/// specific call must read such a line as unknown.
pub(crate) fn line_at_call_budget<'a>(
    calls: impl Iterator<Item = &'a CallFact>,
    line: usize,
) -> bool {
    calls.filter(|call| call.line == line).count() >= MAX_CALL_FACTS_PER_LINE
}

/// Whether any line of `calls` reached [`MAX_CALL_FACTS_PER_LINE`] facts, so
/// the budget may have dropped a call there. An absence proof ("no other call
/// to X") must read such a line as unknown.
pub(crate) fn has_saturated_line<'a>(calls: impl Iterator<Item = &'a CallFact>) -> bool {
    let mut per_line = std::collections::BTreeMap::<usize, usize>::new();
    for call in calls {
        let count = per_line.entry(call.line).or_insert(0);
        *count += 1;
        if *count >= MAX_CALL_FACTS_PER_LINE {
            return true;
        }
    }
    false
}

pub(crate) fn extract_call_facts(body: &str, start_line: usize) -> Vec<CallFact> {
    // Comments (line and block, nested) and string contents are erased
    // before scanning so commented-out calls never become evidence.
    // Masking preserves newlines and byte layout, so line attribution
    // stays exact; the fact text keeps the original source line.
    let masked = mask_comments_and_strings(body);
    let property_macros = super::property_macros::opaque_property_macros(&masked);
    // `#[derive(Error)]`, `#[cfg(test)]` and `#[should_panic(..)]` are
    // attributes, not calls: a parenthesis inside one names no function, so
    // it must not lend reach to a same-named function (#6924). Parameter
    // attributes whose arguments the test runs (`#[values(make(1))]`) stay
    // calls.
    let attributes: Vec<_> = attribute_ranges(&masked)
        .into_iter()
        .filter(|range| !runs_its_arguments(&masked[range.clone()]))
        .collect();
    let mut attribute_index = 0;
    let mut line_offset = 0;
    let mut property_index = 0;
    let mut line_property_start = 0;
    let mut line_property_end = 0;
    let mut calls = Vec::new();
    for (offset, (masked_line, original_line)) in masked
        .split_inclusive('\n')
        .zip(body.split_inclusive('\n'))
        .enumerate()
    {
        while property_macros
            .get(line_property_start)
            .is_some_and(|item| item.range.end <= line_offset)
        {
            line_property_start += 1;
        }
        line_property_end = line_property_end.max(line_property_start);
        while property_macros
            .get(line_property_end)
            .is_some_and(|item| item.range.start < line_offset + masked_line.len())
        {
            line_property_end += 1;
        }
        let line_properties = &property_macros[line_property_start..line_property_end];
        let scan_line = masked_line;
        // One shared allocation per line, built on the first call, instead
        // of one copy of the line per call (quadratic on a long line).
        let mut line_text: Option<CallText> = None;
        let mut line_names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        let bytes = scan_line.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            while property_macros
                .get(property_index)
                .is_some_and(|item| item.range.end <= line_offset + i)
            {
                property_index += 1;
            }
            while attributes
                .get(attribute_index)
                .is_some_and(|range| range.end <= line_offset + i)
            {
                attribute_index += 1;
            }
            if bytes[i] == b'('
                && !attributes
                    .get(attribute_index)
                    .is_some_and(|range| range.contains(&(line_offset + i)))
                && !property_macros
                    .get(property_index)
                    .is_some_and(|item| item.range.contains(&(line_offset + i)))
                && let Some((start, end)) = call_name_bounds_before_paren(scan_line, i)
            {
                let name = &scan_line[start..end];
                // Facts on one line differ only by name (the text is the
                // line), so a repeated name is the duplicate the final
                // dedup would drop; skip it before allocating anything.
                if is_call_name(name)
                    && line_names.len() < MAX_CALL_FACTS_PER_LINE
                    && line_names.insert(name)
                {
                    let text = line_text.get_or_insert_with(|| {
                        property_safe_call_text(original_line, line_offset, line_properties).into()
                    });
                    calls.push(CallFact {
                        line: start_line + offset,
                        name: name.to_string(),
                        text: text.clone(),
                    });
                }
            }
            i += 1;
        }
        line_offset += masked_line.len();
    }
    add_local_alias_calls(&masked, &mut calls);
    calls.sort_by(|a, b| a.line.cmp(&b.line).then(a.name.cmp(&b.name)));
    calls.dedup_by(|a, b| a.line == b.line && a.name == b.name);
    calls
}

/// Most `let` aliases considered per body; each costs a body scan, so the
/// count is bounded. Aliases past the cap are ignored (less reach evidence).
const MAX_LOCAL_ALIASES_PER_BODY: usize = 64;

/// A `let alias = path::to::function;` binding found in a masked body. The
/// path is matched by its last segment name only; it is never resolved to a
/// definition.
struct LocalFnAlias<'a> {
    alias: &'a str,
    target: &'a str,
    line_offset: usize,
}

/// Credit a call through a simple local function alias to the aliased
/// function (`let from_isoywd = NaiveDate::from_isoywd_opt;` then
/// `from_isoywd(..)` also reads as a call to `from_isoywd_opt`). Reach
/// evidence only: it adds a name-only call fact beside the alias-named one
/// and never strengthens an oracle or argument reading.
///
/// Fail-closed: the alias must be bound exactly once by an immutable
/// `let NAME = <path>;` (no type annotation, no `mut`, no closure or
/// call on the right) and every other mention of `NAME` in the body must be
/// a direct `NAME(` call after the binding. Any other mention — a second
/// `let`, a reassignment, a pattern binding, a closure parameter, a nested
/// `fn`, passing the alias along — leaves the alias unresolved and adds
/// nothing.
fn add_local_alias_calls(masked: &str, calls: &mut Vec<CallFact>) {
    let mut added = Vec::new();
    for alias in local_fn_aliases(masked) {
        if !alias_is_unambiguous(masked, &alias) {
            continue;
        }
        for call in calls.iter().filter(|call| call.name == alias.alias) {
            added.push(CallFact {
                line: call.line,
                name: alias.target.to_string(),
                text: call.text.clone(),
            });
        }
    }
    for fact in added {
        let on_line = calls.iter().filter(|call| call.line == fact.line).count();
        if on_line < MAX_CALL_FACTS_PER_LINE {
            calls.push(fact);
        }
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Every `let NAME = a::b::c;` in `masked` (immutable simple binding, no
/// type annotation, a plain path ending in a snake-case segment).
fn local_fn_aliases(masked: &str) -> Vec<LocalFnAlias<'_>> {
    let bytes = masked.as_bytes();
    let mut aliases = Vec::new();
    let mut search = 0;
    while aliases.len() < MAX_LOCAL_ALIASES_PER_BODY
        && let Some(found) = masked[search..].find("let")
    {
        let at = search + found;
        search = at + 3;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        let rest = &masked[at + 3..];
        let trimmed = rest.trim_start();
        if trimmed.len() == rest.len() {
            continue;
        }
        let name_len = trimmed
            .bytes()
            .take_while(|byte| is_ident_byte(*byte))
            .count();
        let name = &trimmed[..name_len];
        let after_name = trimmed[name_len..].trim_start();
        let Some(rhs) = after_name.strip_prefix('=') else {
            continue;
        };
        if rhs.starts_with('=') || name.is_empty() || name == "mut" || name == "ref" {
            continue;
        }
        if name.starts_with(|c: char| c.is_ascii_digit() || c.is_ascii_uppercase()) {
            continue;
        }
        let Some(end) = rhs.find(';') else {
            continue;
        };
        let path = rhs[..end].trim();
        let Some(target) = simple_fn_path_target(path) else {
            continue;
        };
        if target == name {
            continue;
        }
        aliases.push(LocalFnAlias {
            alias: name,
            target,
            line_offset: at,
        });
    }
    aliases
}

/// The last segment of `path` when it is `ident(::ident)*` with a
/// snake-case final segment; anything else (calls, generics, references,
/// closures, literals) is not a simple function path.
fn simple_fn_path_target(path: &str) -> Option<&str> {
    let mut last = "";
    for segment in path.split("::") {
        let segment = segment.trim();
        if segment.is_empty()
            || !segment.bytes().all(is_ident_byte)
            || segment.starts_with(|c: char| c.is_ascii_digit())
        {
            return None;
        }
        last = segment;
    }
    last.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        .then_some(last)
}

/// The alias is bound exactly once and every other mention is a direct call
/// after the binding.
///
/// Limitation: block scope is not modeled. The decision is body-wide, so a
/// same-named mention in any other block (a shadowing parameter, pattern or
/// loop binding, a macro, a field) disables the alias for the whole body
/// instead of being scoped out. That only drops reach evidence; it never
/// credits a call the alias does not make.
fn alias_is_unambiguous(masked: &str, alias: &LocalFnAlias<'_>) -> bool {
    let bytes = masked.as_bytes();
    let name = alias.alias;
    let mut search = 0;
    let mut definition_seen = false;
    while let Some(found) = masked[search..].find(name) {
        let at = search + found;
        let end = at + name.len();
        search = end;
        if (at > 0 && is_ident_byte(bytes[at - 1]))
            || bytes.get(end).is_some_and(|b| is_ident_byte(*b))
        {
            continue;
        }
        // The binding itself: `let NAME` at the recorded offset.
        if !definition_seen
            && at > alias.line_offset
            && masked[alias.line_offset + 3..at].trim().is_empty()
        {
            definition_seen = true;
            continue;
        }
        if !definition_seen {
            return false;
        }
        let before = masked[..at].trim_end();
        let after = masked[end..].trim_start();
        if before.ends_with('.') || before.ends_with("::") || !after.starts_with('(') {
            return false;
        }
        // `fn NAME(` declares a nested function that shadows the alias.
        if let Some(prefix) = before.strip_suffix("fn")
            && !prefix.bytes().next_back().is_some_and(is_ident_byte)
        {
            return false;
        }
    }
    definition_seen
}

/// Whether the attribute's arguments are expressions a test runs: rstest's
/// `#[values(..)]`, `#[with(..)]`, `#[case(..)]` and `#[future(..)]`,
/// `#[test_case(..)]`, and test-strategy's `#[strategy(..)]`, which can sit
/// inside a test's parameter list.
pub(crate) fn runs_its_arguments(attribute: &str) -> bool {
    let inner = attribute.trim_start_matches('#').trim_start();
    let inner = inner.strip_prefix('[').unwrap_or(inner).trim_start();
    let path_end = inner
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(inner.len());
    let name = inner[..path_end].rsplit("::").next().unwrap_or_default();
    matches!(
        name,
        "values" | "with" | "case" | "future" | "test_case" | "strategy"
    )
}

/// Byte ranges of every outer or inner attribute (`#[..]`, `#![..]`) in
/// `masked`, which has comments and string contents erased, so a `#` or
/// bracket inside either cannot open or close one. Sorted by start; nested
/// attributes fall inside their parent's range.
pub(crate) fn attribute_ranges(masked: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = masked.as_bytes();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'#' {
            i += 1;
            continue;
        }
        let mut open = i + 1;
        while open < bytes.len() && bytes[open].is_ascii_whitespace() {
            open += 1;
        }
        if bytes.get(open) == Some(&b'!') {
            open += 1;
            while open < bytes.len() && bytes[open].is_ascii_whitespace() {
                open += 1;
            }
        }
        if bytes.get(open) != Some(&b'[') {
            i += 1;
            continue;
        }
        let mut depth = 0usize;
        let mut end = None;
        for (offset, byte) in bytes[open..].iter().enumerate() {
            match byte {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        // An unclosed attribute is malformed source; claim nothing for it.
        let Some(end) = end else {
            break;
        };
        ranges.push(i..end);
        i = end;
    }
    ranges
}

/// CallFact already owns a source line. Exclude opaque arguments from that
/// same line so later argument readers cannot select a discarded earlier call.
/// This is a deny-only analysis view, never original-byte/source-commitment
/// authority. FunctionFact.body and FileFacts.source retain their exact bytes.
fn property_safe_call_text(
    line: &str,
    offset: usize,
    macros: &[super::property_macros::OpaquePropertyMacro<'_>],
) -> String {
    if macros.is_empty() {
        return line.trim().to_string();
    }
    let mut cursor = 0;
    let mut text = String::with_capacity(line.len());
    for item in macros {
        if item.range.end <= offset {
            continue;
        }
        if item.range.start >= offset + line.len() {
            break;
        }
        let start = item.range.start.saturating_sub(offset);
        let end = (item.range.end - offset).min(line.len());
        text.push_str(&line[cursor..start]);
        text.extend(std::iter::repeat_n(' ', end - start));
        cursor = end;
    }
    text.push_str(&line[cursor..]);
    // Match the original line's trimming, preserving all newly blanked byte
    // positions rather than trimming away an opaque prefix after masking.
    let leading = line.len() - line.trim_start().len();
    let end = line.trim_end().len();
    text.truncate(end);
    text.drain(..leading.min(end));
    text
}

pub(crate) fn call_name_bounds_before_paren(
    line: &str,
    paren_index: usize,
) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut end = paren_index;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    if bytes[end - 1] == b'>' {
        end = turbofish_start(line, end)?;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    (start < end).then_some((start, end))
}

fn turbofish_start(line: &str, end: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    let mut i = end;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'>' => depth += 1,
            b'<' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (i >= 2 && bytes[i - 1] == b':' && bytes[i - 2] == b':')
                        .then_some(i - 2);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_call_name(name: &str) -> bool {
    !matches!(
        name,
        "if" | "while"
            | "match"
            | "for"
            | "loop"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "assert_matches"
    )
}

#[cfg(test)]
fn call_names(calls: &[CallFact]) -> Vec<&str> {
    calls.iter().map(|call| call.name.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias_call_names(body: &str) -> Vec<(usize, String)> {
        extract_call_facts(body, 1)
            .into_iter()
            .map(|call| (call.line, call.name))
            .collect()
    }

    #[test]
    fn local_function_alias_call_also_reads_as_call_to_the_aliased_function() {
        // chrono 6adaa52: the test calls the changed `from_isoywd_opt`
        // only through a `let` alias.
        let body = "fn t() {\n    let from_isoywd = NaiveDate::from_isoywd_opt;\n    assert_eq!(from_isoywd(i32::MIN, 1, Weekday::Mon), None);\n}\n";
        let names = alias_call_names(body);
        assert!(names.contains(&(3, "from_isoywd".to_string())), "{names:?}");
        assert!(
            names.contains(&(3, "from_isoywd_opt".to_string())),
            "{names:?}"
        );
        // The binding line itself is not a call.
        assert!(!names.iter().any(|(line, _)| *line == 2), "{names:?}");
    }

    #[test]
    fn single_segment_alias_is_resolved() {
        let names = alias_call_names("fn t() {\n let f = parse_value;\n f(1);\n f(2);\n}\n");
        assert!(names.contains(&(3, "parse_value".to_string())), "{names:?}");
        assert!(names.contains(&(4, "parse_value".to_string())), "{names:?}");
    }

    #[test]
    fn ambiguous_or_non_function_aliases_are_not_resolved() {
        let cases = [
            // reassigned / shadowed by a second binding
            "fn t() {\n let f = real::target;\n f(1);\n let f = other;\n f(2);\n}\n",
            // mutable binding can be reassigned
            "fn t() {\n let mut f = real::target;\n f(1);\n}\n",
            // typed binding is not a simple alias
            "fn t() {\n let f: fn(u8) = real::target;\n f(1);\n}\n",
            // alias passed along or captured: not only direct calls
            "fn t() {\n let f = real::target;\n run(f);\n f(1);\n}\n",
            // closure parameter reuses the name
            "fn t() {\n let f = real::target;\n let g = |f: u8| f(1);\n f(2);\n}\n",
            // nested fn shadows the alias
            "fn t() {\n let f = real::target;\n fn f() {}\n f();\n}\n",
            // used before the binding
            "fn t() {\n f(1);\n let f = real::target;\n}\n",
            // right side is a call result, not a function path
            "fn t() {\n let f = real::target(1);\n f(2);\n}\n",
            // method path, not a plain function path
            "fn t() {\n let f = value.target;\n f(2);\n}\n",
            // type-like final segment (unit struct or variant), not a function
            "fn t() {\n let f = real::Target;\n f(2);\n}\n",
            // fn parameter reuses the name
            "fn t() {\n let f = real::target;\n fn inner(f: fn()) { f(); }\n f(2);\n}\n",
            // pattern binding reuses the name
            "fn t() {\n let f = real::target;\n if let Some(f) = x { f(1); }\n f(2);\n}\n",
            // loop binding reuses the name
            "fn t() {\n let f = real::target;\n for f in xs { f(1); }\n f(2);\n}\n",
            // macro mention of the name
            "fn t() {\n let f = real::target;\n f!(1);\n f(2);\n}\n",
            // struct field mention of the name
            "fn t() {\n let f = real::target;\n let s = S { f: 1 };\n f(2);\n}\n",
            // alias inside a string or comment
            "fn t() {\n // let f = real::target;\n f(2);\n}\n",
        ];
        for body in cases {
            // A `target` fact on a line that calls through `f` would be an
            // alias credit; `target(1)` on the binding line is a real call.
            let credited = extract_call_facts(body, 1).into_iter().any(|call| {
                matches!(call.name.as_str(), "target" | "Target" | "other")
                    && call
                        .text
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|w| w == "f")
                    && !call.text.starts_with("let ")
            });
            assert!(!credited, "{body}");
        }
    }

    #[test]
    fn lifetimes_and_labels_do_not_mask_call_and_literal_facts() {
        // #3633 review (devin LYzZ + coderabbit LtlJ): nearby lifetime and
        // label apostrophes used to pair with char-literal openings and
        // erase live calls and numbers between them. Both extractors keep
        // the evidence.
        let body = "fn f<'a>() { x(7); 'b'; }\nfn g() { 'lbl: loop { keep(2); } }\n";

        let calls = extract_call_facts(body, 1);
        let call_names: Vec<&str> = calls.iter().map(|call| call.name.as_str()).collect();
        assert!(
            call_names.contains(&"x") && call_names.contains(&"keep"),
            "{call_names:?}"
        );

        let literals = super::super::extract_literal_facts(body, 1);
        let literal_values: Vec<&str> = literals
            .iter()
            .map(|literal| literal.value.as_str())
            .collect();
        assert!(
            literal_values.contains(&"7") && literal_values.contains(&"2"),
            "{literal_values:?}"
        );
    }

    #[test]
    fn given_control_flow_and_assertion_like_calls_when_extracting_then_skips_non_function_names() {
        let calls = extract_call_facts(
            r#"if(condition) {}
while(condition) {}
assert_eq(actual, expected);
real_call(1);
"#,
            10,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].line, 13);
        assert_eq!(calls[0].name, "real_call");
        assert_eq!(calls[0].text, "real_call(1);");
    }

    #[test]
    fn call_extraction_ignores_comment_and_string_mentions() {
        let calls = extract_call_facts(
            r#"// fake_call()
let note = "device_labels(";
real_call(1);
"#,
            20,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].line, 22);
        assert_eq!(calls[0].name, "real_call");
        assert_eq!(calls[0].text, "real_call(1);");
    }

    #[test]
    fn attribute_arguments_are_not_calls_but_code_after_them_is() {
        let calls = extract_call_facts(
            r#"#[derive(Error, Debug)]
#[error("braced {msg}")]
struct E { msg: String }
#![cfg_attr(test, allow(dead_code))]
# [ should_panic(expected = "x") ]
let v = vec![derive_me(1)];
real_call(E { msg: build(2) });
"#,
            40,
        );

        assert_eq!(call_names(&calls), vec!["derive_me", "build", "real_call"]);
        assert_eq!(calls[0].line, 45);
    }

    // rstest parameter attributes run their arguments, so those stay calls,
    // while a sibling `#[allow(..)]` on the same parameter list does not.
    #[test]
    fn parameter_attributes_that_run_their_arguments_stay_calls() {
        let calls = extract_call_facts(
            "fn t(#[values(score(1), score(2))] x: u8, #[rstest::with(make())] y: u8, #[allow(unused)] z: u8) {\n",
            1,
        );

        // Same-line calls are deduplicated; `values`, `with` and `t` keep their
        // pre-#6924 reading.
        assert_eq!(
            call_names(&calls),
            vec!["make", "score", "t", "values", "with"]
        );
    }

    // Malformed source: the scan stops at an unclosed attribute, so it and
    // every later attribute in the body keep the plain call reading.
    #[test]
    fn an_unclosed_attribute_keeps_the_plain_call_reading() {
        let calls = extract_call_facts("before(1);\n#[derive(Error\n", 1);

        assert_eq!(call_names(&calls), vec!["before", "derive"]);
    }

    #[test]
    fn call_extraction_recognizes_turbofish_function_calls() {
        let calls = extract_call_facts(
            r#"
let rendered = render_pipeline::<String>("alpha");
let value = TypeName::new::<usize>();
let parsed = crate::parser::parse::<u64>("42");
let spaced = render_pipeline ("beta");
"#,
            30,
        );

        assert_eq!(
            call_names(&calls),
            vec!["render_pipeline", "new", "parse", "render_pipeline"]
        );
        assert_eq!(calls[0].line, 31);
        assert_eq!(
            calls[0].text,
            "let rendered = render_pipeline::<String>(\"alpha\");"
        );
        assert_eq!(calls[3].line, 34);
        assert_eq!(calls[3].text, "let spaced = render_pipeline (\"beta\");");
    }

    #[test]
    fn repeated_calls_on_one_long_line_collapse_and_share_the_line() {
        // A chain of N `.f()` calls on one line used to push N facts that
        // each copied the whole line (quadratic memory). Distinct names on
        // that line must read one allocation, not a private copy each.
        let n = 5_000;
        let body = format!("fn a(){{ x{};}}\n", ".f()".repeat(n));
        let calls = extract_call_facts(&body, 1);
        assert_eq!(call_names(&calls), vec!["a", "f"]);
        assert_eq!(calls[1].text.len(), body.trim().len());
        assert!(calls[0].text.shares_allocation(&calls[1].text));
    }

    #[test]
    fn the_per_line_budget_keeps_the_first_names_in_scan_order() {
        let total = MAX_CALL_FACTS_PER_LINE + 50;
        let names: Vec<String> = (0..total).map(|index| format!("g{index}")).collect();
        // `g0` repeats after the budget filled; `g300` is first seen past it
        // and repeats too.
        let body = format!(
            "fn a(){{ {}.g0().g300().g300();}}\n",
            names.join("().") + "()"
        );
        let calls = extract_call_facts(&body, 1);
        assert_eq!(calls.len(), MAX_CALL_FACTS_PER_LINE);
        let kept: std::collections::BTreeSet<&str> =
            calls.iter().map(|call| call.name.as_str()).collect();
        // `a` is scanned first, then g0..g254.
        assert!(kept.contains("a"));
        assert!(kept.contains("g0") && kept.contains("g254"));
        assert!(!kept.contains("g255") && !kept.contains("g300"));
        assert!(has_saturated_line(calls.iter()));
    }

    #[test]
    fn line_at_call_budget_reads_only_its_own_line() {
        let names: Vec<String> = (0..MAX_CALL_FACTS_PER_LINE + 5)
            .map(|index| format!("g{index}"))
            .collect();
        let body = format!(
            "fn a(){{ {};}}\nfn b(){{ x.f(); }}\n",
            names.join("().") + "()"
        );
        let calls = extract_call_facts(&body, 1);
        assert!(line_at_call_budget(calls.iter(), 1));
        assert!(!line_at_call_budget(calls.iter(), 2));
    }

    #[test]
    fn a_line_under_the_budget_is_not_saturated() {
        let calls = extract_call_facts("fn a(){ x.f().g(); }\n", 1);
        assert!(!has_saturated_line(calls.iter()));
    }

    #[test]
    fn calls_with_distinct_names_on_one_line_share_one_line_allocation() {
        let names: Vec<String> = (0..MAX_CALL_FACTS_PER_LINE + 50)
            .map(|index| format!("g{index}"))
            .collect();
        let body = format!("fn a(){{ {};}}\n", names.join("().") + "()");
        let calls = extract_call_facts(&body, 1);
        // The budget bounds the facts for the line, and every kept fact
        // reads the same allocation instead of a private copy.
        assert_eq!(calls.len(), MAX_CALL_FACTS_PER_LINE);
        assert!(
            calls
                .iter()
                .all(|call| call.text.shares_allocation(&calls[0].text))
        );
    }
}
