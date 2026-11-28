//! `PythonHiddenCode` answers what the per-question lexers answered before it
//! read each text once (#14195). The previous implementations are kept here,
//! unchanged, as the oracle.

use super::super::static_limits::single_line_fstring_is_ambiguous;
use super::super::{PrefixScan, python_prefix_hides_code};
use super::{
    PythonHiddenCode, fstring_field_scan, is_python_identifier_char, python_text_hides_code,
    single_line_fstring_offset_is_code, single_line_string_len, string_prefix_is_f_string,
};

/// `python_prefix_hides_code` as it was: one pass over the whole prefix.
fn reference_prefix_hides_code(prefix: &str) -> bool {
    /// One open string literal on the prefix scanner's stack.
    struct Literal {
        quote: char,
        triple: bool,
        formatted: bool,
        /// The scanner is inside this f-string's replacement field: its text
        /// is code, not string text.
        in_field: bool,
        /// Open braces of that field (a dict or set literal nests).
        field_depth: usize,
    }
    let chars: Vec<char> = prefix.chars().collect();
    let mut stack: Vec<Literal> = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let in_code = stack.last().is_none_or(|literal| literal.in_field);
        if in_code {
            if ch == '#' {
                return true;
            }
            if ch == '\'' || ch == '"' {
                let triple = chars.get(index + 1) == Some(&ch) && chars.get(index + 2) == Some(&ch);
                let formatted = reference_chars_end_with_f_prefix(&chars[..index]);
                // A single-line f-string whose quote pairing cannot be followed
                // (3.12 same-quote nesting in a field, a newline in a field)
                // hides what follows: a miss, never an invented call (#9230).
                if !triple
                    && formatted
                    && single_line_fstring_is_ambiguous(chars[index..].iter().copied())
                {
                    return true;
                }
                stack.push(Literal {
                    quote: ch,
                    triple,
                    formatted,
                    in_field: false,
                    field_depth: 0,
                });
                index += if triple { 3 } else { 1 };
                continue;
            }
            if let Some(literal) = stack.last_mut()
                && literal.in_field
            {
                if ch == '{' {
                    literal.field_depth += 1;
                } else if ch == '}' {
                    literal.field_depth = literal.field_depth.saturating_sub(1);
                    if literal.field_depth == 0 {
                        literal.in_field = false;
                    }
                }
            }
            index += 1;
            continue;
        }
        let Some((quote, triple, formatted)) = stack
            .last()
            .map(|literal| (literal.quote, literal.triple, literal.formatted))
        else {
            index += 1;
            continue;
        };
        if ch == '\\' {
            index += 2;
            continue;
        }
        if triple {
            if chars.len() - index >= 3 && chars[index..index + 3].iter().all(|c| *c == quote) {
                stack.pop();
                index += 3;
                continue;
            }
        } else if ch == quote {
            stack.pop();
            index += 1;
            continue;
        }
        if formatted {
            if ch == '{' {
                if chars.get(index + 1) == Some(&'{') {
                    index += 2;
                    continue;
                }
                if let Some(literal) = stack.last_mut() {
                    literal.in_field = true;
                    literal.field_depth = 1;
                }
                index += 1;
                continue;
            }
            if ch == '}' && chars.get(index + 1) == Some(&'}') {
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    stack.last().is_some_and(|literal| !literal.in_field)
}

/// Whether the identifier run that ends `before` is an `f` string prefix
/// (`f`, `F`, `rf`, `fR`, ...): at most two characters, and not the tail of a
/// longer name such as `perf`.
fn reference_chars_end_with_f_prefix(before: &[char]) -> bool {
    // Stop at the third identifier character: the answer is already `false`,
    // and the caller pays this per quote, so walking (and allocating) the whole
    // identifier run of a long line is unbounded work for a bounded answer.
    let mut run = 0usize;
    for ch in before.iter().rev() {
        if !is_python_identifier_char(*ch) {
            break;
        }
        run += 1;
        if run > 2 {
            return false;
        }
    }
    if run == 0 {
        return false;
    }
    let prefix: String = before[before.len() - run..].iter().collect();
    matches!(prefix.to_ascii_lowercase().as_str(), "f" | "rf" | "fr")
}

/// `inside_triple_quoted_string` as it was: lexes from byte 0 for every `idx`.
fn reference_inside_triple_quoted_string(text: &str, idx: usize) -> bool {
    // `(delimiter, is_f_string)` of the open triple-quoted string.
    let mut triple: Option<(&str, bool)> = None;
    let mut cursor = 0;
    while cursor < idx {
        let rest = &text[cursor..];
        if let Some((delimiter, formatted)) = triple {
            if let Some(escaped) = rest.strip_prefix('\\') {
                // In an f-string a backslash before a brace is a literal
                // character on its own; the brace then lexes normally.
                let brace = formatted && matches!(escaped.chars().next(), Some('{' | '}'));
                cursor += 1 + if brace {
                    0
                } else {
                    escaped.chars().next().map_or(0, char::len_utf8)
                };
            } else if rest.starts_with(delimiter) {
                triple = None;
                cursor += delimiter.len();
            } else if formatted && rest.starts_with("{{") {
                cursor += 2;
            } else if formatted && rest.starts_with('{') {
                // A field the scanner cannot prove closed (unterminated, a
                // comment, or the string's own delimiter nested inside it)
                // hides the rest of the text: a miss, never an invented call.
                let Some(field) = fstring_field_scan(rest, delimiter, idx - cursor, 0) else {
                    return true;
                };
                if field.target_hidden {
                    return true;
                }
                if idx < cursor + field.len {
                    return false;
                }
                cursor += field.len;
            } else {
                cursor += rest.chars().next().map_or(1, char::len_utf8);
            }
            continue;
        }
        let delimiter = ["\"\"\"", "'''"]
            .into_iter()
            .find(|delimiter| rest.starts_with(delimiter));
        if let Some(delimiter) = delimiter {
            triple = Some((delimiter, string_prefix_is_f_string(text, cursor)));
            cursor += 3;
        } else if rest.starts_with('#') || rest.starts_with('"') || rest.starts_with('\'') {
            // A single-line f-string whose quote pairing cannot be followed
            // (3.12 same-quote nesting in a field, a newline in a field)
            // hides the rest of the text: a miss, never an invented call
            // (#9230).
            let formatted = !rest.starts_with('#') && string_prefix_is_f_string(text, cursor);
            if formatted && single_line_fstring_is_ambiguous(rest.chars()) {
                return true;
            }
            let len = if rest.starts_with('#') {
                rest.find('\n').unwrap_or(rest.len())
            } else {
                single_line_string_len(rest)
            };
            if idx < cursor + len {
                // A replacement field of a single-line f-string is code, not
                // string text, so a call there stays visible (#14754 review).
                if !(formatted && single_line_fstring_offset_is_code(rest, idx - cursor)) {
                    return true;
                }
            }
            cursor += len;
        } else {
            cursor += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    triple.is_some()
}

/// `python_text_hides_code` as it was.
fn reference_text_hides_code(text: &str, idx: usize) -> bool {
    let prefix = text[..idx]
        .rsplit_once('\n')
        .map_or(&text[..idx], |(_, line)| line);
    reference_prefix_hides_code(prefix) || reference_inside_triple_quoted_string(text, idx)
}

fn boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect()
}

/// Every answer of the reference for `text`, by char boundary.
fn reference_answers(text: &str) -> Vec<bool> {
    boundaries(text)
        .into_iter()
        .map(|idx| reference_text_hides_code(text, idx))
        .collect()
}

/// Pieces that open, close, nest and break every construct the two lexers
/// read: comments, single and tripled quotes of both kinds, string prefixes
/// (and names that only end in one), replacement fields, escaped braces,
/// format specs, backslashes, line breaks and multi-byte characters.
const FRAGMENTS: &[&str] = &[
    "a",
    "perf",
    " ",
    "\n",
    "\r\n",
    "#",
    "\"",
    "'",
    "\"\"\"",
    "'''",
    "\"\"",
    "''",
    "f",
    "rf",
    "F",
    "b",
    "{",
    "}",
    "{{",
    "}}",
    ":",
    "\\",
    "\\\n",
    "\\\"",
    "\\{",
    "(",
    ")",
    "[",
    "]",
    "é",
    "→",
    "helper(1)",
    "f\"{a}\"",
    "f'{a[\"k\"]}'",
    "f\"{'x'}\"",
    "f\"{\"x\"}\"",
    "f\"\"\"{",
    "}\"\"\"",
    "f'''{a:>{w}}'''",
    "!r",
    "=",
];

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: 3000,
        ..proptest::test_runner::Config::default()
    })]

    #[test]
    fn resumed_scan_matches_the_per_question_lexers_at_every_offset(
        picks in proptest::collection::vec(0usize..FRAGMENTS.len(), 0..48)
    ) {
        let text: String = picks.iter().map(|pick| FRAGMENTS[*pick]).collect();
        let expected = reference_answers(&text);
        let offsets = boundaries(&text);

        // One scan asked about every offset in order: the shape of a
        // `match_indices` pass.
        let mut scan = PythonHiddenCode::new(&text);
        let resumed: Vec<bool> = offsets.iter().map(|idx| scan.hides(*idx)).collect();
        proptest::prop_assert_eq!(&resumed, &expected, "in-order scan of {:?}", text);

        // One fresh question per offset.
        let fresh: Vec<bool> = offsets
            .iter()
            .map(|idx| python_text_hides_code(&text, *idx))
            .collect();
        proptest::prop_assert_eq!(&fresh, &expected, "fresh questions of {:?}", text);
    }

    #[test]
    fn resumed_scan_matches_for_sparse_and_repeated_and_backward_offsets(
        picks in proptest::collection::vec(0usize..FRAGMENTS.len(), 0..48),
        asks in proptest::collection::vec(0usize..4096, 0..24),
        sorted in proptest::bool::ANY,
    ) {
        let text: String = picks.iter().map(|pick| FRAGMENTS[*pick]).collect();
        let offsets = boundaries(&text);
        let mut asks: Vec<usize> = asks.iter().map(|ask| offsets[ask % offsets.len()]).collect();
        if sorted {
            asks.sort_unstable();
        }
        let mut scan = PythonHiddenCode::new(&text);
        for idx in asks {
            proptest::prop_assert_eq!(
                scan.hides(idx),
                reference_text_hides_code(&text, idx),
                "offset {} of {:?}",
                idx,
                text
            );
        }
    }

    #[test]
    fn resumed_prefix_scan_matches_the_whole_prefix_lexer(
        picks in proptest::collection::vec(0usize..FRAGMENTS.len(), 0..48),
        skips in proptest::collection::vec(1usize..6, 0..48),
    ) {
        let line: String = picks
            .iter()
            .map(|pick| FRAGMENTS[*pick])
            .filter(|fragment| !fragment.contains('\n'))
            .collect();
        let offsets = boundaries(&line);
        // Growing prefixes with gaps of varying width between questions.
        let mut scan = PrefixScan::default();
        let mut at = 0;
        let mut skips = skips.into_iter().chain(std::iter::repeat(1));
        while at < offsets.len() {
            let prefix = &line[..offsets[at]];
            proptest::prop_assert_eq!(
                scan.hides(prefix),
                reference_prefix_hides_code(prefix),
                "prefix {:?} of {:?}",
                prefix,
                line
            );
            proptest::prop_assert_eq!(
                python_prefix_hides_code(prefix),
                reference_prefix_hides_code(prefix),
                "fresh prefix {:?}",
                prefix
            );
            at += skips.next().unwrap_or(1);
        }
    }
}

/// The differential cannot hold on one constant answer: fixed texts reach
/// hidden and visible offsets through each rule, in order, on one scan.
#[test]
fn fixed_texts_reach_hidden_and_visible_offsets_through_each_rule() {
    // (text, needle, expected answers for each occurrence of the needle)
    let cases: &[(&str, &str, &[bool])] = &[
        ("go(1)  # go(2)\ngo(3)", "go", &[false, true, false]),
        ("x = 'go(1)' + go(2)", "go", &[true, false]),
        ("'''it's go(1)'''; go(2)", "go", &[true, false]),
        ("\"\"\"\ngo(1)\n\"\"\"\ngo(2)", "go", &[true, false]),
        (
            "f\"\"\"{\n  go(1)\n} go(2) {{ go(3) }}\"\"\"\ngo(4)",
            "go",
            &[false, true, true, false],
        ),
        ("f\"{d[\"go\"]}\" + go(2)\ngo(3)", "go", &[true, true, true]),
        ("f'go(1) {go(2)}' + go(3)", "go", &[true, false, false]),
        ("perf'go(1)' go(2)", "go", &[true, false]),
        ("'a\\\ngo(1)' go(2)", "go", &[true, true]),
        ("\"\" go(1) \"\"\" go(2)", "go", &[false, true]),
    ];
    for (text, needle, expected) in cases {
        let mut scan = PythonHiddenCode::new(text);
        let resumed: Vec<bool> = text
            .match_indices(needle)
            .map(|(idx, _)| scan.hides(idx))
            .collect();
        let reference: Vec<bool> = text
            .match_indices(needle)
            .map(|(idx, _)| reference_text_hides_code(text, idx))
            .collect();
        assert_eq!(&reference, expected, "reference answers for {text:?}");
        assert_eq!(&resumed, expected, "resumed answers for {text:?}");
    }
}

/// The finding: one line with 20,000 `for` clauses. The per-question lexers
/// re-read the line for each of the 20,000 matches (about 8 s in a release
/// build for the whole check); one resumed scan reads it once. The deadline
/// leaves three orders of magnitude for a slow runner while still failing a
/// pass that is quadratic again.
#[test]
fn resumed_scan_stays_linear_on_a_long_line() -> Result<(), String> {
    let mut text = String::from("def price(a):\n    return [a");
    for _ in 0..20_000 {
        text.push_str(" for _ in [1]");
    }
    text.push_str("]  # for a\n");
    let started = std::time::Instant::now();
    let mut scan = PythonHiddenCode::new(&text);
    let visible = text
        .match_indices("for")
        .filter(|(idx, _)| !scan.hides(*idx))
        .count();
    let elapsed = started.elapsed();
    if visible != 20_000 {
        return Err(format!(
            "expected 20000 visible `for` tokens, found {visible}"
        ));
    }
    if elapsed > std::time::Duration::from_secs(10) {
        return Err(format!(
            "resumed scan took {elapsed:?} on a {} byte line",
            text.len()
        ));
    }
    Ok(())
}
