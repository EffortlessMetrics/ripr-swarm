//! #9871: the production functions that read a changed `const`/`static`.
//!
//! A changed constant sits outside every function, so the probe has no owner
//! and the related-test finder has no name to search for: a test that calls
//! the function reading the constant was reported as "no static test path".
//! This module names the same-file readers so the classifier can relate tests
//! that call them. It only widens which tests are *related*; the probe stays
//! `static_unknown`, so nothing here can promote a verdict.

use crate::analysis::diff::ChangedFile;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{
    FactSlice, FileFacts, FileFactsView, FunctionFact, FunctionSourceRole, FunctionSummary,
    ProbeShapeFact, ProbeShapeKind, RustIndex,
};
use crate::analysis::rust_index::find_file_facts;
use crate::domain::Probe;
use std::path::{Path, PathBuf};

/// Readers tried per changed constant, matching the `ripr check` stub route.
const READER_LIMIT: usize = 6;

/// The production functions of the probe's file that name the constant the
/// probe's changed line declares. Empty (fail closed) when the probe does not
/// declare a named constant, when the name is declared more than once in the
/// file (an associated const per impl, a const generic, a local `const`
/// cannot be told apart by spelling), or when no function names it.
pub(in crate::analysis) fn same_file_const_readers<'a>(
    probe: &Probe,
    probe_file: &Path,
    index: &'a RustIndex,
) -> Vec<&'a FunctionSummary> {
    let Some(name) = probe.changed_const_name() else {
        return Vec::new();
    };
    let Some(facts) = find_file_facts(index, probe_file) else {
        return Vec::new();
    };
    if declaration_count(&mask_comments_and_strings(&facts.source), name) != 1 {
        return Vec::new();
    }
    facts
        .functions
        .iter()
        .filter(|function| function.source_role == FunctionSourceRole::Production)
        .filter(|function| function_reads(&function.body, name))
        .take(READER_LIMIT)
        .collect()
}

/// #11400: the declaration name of a complete single-line `const`/`static`
/// item (`pub const NAME: T = ...;`), the only shape the consumer retarget
/// reads. A declaration split over lines keeps the generic changed-constant
/// finding.
pub(in crate::analysis) fn retargetable_const_name(text: &str) -> Option<&str> {
    let text = text.trim();
    if !text.ends_with(';') {
        return None;
    }
    crate::domain::const_item_name(text)
}

/// The parts of a file's facts the consumer relation reads, so the indexed
/// view `ripr check` holds and the freshly parsed facts `ripr repair` builds
/// feed one function.
#[derive(Clone, Copy)]
pub(in crate::analysis) struct ConstFileFacts<'a> {
    source: &'a str,
    functions: FactSlice<'a, FunctionFact>,
    probe_shapes: &'a [ProbeShapeFact],
}

impl<'a> ConstFileFacts<'a> {
    pub(in crate::analysis) fn from_view(view: &'a FileFactsView<'a>) -> Self {
        Self {
            source: &view.data().source,
            functions: view.functions,
            probe_shapes: &view.data().probe_shapes,
        }
    }

    pub(in crate::analysis) fn from_facts(facts: &'a FileFacts) -> Self {
        Self {
            source: &facts.source,
            functions: FactSlice::from_slice(&facts.functions),
            probe_shapes: &facts.probe_shapes,
        }
    }
}

/// One line that returns or renders a value built from a changed constant,
/// with the parser's `return_value` expression that starts on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct ConstConsumer {
    pub line: usize,
    pub expression: String,
}

/// #11400: the consumers of the constant `name`: [`const_consumer_lines`] that
/// the diff does not itself change (`changed_lines`, a directly changed line
/// already carries its own probe) and on which the parser starts a
/// single-line `return_value` shape. This is the one relation `ripr check`
/// (probe retarget) and `ripr repair` (change scope) both read.
pub(in crate::analysis) fn const_consumers(
    facts: ConstFileFacts<'_>,
    name: &str,
    changed_lines: &[usize],
) -> Vec<ConstConsumer> {
    const_consumer_lines(facts, name)
        .into_iter()
        .filter(|line| !changed_lines.contains(line))
        .filter_map(|line| {
            facts
                .probe_shapes
                .iter()
                .find(|shape| {
                    shape.kind == ProbeShapeKind::ReturnValue
                        && shape.start_line == line
                        && shape.end_line == line
                })
                .map(|shape| ConstConsumer {
                    line,
                    expression: shape.text.to_string(),
                })
        })
        .collect()
}

/// #11400: `(file, line)` for every consumer line of every constant the diff
/// declares, read from the working tree under `root`. Unreadable or
/// unparseable files contribute nothing (fail closed).
pub(crate) fn changed_const_consumer_lines(
    root: &Path,
    files: &[ChangedFile],
) -> Vec<(PathBuf, usize)> {
    let mut found = Vec::new();
    for file in files {
        let names: Vec<&str> = file
            .added_lines
            .iter()
            .filter_map(|line| retargetable_const_name(&line.text))
            .collect();
        if names.is_empty() || file.path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(root.join(&file.path)) else {
            continue;
        };
        let Ok(facts) =
            crate::analysis::syntax::ra::summarize_file_with_parser(&file.path, &source)
        else {
            continue;
        };
        let changed_lines: Vec<usize> = file
            .added_lines
            .iter()
            .chain(file.removed_lines.iter())
            .map(|line| line.new_side_line)
            .collect();
        for name in names {
            for consumer in
                const_consumers(ConstFileFacts::from_facts(&facts), name, &changed_lines)
            {
                found.push((file.path.clone(), consumer.line));
            }
        }
    }
    found
}

/// #11400: the 1-based lines of same-file production functions that read the
/// constant `name` as one plain top-level statement of the function body.
///
/// This is the diff-side relation that lets a changed constant stand for the
/// code that consumes it. It is deliberately narrower than
/// [`same_file_const_readers`]: the name must be declared exactly once in the
/// file, the reading function must not shadow it ([`function_reads`]), and the
/// reading line must sit directly in the function body (brace depth one, no
/// nested block, closure or short-circuit on the line) with no earlier
/// `return` or `?` in the function. Anything else reads the constant only
/// conditionally, so it yields no line and the caller keeps the generic
/// changed-constant finding.
fn const_consumer_lines(facts: ConstFileFacts<'_>, name: &str) -> Vec<usize> {
    let masked_source = mask_comments_and_strings(facts.source);
    if declaration_count(&masked_source, name) != 1 {
        return Vec::new();
    }
    let raw_lines: Vec<&str> = facts.source.lines().collect();
    let masked_lines: Vec<&str> = masked_source.lines().collect();
    let mut lines = Vec::new();
    for function in facts
        .functions
        .iter()
        .filter(|function| function.source_role == FunctionSourceRole::Production)
        .filter(|function| function_reads(&function.body, name))
        .filter(|function| !super::const_flow::reader_is_evaluable(&function.body))
        .take(READER_LIMIT)
    {
        lines.extend(function_consumer_lines(
            function.start_line,
            function.end_line,
            &raw_lines,
            &masked_lines,
            name,
        ));
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn function_consumer_lines(
    start_line: usize,
    end_line: usize,
    raw_lines: &[&str],
    masked_lines: &[&str],
    name: &str,
) -> Vec<usize> {
    let mut found = Vec::new();
    // Brace depth inside the function: 0 before its opening brace.
    let mut depth = 0usize;
    let mut early_exit = false;
    for line in start_line..=end_line.min(masked_lines.len()) {
        let Some(&masked) = line
            .checked_sub(1)
            .and_then(|index| masked_lines.get(index))
        else {
            break;
        };
        let raw = raw_lines.get(line - 1).copied().unwrap_or("");
        let braces = masked.contains(['{', '}']);
        if depth == 1
            && !braces
            && !early_exit
            && !masked.contains(['|', '?'])
            && !masked.contains("&&")
            && (code_reads(masked, name) || format_macro_reads(raw, name))
        {
            found.push(line);
        }
        if depth >= 1 && (contains_word(masked, "return") || masked.contains('?')) {
            early_exit = true;
        }
        for byte in masked.bytes() {
            match byte {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    found
}

fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(found) = text[start..].find(word) {
        let at = start + found;
        let end = at + word.len();
        start = end;
        if (at == 0 || !is_ident_byte(bytes[at - 1]))
            && !bytes.get(end).is_some_and(|b| is_ident_byte(*b))
        {
            return true;
        }
    }
    false
}

/// Whether `body` reads the constant `name`: a bare identifier, `Self::NAME`
/// or `self::NAME` in code, or an inline format argument (`{NAME}`,
/// `{NAME:>4}`) in a format macro's string literal. Fail closed: a name
/// qualified by another path (`crate::other::NAME`, `Other::NAME`), a field or
/// method (`x.NAME`), a local `let NAME` shadow, an escaped `{{NAME}}`, a
/// comment, or a plain string is not a read.
pub(in crate::analysis) fn function_reads(body: &str, name: &str) -> bool {
    let masked = mask_comments_and_strings(body);
    if shadows(&masked, name) {
        return false;
    }
    code_reads(&masked, name) || format_macro_reads(body, name)
}

/// A local, loop variable, pattern binding or parameter spelled like the
/// constant. Every check is whole-word, so `let LIMIT_X` does not shadow.
fn shadows(masked: &str, name: &str) -> bool {
    let bytes = masked.as_bytes();
    let mut start = 0;
    while let Some(found) = masked[start..].find(name) {
        let at = start + found;
        let end = at + name.len();
        start = end;
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && !bytes.get(end).is_some_and(|b| is_ident_byte(*b));
        if !whole {
            continue;
        }
        let before = masked[..at].trim_end();
        let after = masked[end..].trim_start();
        let prev = before.chars().next_back();
        // `let NAME`, `let mut NAME`, `mut NAME: T`, `for NAME in`.
        if ends_with_word(before, "let")
            || ends_with_word(before, "for")
            || (ends_with_word(before, "mut") && !before.trim_end_matches("mut").ends_with('&'))
        {
            return true;
        }
        // A binding inside a `let (a, NAME)` / `let [a, NAME]` pattern.
        if let Some(let_at) = before.rfind("let ")
            && !before[let_at..].contains(['=', ';', '{'])
            && (let_at == 0 || !is_ident_byte(before.as_bytes()[let_at - 1]))
        {
            return true;
        }
        // Parameters: `(NAME: T`, `, NAME: T`, closure `|NAME|`, `|NAME,`,
        // `|a, NAME|`.
        let typed = after.starts_with(':') && !after.starts_with("::");
        if (typed && matches!(prev, Some('(' | ',' | '|')))
            || (prev == Some('|') && after.starts_with(['|', ',']))
            || (prev == Some(',') && after.starts_with('|'))
        {
            return true;
        }
    }
    false
}

/// Whether `text` ends with the whole word `word`.
fn ends_with_word(text: &str, word: &str) -> bool {
    text.strip_suffix(word)
        .is_some_and(|rest| rest.as_bytes().last().is_none_or(|b| !is_ident_byte(*b)))
}

/// Whether the occurrence ending at `end` is only assigned to (`NAME = x`,
/// `NAME += x`), which writes the item instead of reading its value.
fn is_assigned(masked: &str, end: usize) -> bool {
    let rest = masked[end..].trim_start();
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some('='), next) => next != Some('=') && next != Some('>'),
        (Some('+' | '-' | '*' | '/' | '%' | '^' | '|' | '&'), Some('=')) => true,
        (Some('<'), Some('<')) | (Some('>'), Some('>')) => chars.next() == Some('='),
        _ => false,
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn code_reads(masked: &str, name: &str) -> bool {
    let bytes = masked.as_bytes();
    let mut start = 0;
    while let Some(found) = masked[start..].find(name) {
        let at = start + found;
        let end = at + name.len();
        start = end;
        if (at > 0 && is_ident_byte(bytes[at - 1]))
            || bytes.get(end).is_some_and(|b| is_ident_byte(*b))
        {
            continue;
        }
        let before = masked[..at].trim_end();
        if (before.ends_with('.') && !before.ends_with("..")) || is_assigned(masked, end) {
            continue;
        }
        if let Some(path) = before.strip_suffix("::") {
            let segment_start = path
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            if !matches!(&path[segment_start..], "Self" | "self") {
                continue;
            }
            // `a::Self::NAME` is still another path.
            if path[..segment_start].trim_end().ends_with("::") {
                continue;
            }
        }
        return true;
    }
    false
}

/// Format macros and the zero-based argument index of their format string.
const FORMAT_MACROS: [(&str, usize); 12] = [
    ("format", 0),
    ("print", 0),
    ("println", 0),
    ("eprint", 0),
    ("eprintln", 0),
    ("write", 1),
    ("writeln", 1),
    ("panic", 0),
    ("format_args", 0),
    ("assert", 1),
    ("assert_eq", 2),
    ("assert_ne", 2),
];

/// Whether the format-string argument of a format macro in `body`
/// captures `name` inline. Comments are skipped; a literal outside a format
/// macro, or an escaped `{{name}}`, does not count.
fn format_macro_reads(body: &str, name: &str) -> bool {
    let chars: Vec<char> = body.chars().collect();
    // Per open bracket: the format-string argument index (when it is a
    // format macro's parenthesis) and the commas seen at its level.
    let mut stack: Vec<(Option<usize>, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            '\'' => {
                // A char literal (`'"'`, `'\n'`); a lifetime has no closing quote.
                if chars.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else if chars.get(i + 1) == Some(&'\\') && chars.get(i + 3) == Some(&'\'') {
                    i += 4;
                } else {
                    i += 1;
                }
            }
            '(' => {
                let mut j = i;
                while j > 0 && chars[j - 1].is_whitespace() {
                    j -= 1;
                }
                let position = if j > 0 && chars[j - 1] == '!' {
                    let end = j - 1;
                    let mut k = end;
                    while k > 0 && (chars[k - 1].is_alphanumeric() || chars[k - 1] == '_') {
                        k -= 1;
                    }
                    let ident: String = chars[k..end].iter().collect();
                    FORMAT_MACROS
                        .iter()
                        .find(|(macro_name, _)| *macro_name == ident)
                        .map(|(_, position)| *position)
                } else {
                    None
                };
                stack.push((position, 0));
                i += 1;
            }
            '[' | '{' => {
                stack.push((None, 0));
                i += 1;
            }
            ')' | ']' | '}' => {
                stack.pop();
                i += 1;
            }
            ',' => {
                if let Some(frame) = stack.last_mut() {
                    frame.1 += 1;
                }
                i += 1;
            }
            '"' => {
                let mut literal = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        literal.push(chars[i]);
                        i += 1;
                    }
                    literal.push(chars[i]);
                    i += 1;
                }
                i += 1;
                if stack
                    .last()
                    .is_some_and(|(position, commas)| *position == Some(*commas))
                    && literal_captures(&literal, name)
                {
                    return true;
                }
            }
            _ => i += 1,
        }
    }
    false
}

/// Whether a format string captures `name` inline (`{name}`, `{name:?}`),
/// treating `{{` as an escaped brace.
fn literal_captures(literal: &str, name: &str) -> bool {
    let bytes = literal.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if bytes.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            let rest = &literal[i + 1..];
            let end = rest.find(['}', ':']).unwrap_or(rest.len());
            if &rest[..end] == name {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// How many times `source` declares `name` as a `const` or `static` item (or
/// a const generic parameter, which shares the spelling).
fn declaration_count(masked_source: &str, name: &str) -> usize {
    let mut count = 0;
    for keyword in ["const", "static"] {
        let mut start = 0;
        while let Some(found) = masked_source[start..].find(keyword) {
            let at = start + found;
            start = at + keyword.len();
            let before_ok = masked_source[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            if !before_ok {
                continue;
            }
            let rest = masked_source[start..].trim_start();
            if rest.len() == masked_source[start..].len() {
                continue;
            }
            let rest = rest
                .strip_prefix("mut ")
                .map_or(rest, |after_mut| after_mut.trim_start());
            if let Some(after_name) = rest.strip_prefix(name)
                && after_name
                    .chars()
                    .next()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
            {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::{declaration_count, function_reads};

    #[test]
    fn reads_accept_bare_self_and_inline_format_captures() {
        assert!(function_reads("{ attempt < LIMIT }", "LIMIT"));
        assert!(function_reads("{ attempt < Self::LIMIT }", "LIMIT"));
        assert!(function_reads("{ attempt < self::LIMIT }", "LIMIT"));
        assert!(function_reads("{ attempt == LIMIT }", "LIMIT"));
        // A range operator is not field access.
        assert!(function_reads("{ for i in 0..LIMIT { go(i) } }", "LIMIT"));
        assert!(function_reads("{ &v[..LIMIT] }", "LIMIT"));
        assert!(function_reads("{ a..LIMIT }", "LIMIT"));
        assert!(function_reads("{ a..=LIMIT }", "LIMIT"));
        assert!(function_reads("{ let LIMIT_X = 1; LIMIT }", "LIMIT"));
        assert!(function_reads("{ f(&mut LIMIT) }", "LIMIT"));
        assert!(function_reads(
            "{ assert_eq!(v.len(), 2, \"{LIMIT}\") }",
            "LIMIT"
        ));
        assert!(function_reads("{ write!(f, \"{LIMIT}\") }", "LIMIT"));
        assert!(function_reads("{ f(a, LIMIT) }", "LIMIT"));
        assert!(function_reads(
            "{ if LIMIT >= 2 { 1 } else { 0 } }",
            "LIMIT"
        ));
        assert!(function_reads("{ format!(\"{LIMIT}\\n\") }", "LIMIT"));
        assert!(function_reads("{ format!(\"{LIMIT:>4}\") }", "LIMIT"));
        assert!(function_reads("{ write!(f, \"x={LIMIT:?}\") }", "LIMIT"));
        // `{{` escapes, so the odd brace still captures.
        assert!(function_reads("{ format!(\"{{{LIMIT}}}\") }", "LIMIT"));
    }

    #[test]
    fn reads_refuse_other_constants_and_non_reads() {
        for body in [
            "{ attempt < LIMIT_MAX }",
            "{ attempt } // LIMIT",
            "{ \"LIMIT\" }",
            "{ crate::other::LIMIT }",
            "{ Other::LIMIT }",
            "{ a::Self::LIMIT }",
            "{ x.LIMIT }",
            "{ x . LIMIT }",
            "{ LIMIT <<= 1; }",
            "{ LIMIT >>= 1; }",
            "{ let (a, LIMIT) = p; LIMIT }",
            "{ let mut LIMIT: u32 = 1; LIMIT }",
            "{ v.map(|a, LIMIT| LIMIT) }",
            "{ v.map(|LIMIT, b| LIMIT) }",
            "{ println!(\"{}\", \"{LIMIT}\") }",
            "{ assert!(s == \"{LIMIT}\") }",
            "{ x.\n LIMIT }",
            "{ let LIMIT = 1; LIMIT }",
            "{ LIMIT = 5; }",
            "{ LIMIT += 1; }",
            "fn f(LIMIT: u32) -> u32 { LIMIT }",
            "fn f(a: u32, LIMIT: u32) -> u32 { LIMIT }",
            "{ for LIMIT in 0..3 { use_it(LIMIT) } }",
            "{ format!(\"{{LIMIT}}\") }",
            "{ format!(\"{label}\") } // {LIMIT}",
            "{ // format!(\"{LIMIT}\")\n 1 }",
            "{ \"{LIMIT}\".to_string() }",
            "{ format!(\"{label}\") }",
        ] {
            assert!(!function_reads(body, "LIMIT"), "{body}");
        }
    }

    #[test]
    fn declaration_count_counts_items_and_generics_not_prefixes() {
        assert_eq!(declaration_count("pub const LIMIT: u32 = 3;", "LIMIT"), 1);
        assert_eq!(declaration_count("static mut LIMIT: u32 = 3;", "LIMIT"), 1);
        assert_eq!(
            declaration_count(
                "const LIMIT: u32 = 3;\nimpl A { const LIMIT: u32 = 4; }",
                "LIMIT"
            ),
            2
        );
        assert_eq!(
            declaration_count("struct B<const LIMIT: usize>;", "LIMIT"),
            1
        );
        // A longer name, a read, and a `const fn` do not declare `LIMIT`.
        assert_eq!(declaration_count("const LIMIT_MAX: u32 = 3;", "LIMIT"), 0);
        assert_eq!(declaration_count("fn f() -> u32 { LIMIT }", "LIMIT"), 0);
        assert_eq!(declaration_count("const fn LIMIT() {}", "LIMIT"), 0);
        assert_eq!(declaration_count("reconst LIMIT", "LIMIT"), 0);
        // Visibility, attributes, line breaks and a local const all count.
        assert_eq!(
            declaration_count("pub(crate) const LIMIT: u32 = 3;", "LIMIT"),
            1
        );
        assert_eq!(
            declaration_count("#[allow(x)]\npub static\n    LIMIT: u32 = 3;", "LIMIT"),
            1
        );
        assert_eq!(
            declaration_count(
                "const LIMIT: u32 = 3;\nfn f() { const LIMIT: u32 = 9; }",
                "LIMIT"
            ),
            2
        );
        assert_eq!(declaration_count("const _: u32 = 3;", "LIMIT"), 0);
    }

    fn consumers(source: &str, changed: &[usize]) -> Vec<(usize, String)> {
        let facts = crate::analysis::syntax::ra::summarize_file_with_parser(
            std::path::Path::new("src/lib.rs"),
            source,
        )
        .expect("fixture parses");
        let name = super::retargetable_const_name(source.lines().next().unwrap_or(""))
            .expect("first line declares a constant");
        super::const_consumers(super::ConstFileFacts::from_facts(&facts), name, changed)
            .into_iter()
            .map(|c| (c.line, c.expression))
            .collect()
    }

    #[test]
    fn consumers_name_the_plain_returned_read() {
        let source = "pub const G: &str = \"hi\";\n\npub fn greet(n: &str) -> String {\n    format!(\"{G}, {n}\")\n}\n";
        let found = consumers(source, &[1]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, 4);
    }

    #[test]
    fn consumers_refuse_unsafe_reads() {
        let head = "pub const G: &str = \"hi\";\n";
        for body in [
            // shadowed by a binding
            "pub fn f(n: &str) -> String {\n    let G = 1;\n    format!(\"{G}{n}\")\n}\n",
            // shadowed by a parameter
            "pub fn f(G: &str) -> String {\n    format!(\"{G}\")\n}\n",
            // another module's namesake
            "pub fn f() -> String {\n    other::G.to_string()\n}\n",
            // conditional read
            "pub fn f(b: bool) -> String {\n    if b {\n        G.to_string()\n    } else {\n        String::new()\n    }\n}\n",
            // early return before the read
            "pub fn f(b: bool) -> String {\n    if b { return String::new(); }\n    G.to_string()\n}\n",
            // no reader
            "pub fn f() -> String {\n    String::new()\n}\n",
        ] {
            let source = format!("{head}\n{body}");
            assert!(consumers(&source, &[1]).is_empty(), "{body}");
        }
        // declared twice
        let twice = "pub const G: &str = \"hi\";\nmod m { pub const G: &str = \"x\"; }\npub fn f() -> String {\n    G.to_string()\n}\n";
        assert!(consumers(twice, &[1]).is_empty());
        // a reader the diff already changes keeps its own probe
        let source =
            "pub const G: &str = \"hi\";\n\npub fn f() -> String {\n    G.to_string()\n}\n";
        assert!(consumers(source, &[1, 4]).is_empty());
    }
}
