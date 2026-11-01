//! #9871: the production functions that read a changed `const`/`static`.
//!
//! A changed constant sits outside every function, so the probe has no owner
//! and the related-test finder has no name to search for: a test that calls
//! the function reading the constant was reported as "no static test path".
//! This module names the same-file readers so the classifier can relate tests
//! that call them. It only widens which tests are *related*; the probe stays
//! `static_unknown`, so nothing here can promote a verdict.

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{
    FunctionSourceRole, FunctionSummary, ProbeShapeFact, ProbeShapeKind, RustIndex,
};
use crate::analysis::rust_index::find_file_facts;
use crate::domain::Probe;
use std::path::Path;

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

/// #11400: the one-line returned expressions through which a changed `const`
/// item reaches its readers' return values. `changed`
/// is the changed declaration line (`pub const GREETING: &str = "hi";`).
///
/// The one rule the diff probe (`ripr check`) and the repair scope
/// (`ripr repair`) both read, so they tie the same code to the change.
/// Fail closed (empty) unless all of these hold:
/// - the line declares a `const` (not a `static`, whose value may change at
///   run time) whose name is declared once in the file;
/// - the file has exactly one production reader ([`function_reads`]:
///   shadows, other-path qualifiers and escaped `{{NAME}}` are not reads),
///   and no other production code anywhere names that reader;
/// - no code outside a function reads the name, and no `use` anywhere in the
///   file imports an item spelled like it or a `format`;
/// - the name occurs, over the raw text of every workspace Rust file, only on
///   the declaration and in the reader's returned `format!`
///   ([`name_confined`]);
/// - the declaration sits directly in a module (not an `impl` or trait), the
///   reader is in that same module, and no module chain is opened twice;
/// - the reader's whole body is one one-line returned expression that
///   carries the whole constant ([`returns_constant_whole`]). A branch,
///   binding, closure, async block or second return keeps the change
///   unknown.
///
/// The caller also requires the paired old and new values to differ.
pub(in crate::analysis) fn const_reader_returns<'a>(
    root: &Path,
    changed: &str,
    declaration_line: usize,
    file: &Path,
    index: &'a RustIndex,
) -> Vec<&'a ProbeShapeFact> {
    let trimmed = changed.trim_start();
    let declares_const = trimmed.starts_with("const ")
        || trimmed
            .strip_prefix("pub")
            .is_some_and(|rest| rest.contains("const ") && !rest.contains("static "));
    let Some(name) = crate::domain::declared_const_name(changed).filter(|_| declares_const) else {
        return Vec::new();
    };
    let Some(facts) = find_file_facts(index, file) else {
        return Vec::new();
    };
    let masked = mask_comments_and_strings(&facts.source);
    let spans: Vec<(usize, usize)> = facts
        .functions
        .iter()
        .map(|function| (function.start_line, function.end_line))
        .collect();
    if declaration_count(&masked, name) != 1
        || item_level_read(&masked, name, &spans)
        || imports_name(&masked, name)
        || imports_name(&masked, "format")
        || repeats_inline_module(&masked)
        || defines_format_macro(index)
    {
        return Vec::new();
    }
    let readers = facts
        .functions
        .iter()
        .filter(|function| function.source_role == FunctionSourceRole::Production)
        .filter(|function| function_reads(&function.body, name))
        .collect::<Vec<_>>();
    // Exactly one reader, which no other production code calls: then a test
    // can reach the constant's value only through the reader itself, and a
    // second reader or caller cannot feed the expected side of its check.
    let [reader] = readers.as_slice() else {
        return Vec::new();
    };
    if reader_has_production_callers(index, reader) {
        return Vec::new();
    }
    // One module scope for the declaration and every reader, and the
    // declaration directly in a module (never in an `impl` or trait), so the
    // spelling names this constant and no associated or imported one.
    let reader_lines: Vec<usize> = readers.iter().map(|reader| reader.start_line).collect();
    if !super::shares_module_scope(&facts.source, declaration_line, &reader_lines) {
        return Vec::new();
    }
    let mut found = Vec::new();
    for owner in readers {
        let mut returns = facts.data().probe_shapes.iter().filter(|shape| {
            shape.kind == ProbeShapeKind::ReturnValue
                && owner.start_line <= shape.start_line
                && shape.end_line <= owner.end_line
        });
        // The reader's whole body is one returned expression: no branch,
        // binding, closure or async block can stand between the constant and
        // the value the caller gets.
        let (Some(shape), None) = (returns.next(), returns.next()) else {
            return Vec::new();
        };
        if shape.start_line != shape.end_line
            || !returns_constant_whole(&shape.text, name)
            || !body_is_only(&owner.body, &shape.text)
        {
            return Vec::new();
        }
        found.push(shape);
    }
    // The structural rule: in the raw text of every workspace Rust file the
    // name occurs exactly twice, once on the declaration and once in the
    // reader's returned `format!`. Any other mention (a test, a helper, a
    // `use .. as` alias, a path-qualified read, a doc test, a macro body or a
    // comment) may carry the constant's value somewhere ripr does not follow.
    match found.as_slice() {
        [shape] if name_confined(root, file, name, declaration_line, shape.start_line) => found,
        _ => Vec::new(),
    }
}

/// Whether the whole word `name` occurs, over the raw text (comments,
/// strings, doc comments and macro bodies included) of every Rust file
/// `discover_rust_files` finds under `root`, exactly once on
/// `declaration_line` and once on `return_line` of `file`, and nowhere else.
/// An unreadable file, or a `file` the discovery does not list, fails
/// closed. `ripr check` and `ripr repair` both read this same scan.
pub(in crate::analysis) fn name_confined(
    root: &Path,
    file: &Path,
    name: &str,
    declaration_line: usize,
    return_line: usize,
) -> bool {
    let target = super::related_tests::normalize_path(file);
    let mut saw_file = false;
    for path in crate::analysis::workspace_rust_files(root) {
        let Ok(text) = std::fs::read_to_string(root.join(&path)) else {
            return false;
        };
        let lines = word_lines(&text, name);
        if super::related_tests::normalize_path(&path) == target {
            saw_file = true;
            if lines != [declaration_line, return_line] {
                return false;
            }
        } else if !lines.is_empty() {
            return false;
        }
    }
    saw_file
}

/// The 1-based line of every whole-word occurrence of `name` in `text`, one
/// entry per occurrence.
fn word_lines(text: &str, name: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .flat_map(|(at, line)| {
            let bytes = line.as_bytes();
            line.match_indices(name)
                .filter(move |(start, _)| {
                    let end = start + name.len();
                    start
                        .checked_sub(1)
                        .and_then(|before| bytes.get(before))
                        .is_none_or(|byte| !is_ident_byte(*byte))
                        && !bytes.get(end).is_some_and(|byte| is_ident_byte(*byte))
                })
                .map(move |_| at + 1)
        })
        .collect()
}

/// Whether production code other than `reader` names it. A caller such as
/// `fn tag() -> String { banner_line() }` gives a test a second way to the
/// constant's value, so a test can build its expected side from it.
fn reader_has_production_callers(index: &RustIndex, reader: &FunctionSummary) -> bool {
    index.files().iter().any(|(_, facts)| {
        facts.functions.iter().any(|function| {
            function.source_role == FunctionSourceRole::Production
                && function.id != reader.id
                && contains_word(&function.body, &reader.name)
        })
    })
}

/// Whether a workspace `macro_rules! format` could replace `std::format!`.
fn defines_format_macro(index: &RustIndex) -> bool {
    index.files().iter().any(|(_, facts)| {
        facts.source.contains("macro_rules!") && {
            let masked = mask_comments_and_strings(&facts.source);
            masked.match_indices("macro_rules!").any(|(at, found)| {
                let rest = masked
                    .get(at + found.len()..)
                    .unwrap_or_default()
                    .trim_start();
                rest.strip_prefix("format").is_some_and(|after| {
                    after
                        .as_bytes()
                        .first()
                        .is_none_or(|byte| !is_ident_byte(*byte))
                })
            })
        }
    })
}

/// Whether one chain of inline modules (`mod a { .. }`) is opened twice, as a
/// pair of `#[cfg]` alternatives does: spelling cannot tell which copy a
/// reader or the declaration belongs to.
fn repeats_inline_module(masked: &str) -> bool {
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut seen: std::collections::BTreeSet<Vec<Option<String>>> =
        std::collections::BTreeSet::new();
    for line in masked.lines() {
        let mut words = line.split_whitespace().peekable();
        let mut module: Option<String> = None;
        while let Some(word) = words.next() {
            if word == "mod"
                && let Some(name) = words.peek()
            {
                module = Some(name.trim_end_matches(['{', ';']).to_string());
            }
        }
        // `mod a` with its `{` on a later line: the chain cannot be read
        // line by line, so treat it as ambiguous.
        if module.is_some() && !line.contains(['{', ';']) {
            return true;
        }
        for ch in line.chars() {
            match ch {
                '{' => {
                    let opened = module.take();
                    if opened.is_some() {
                        let mut chain = stack.clone();
                        chain.push(opened.clone());
                        if !seen.insert(chain) {
                            return true;
                        }
                    }
                    stack.push(opened);
                }
                '}' => {
                    stack.pop();
                }
                _ => {}
            }
        }
    }
    false
}

/// Whether the function `body` (signature included) consists of exactly the
/// returned expression `returned`, as a tail expression or one `return`.
fn body_is_only(body: &str, returned: &str) -> bool {
    let masked = mask_comments_and_strings(body);
    let (Some(open), Some(close)) = (masked.find('{'), masked.rfind('}')) else {
        return false;
    };
    if body.find('{') != Some(open) || body.rfind('}') != Some(close) || close <= open {
        return false;
    }
    let squeeze = |text: &str| {
        let text = text.trim();
        let text = text.strip_prefix("return ").unwrap_or(text);
        let text = text.strip_suffix(';').unwrap_or(text);
        text.split_whitespace().collect::<String>()
    };
    let inner = body.get(open + 1..close).unwrap_or_default();
    !squeeze(returned).is_empty() && squeeze(inner) == squeeze(returned)
}

/// Whether a `use` declaration anywhere in the file names `name`: an import
/// of another item spelled like the constant.
fn imports_name(masked: &str, name: &str) -> bool {
    let mut rest = masked;
    while let Some(at) = rest.find("use") {
        let before_ok = at
            .checked_sub(1)
            .and_then(|before| rest.as_bytes().get(before))
            .is_none_or(|byte| !is_ident_byte(*byte));
        let after = rest.get(at + 3..).unwrap_or_default();
        let after_ok = after
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_whitespace());
        if before_ok && after_ok {
            let statement = after.split(';').next().unwrap_or_default();
            if contains_word(statement, name) {
                return true;
            }
        }
        rest = after;
    }
    false
}

/// Whether `text` holds `name` as a whole identifier.
fn contains_word(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(name).any(|(at, _)| {
        let end = at + name.len();
        at.checked_sub(1)
            .and_then(|before| bytes.get(before))
            .is_none_or(|byte| !is_ident_byte(*byte))
            && !bytes.get(end).is_some_and(|byte| is_ident_byte(*byte))
    })
}

/// Whether code outside every function reads `name`: another constant's
/// initializer, a compile-time `assert!`, an array length or a `static`. Such
/// a read is not a returned value, so the change is not described through the
/// readers. The declaration line is not a read.
fn item_level_read(masked: &str, name: &str, functions: &[(usize, usize)]) -> bool {
    masked.lines().enumerate().any(|(at, line)| {
        let number = at + 1;
        !functions
            .iter()
            .any(|&(start, end)| start <= number && number <= end)
            && crate::domain::declared_const_name(line) != Some(name)
            && code_reads(line, name)
    })
}

/// Whether the returned expression `text` changes whenever the constant
/// `name` does, for the same inputs: one `format!` whose only reads are plain
/// `{NAME}` inline arguments, with no named argument and no other argument
/// naming the constant. A bare `NAME` return is not followed: the value-flow
/// evaluation decides it (an exact pin reads exposed), and a retargeted bare
/// name would read as a value ripr cannot follow while repair started on it.
/// A comparison (`x > NAME`), arithmetic, a format spec (`{NAME:.3}`) or any
/// other use may hide the change for some input, so it is not followed either.
fn returns_constant_whole(text: &str, name: &str) -> bool {
    let text = text
        .trim()
        .trim_start_matches("return ")
        .trim_end_matches(';')
        .trim();
    let Some(inner) = text
        .strip_prefix("format!(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    // One string literal, then only plain arguments that are not the name.
    let Some(literal_end) = inner
        .strip_prefix('"')
        .and_then(|rest| rest.find('"'))
        .map(|end| end + 2)
    else {
        return false;
    };
    let literal = &inner[..literal_end];
    if literal.contains('\\') {
        return false;
    }
    let plain = format!("{{{name}}}");
    let arguments = &inner[literal_end..];
    literal.contains(&plain)
        && !literal.replace(&plain, "").contains(&format!("{{{name}"))
        && !literal.contains("{{")
        && !contains_word(arguments, name)
        && !arguments.contains(['"', '(', '='])
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
    use super::{
        body_is_only, declaration_count, function_reads, imports_name, repeats_inline_module,
        returns_constant_whole, word_lines,
    };

    #[test]
    fn a_module_opened_twice_or_split_from_its_brace_is_ambiguous() {
        assert!(repeats_inline_module("mod a {\n}\nmod a {\n}\n"));
        assert!(repeats_inline_module("pub mod a\n{\n}\n"));
        assert!(!repeats_inline_module("mod a {\n}\nmod b {\n}\nmod c;\n"));
    }

    #[test]
    fn word_lines_count_every_raw_occurrence() {
        let text = "pub const BANNER: &str = \"v1\";\n/// `fx::BANNER`\nfn f() -> String { format!(\"[{BANNER}]\") }\nconst BANNER_X: u8 = 1;\n";
        assert_eq!(word_lines(text, "BANNER"), vec![1, 2, 3]);
        assert_eq!(word_lines("{BANNER}{BANNER}", "BANNER"), vec![1, 1]);
    }

    #[test]
    fn only_a_body_that_is_the_returned_expression_is_followed() {
        let returned = "format!(\"{GREETING}, {name}\")";
        for body in [
            "pub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}",
            "pub fn greet(name: &str) -> String {\n    return format!(\"{GREETING}, {name}\");\n}",
        ] {
            assert!(body_is_only(body, returned), "{body}");
        }
        for body in [
            "pub fn greet(name: &str, f: bool) -> String {\n    if f { return format!(\"{GREETING}, {name}\"); }\n    String::new()\n}",
            "pub fn greet(name: &str) -> String {\n    let _ = 1;\n    format!(\"{GREETING}, {name}\")\n}",
        ] {
            assert!(!body_is_only(body, returned), "{body}");
        }
    }

    #[test]
    fn any_use_of_the_name_is_an_import() {
        assert!(imports_name("use crate::lang::GREETING;", "GREETING"));
        assert!(imports_name(
            "mod fr {\n    use crate::{\n        lang::GREETING,\n    };\n}",
            "GREETING"
        ));
        assert!(!imports_name("use crate::lang::GREETING_X;", "GREETING"));
        assert!(!imports_name("fn reuse() { GREETING }", "GREETING"));
    }

    #[test]
    fn only_a_return_that_carries_the_whole_constant_is_followed() {
        for text in [
            "format!(\"{GREETING}, {name}\")",
            "return format!(\"{GREETING}\");",
            "format!(\"{GREETING}{GREETING}\")",
        ] {
            assert!(returns_constant_whole(text, "GREETING"), "{text}");
        }
        for text in [
            "GREETING",
            "self::GREETING",
            "GREETING.to_string()",
            "return GREETING;",
            "x > GREETING",
            "GREETING + 1",
            "GREETING.len()",
            "format!(\"{GREETING:.3}\")",
            "format!(\"{}\", GREETING)",
            "format!(\"{GREETING}\").len()",
            "format!(\"{{GREETING}}\")",
            "format!(\"\\\"{GREETING}\")",
            "format!(\"{GREETING}{n}\", n = f(1))",
            "Other::GREETING",
            "Self::GREETING",
            "format!(\"{GREETING}\", GREETING = name)",
            "format!(\"{GREETING}{x}\", x = GREETING)",
        ] {
            assert!(!returns_constant_whole(text, "GREETING"), "{text}");
        }
    }

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
}
