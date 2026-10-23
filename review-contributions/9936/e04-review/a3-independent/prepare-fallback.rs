#[path="literal_mask.rs"] mod literal_mask;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::{SourceType,GetSpan};
pub(crate) fn ts_mask_literals_and_comments(text: &str) -> String {
    let prepared = literal_mask::prepare(text, None);
    ts_mask_lexically(
        &prepared.code,
        !prepared.regex_known,
        &prepared.operand_ends,
    )
}

/// Preserve the parser's syntax kind: identical bytes can mean JSX code in
/// `.tsx` and a type assertion followed by a regex literal in `.ts`.
pub(crate) fn ts_mask_literals_and_comments_for_source_type(
    text: &str,
    source_type: SourceType,
) -> String {
    let prepared = literal_mask::prepare(text, Some(source_type));
    ts_mask_lexically(
        &prepared.code,
        !prepared.regex_known,
        &prepared.operand_ends,
    )
}

/// [`ts_mask_literals_and_comments`] with every `/` read as division, so a
/// regex-versus-division misjudgement cannot hide code from a scan that also
/// runs on this mask. A real regex literal holding a quote can misread the rest
/// of its line here; callers require agreement of both masks.
pub(crate) fn ts_mask_literals_and_comments_division_only(text: &str) -> String {
    ts_mask_lexically(text, false, &std::collections::BTreeSet::new())
}

fn ts_mask_lexically(
    text: &str,
    regex_allowed: bool,
    operand_ends: &std::collections::BTreeSet<usize>,
) -> String {
    fn next_char(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
        offset: &mut usize,
    ) -> Option<char> {
        let c = chars.next()?;
        *offset += c.len_utf8();
        Some(c)
    }
    #[derive(Clone, Copy)]
    enum Mode {
        Code,
        Quote(char),
        Template,
        Regex { in_class: bool },
        RegexFlags,
        LineComment,
        BlockComment,
    }
    // Keywords after which an operand (so a regex) follows; a property
    // name with the same spelling (`x.return`) is not a keyword.
    const OPERAND_KEYWORDS: [&str; 12] = [
        "return",
        "typeof",
        "void",
        "delete",
        "in",
        "instanceof",
        "new",
        "throw",
        "case",
        "yield",
        "await",
        "of",
    ];
    // Keywords that take a string operand (`x as 'p'`, `from 'p'`) though a
    // `/` after them is not a regex; a quote after any other word is JSX text.
    const STRING_OPERAND_KEYWORDS: [&str; 14] = [
        "as",
        "is",
        "module",
        "static",
        "get",
        "set",
        "satisfies",
        "extends",
        "from",
        "import",
        "export",
        "default",
        "else",
        "do",
    ];
    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '$'
    }
    fn operand_expected(prev: Option<char>, word: &str, word_after_dot: bool) -> bool {
        match prev {
            None => true,
            Some(c) if is_word_char(c) => !word_after_dot && OPERAND_KEYWORDS.contains(&word),
            Some(c) => "(,=:[!&|?{};+-*%<>~^".contains(c),
        }
    }
    fn string_expected(prev: Option<char>, word: &str, word_after_dot: bool, spread: bool) -> bool {
        operand_expected(prev, word, word_after_dot)
            || spread
            || (prev.is_some_and(is_word_char)
                && !word_after_dot
                && STRING_OPERAND_KEYWORDS.contains(&word))
    }
    let mut out = String::with_capacity(text.len());
    let mut mode = Mode::Code;
    // The last non-whitespace code character, to tell a regex from division.
    let mut prev: Option<char> = None;
    // The identifier or keyword that `prev` ends, and whether a `.` precedes
    // it (a member name).
    let mut word = String::new();
    let mut word_after_dot = false;
    let mut after_space = false;
    // Consecutive `.` ending at `prev`: three is a spread (`[...'p']`), while
    // a sentence's `.` in JSX text (`Hi. "Bye {p(1)}`) is not.
    let mut dots = 0usize;
    // `++`/`--` that directly follows an operand is postfix, so a `/` after
    // it is division (`i++ / 2`), never a regex start.
    let mut postfix_op = false;
    let mut op_after_operand = false;
    // One entry per open `${`: the brace depth inside that interpolation.
    let mut interpolations: Vec<usize> = Vec::new();
    let mut chars = text.chars().peekable();
    let mut source_offset = 0usize;
    while let Some(c) = next_char(&mut chars, &mut source_offset) {
        let at = source_offset - c.len_utf8();
        if let Mode::RegexFlags = mode {
            if c.is_ascii_alphabetic() {
                out.push(' ');
                continue;
            }
            mode = Mode::Code;
        }
        match mode {
            Mode::Code => {
                match c {
                    // A string literal starts only where an operand is
                    // expected; after a word or `)` a quote is JSX text
                    // (`<b>Don't {p(1)}</b>`) and stays code.
                    '\'' | '"' if string_expected(prev, &word, word_after_dot, dots >= 3) => {
                        mode = Mode::Quote(c);
                        out.push(c);
                    }
                    '`' => {
                        mode = Mode::Template;
                        out.push(c);
                    }
                    '/' if chars.peek() == Some(&'/') => {
                        next_char(&mut chars, &mut source_offset);
                        mode = Mode::LineComment;
                        out.push_str("  ");
                    }
                    '/' if chars.peek() == Some(&'*') => {
                        next_char(&mut chars, &mut source_offset);
                        mode = Mode::BlockComment;
                        out.push_str("  ");
                    }
                    '/' if regex_allowed
                        && !postfix_op
                        && operand_expected(prev, &word, word_after_dot) =>
                    {
                        mode = Mode::Regex { in_class: false };
                        out.push(c);
                    }
                    '{' => {
                        if let Some(depth) = interpolations.last_mut() {
                            *depth += 1;
                        }
                        out.push(c);
                    }
                    '}' => {
                        match interpolations.last_mut() {
                            Some(0) => {
                                interpolations.pop();
                                mode = Mode::Template;
                            }
                            Some(depth) => *depth -= 1,
                            None => {}
                        }
                        out.push(c);
                    }
                    _ => out.push(c),
                }
                // A comment is whitespace: it keeps the lexical context
                // before it, so `f(/* c */ 'p')` still opens a string.
                if matches!(mode, Mode::LineComment | Mode::BlockComment) {
                    after_space = true;
                    continue;
                }
                if (c == '+' || c == '-') && !after_space && prev == Some(c) {
                    postfix_op = op_after_operand && !postfix_op;
                } else {
                    if c == '+' || c == '-' {
                        op_after_operand = !operand_expected(prev, &word, word_after_dot);
                    }
                    if !c.is_whitespace() {
                        postfix_op = false;
                    }
                }
                if is_word_char(c) {
                    // Whitespace ends a word: `x as` is two words.
                    if !prev.is_some_and(is_word_char) || after_space {
                        word.clear();
                        word_after_dot = prev == Some('.');
                    }
                    word.push(c);
                } else if !c.is_whitespace() {
                    word.clear();
                }
                if c == '.' {
                    dots = if prev == Some('.') && !after_space {
                        dots + 1
                    } else {
                        1
                    };
                } else if !c.is_whitespace() {
                    dots = 0;
                }
                after_space = c.is_whitespace();
                if !after_space {
                    prev = Some(c);
                }
            }
            Mode::Quote(quote) => {
                if c == '\\' {
                    next_char(&mut chars, &mut source_offset);
                    out.push_str("  ");
                } else if c == quote || c == '\n' {
                    mode = Mode::Code;
                    prev = Some(c);
                    out.push(c);
                } else {
                    out.push(' ');
                }
            }
            Mode::Template => {
                if c == '\\' {
                    next_char(&mut chars, &mut source_offset);
                    out.push_str("  ");
                } else if c == '`' {
                    mode = Mode::Code;
                    prev = Some(c);
                    out.push(c);
                } else if c == '$' && chars.peek() == Some(&'{') {
                    next_char(&mut chars, &mut source_offset);
                    interpolations.push(0);
                    mode = Mode::Code;
                    prev = Some('{');
                    out.push_str("${");
                } else {
                    out.push(' ');
                }
            }
            Mode::Regex { in_class } => {
                if c == '\\' {
                    next_char(&mut chars, &mut source_offset);
                    out.push_str("  ");
                } else if c == '\n' {
                    mode = Mode::Code;
                    out.push(c);
                } else if c == '/' && !in_class {
                    mode = Mode::RegexFlags;
                    prev = Some(c);
                    out.push(c);
                } else {
                    if c == '[' {
                        mode = Mode::Regex { in_class: true };
                    } else if c == ']' {
                        mode = Mode::Regex { in_class: false };
                    }
                    out.push(' ');
                }
            }
            // Handled before the match: flags are masked, then code resumes.
            Mode::RegexFlags => out.push(c),
            Mode::LineComment => {
                // ECMAScript line comments end at all four LineTerminators.
                if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                    mode = Mode::Code;
                    out.push(c);
                } else {
                    out.push(' ');
                }
            }
            Mode::BlockComment => {
                if c == '*' && chars.peek() == Some(&'/') {
                    next_char(&mut chars, &mut source_offset);
                    mode = Mode::Code;
                    out.push_str("  ");
                } else {
                    out.push(' ');
                }
            }
        }
        if operand_ends.contains(&at) {
            mode = Mode::Code;
            prev = Some(')');
            word.clear();
            word_after_dot = false;
            after_space = false;
            dots = 0;
            postfix_op = false;
            op_after_operand = false;
        }
    }
    out
}

fn assigns_next(rest: &str) -> bool {
    let rest = rest.trim_start();
    let operator_end = rest
        .find(|ch: char| {
            !matches!(
                ch,
                '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '?' | '<' | '>'
            )
        })
        .unwrap_or(rest.len());
    let after = &rest[operator_end..];
    after.starts_with('=')
        && !after[1..].starts_with(['=', '>'])
        && !matches!(&rest[..operator_end], "<" | ">" | "!")
}

fn global_object_member(before: &str) -> bool {
    let receiver = before[..before.len() - 1].trim_end();
    ["globalThis", "this", "window", "global", "self"]
        .iter()
        .any(|name| {
            receiver.strip_suffix(name).is_some_and(|head| {
                !head.ends_with(|ch: char| {
                    ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.'
                })
            })
        })
}

fn wrapped_write(before: &str, rest: &str) -> bool {
    if before.ends_with('(') {
        let lead = before.trim_end_matches(|ch: char| ch == '(' || ch.is_whitespace());
        if lead.ends_with("++") || lead.ends_with("--") {
            return true;
        }
    }
    let mut rest = rest;
    let mut wrapped = false;
    loop {
        let trimmed = rest.trim_start();
        if let Some(after) = trimmed.strip_prefix(')') {
            rest = after;
        } else if trimmed.starts_with('!') && !trimmed.starts_with("!=") {
            rest = &trimmed[1..];
        } else if ["as", "satisfies"].iter().any(|keyword| {
            trimmed
                .strip_prefix(keyword)
                .is_some_and(|tail| tail.starts_with(|ch: char| ch.is_whitespace() || ch == '('))
        }) {
            let mut depth = 0usize;
            let mut close = None;
            for (offset, ch) in trimmed.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' if depth == 0 => {
                        close = Some(offset);
                        break;
                    }
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            match close {
                Some(close) => rest = &trimmed[close..],
                None => return false,
            }
        } else {
            break;
        }
        wrapped = true;
    }
    if !wrapped {
        return false;
    }
    let rest = rest.trim_start();
    if rest.starts_with("++") || rest.starts_with("--") {
        return true;
    }
    let operator_end = rest
        .find(|ch: char| {
            !matches!(
                ch,
                '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '?' | '<' | '>'
            )
        })
        .unwrap_or(rest.len());
    let after_operator = &rest[operator_end..];
    if after_operator.starts_with('=')
        && !after_operator[1..].starts_with(['=', '>'])
        && !matches!(&rest[..operator_end], "<" | ">" | "!")
    {
        return true;
    }
    ["of", "in"].iter().any(|keyword| {
        rest.strip_prefix(keyword).is_some_and(|tail| {
            !tail.starts_with(|ch: char| ch.is_alphanumeric() || ch == '_' || ch == '$')
        })
    })
}

fn blank_byte_order_marks(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains('\u{feff}') {
        std::borrow::Cow::Owned(text.replace('\u{feff}', "   "))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

pub(crate) fn strict_identifier_writes(text: &str) -> Vec<(String, usize)> {
    scan_writes(text, true)
}

fn scan_writes(text: &str, strict: bool) -> Vec<(String, usize)> {
    let text = blank_byte_order_marks(text);
    let text = text.as_ref();
    let destructured = destructuring_ranges(text);
    let mut writes = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        if !(ch == '_' || ch == '$' || ch.is_alphabetic()) {
            index += ch.len_utf8();
            continue;
        }
        let start = index;
        while let Some(ch) = text[index..].chars().next() {
            if !(ch == '_' || ch == '$' || ch.is_alphanumeric()) {
                break;
            }
            index += ch.len_utf8();
        }
        let before = text[..start].trim_end();
        let global = strict && before.ends_with('.') && global_object_member(before);
        if before.ends_with('.') && !before.ends_with("...") && !global {
            continue;
        }
        let rest = text[index..].trim_start();
        let operator_end = rest
            .find(|ch: char| {
                !matches!(
                    ch,
                    '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '?' | '<' | '>'
                )
            })
            .unwrap_or(rest.len());
        let after_operator = &rest[operator_end..];
        let assigns = after_operator.starts_with('=')
            && !after_operator[1..].starts_with(['=', '>'])
            && !matches!(&rest[..operator_end], "<" | ">" | "!");
        let steps = rest.starts_with("++")
            || rest.starts_with("--")
            || before.ends_with("++")
            || before.ends_with("--");
        let loop_target = ["let", "const", "var"]
            .iter()
            .fold(before, |prefix, keyword| {
                prefix.strip_suffix(keyword).unwrap_or(prefix).trim_end()
            })
            .strip_suffix('(')
            .is_some_and(|prefix| {
                let prefix = prefix.trim_end();
                prefix.ends_with("for") || prefix.ends_with("await")
            });
        let in_target = destructured.iter().any(|range| range.contains(&start));
        let wrapped = strict && wrapped_write(before, rest);
        if assigns || steps || loop_target || in_target || wrapped {
            writes.push((text[start..index].to_string(), start));
        }
    }
    writes
}

fn destructuring_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    for (close, &byte) in bytes.iter().enumerate() {
        let open_byte = match byte {
            b']' => b'[',
            b'}' => b'{',
            _ => continue,
        };
        let rest = text[close + 1..].trim_start();
        let assigned = rest.starts_with('=') && !rest[1..].starts_with(['=', '>']);
        // `for ([name] of ...)` / `for ({ name } in ...)` write their targets.
        let loop_bound = ["of", "in"].iter().any(|keyword| {
            rest.strip_prefix(keyword).is_some_and(|tail| {
                tail.starts_with(|c: char| c.is_whitespace() || "([{'\"`".contains(c))
            })
        });
        if !assigned && !loop_bound {
            continue;
        }
        let mut depth = 0usize;
        for open in (0..close).rev() {
            if bytes[open] == byte {
                depth += 1;
            } else if bytes[open] == open_byte {
                if depth == 0 {
                    ranges.push(open..close);
                    break;
                }
                depth -= 1;
            }
        }
    }
    ranges
}

fn byte_preserving_code_mask(source: &str) -> Option<String> {
    let masked = ts_mask_literals_and_comments(source);
    preserve_mask_byte_width(source, &masked)
}

fn byte_preserving_code_mask_for_source_type(
    source: &str,
    source_type: SourceType,
) -> Option<String> {
    let masked =
        ts_mask_literals_and_comments_for_source_type(source, source_type);
    preserve_mask_byte_width(source, &masked)
}

fn byte_preserving_division_mask(source: &str) -> Option<String> {
    let masked = ts_mask_literals_and_comments_division_only(source);
    preserve_mask_byte_width(source, &masked)
}

fn preserve_mask_byte_width(source: &str, masked: &str) -> Option<String> {
    if source.chars().count() != masked.chars().count() {
        return None;
    }
    let mut code = String::with_capacity(source.len());
    for (original, masked) in source.chars().zip(masked.chars()) {
        if original == masked {
            code.push(original);
        } else if masked == ' ' {
            code.extend(std::iter::repeat_n(' ', original.len_utf8()));
        } else {
            return None;
        }
    }
    Some(code)
}

fn main() {
    let cases = [
        ("module", "ts", "const a = /é`/; test = null;", true),
        ("function-body", "ts", "return /é`/; test = null;", true),
        ("class-member", "ts", "run() { return /é`/; }", true),
        ("malformed-typed", "ts", "const café = `é", false),
        ("malformed-unknown", "default", "const café = `é", false),
        ("ambiguous-unknown", "default", "const v = <div>x</div>; test = null; const y = a /g;", false),
        ("ambiguous-ts", "ts", "const v = <div>x</div>; test = null; const y = a /g;", true),
        ("ambiguous-tsx", "tsx", "const v = <div>x</div>; test = null; const y = a /g;", true),
    ];
    for (name, kind, text, expected_known) in cases {
        let source_type = match kind { "ts"=>Some(SourceType::ts()), "tsx"=>Some(SourceType::tsx()), _=>None };
        let prepared = literal_mask::prepare(text, source_type);
        assert_eq!(prepared.regex_known, expected_known, "{name}");
        let legacy = ts_mask_lexically(text, true, &std::collections::BTreeSet::new());
        let normal = if let Some(st) = source_type { ts_mask_literals_and_comments_for_source_type(text, st) } else { ts_mask_literals_and_comments(text) };
        let byte_mask = preserve_mask_byte_width(text, &normal).unwrap();
        assert_eq!(text.len(), byte_mask.len(), "{name}");
        if !expected_known {
            assert_eq!(prepared.code, text, "{name}");
            assert!(prepared.operand_ends.is_empty(), "{name}");
            assert_eq!(normal, legacy, "{name}");
            assert_eq!(byte_mask, preserve_mask_byte_width(text, &legacy).unwrap(), "{name}");
        }
        println!("{name}: regex_known={} unchanged={} operand_ends={:?} byte_width={} legacy_equal={} mask={:?}", prepared.regex_known, prepared.code==text, prepared.operand_ends, byte_mask.len(), normal==legacy, byte_mask);
        for (index,(prefix,suffix)) in [("",""),("async function* __ripr_mask_body__() {\n","\n}"),("class __RiprMaskOwner__ {\n","\n}")].into_iter().enumerate() {
            let code = format!("{prefix}{text}{suffix}");
            let counts: Vec<_> = [SourceType::ts(),SourceType::tsx()].into_iter().map(|st|{let alloc=Allocator::default();Parser::new(&alloc,&code,st).parse().errors.len()}).collect();
            if name.starts_with("malformed") { assert!(counts.iter().all(|n|*n>0), "{name} prefix {index}"); }
            println!("  prefix={index} TS_errors={} TSX_errors={}",counts[0],counts[1]);
        }
    }
}
