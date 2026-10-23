//! #9871: the production functions that read a changed `const`/`static`.
//!
//! A changed constant sits outside every function, so the probe has no owner
//! and the related-test finder has no name to search for: a test that calls
//! the function reading the constant was reported as "no static test path".
//! This module names the same-file readers so the classifier can relate tests
//! that call them. It only widens which tests are *related*; the probe stays
//! `static_unknown`, so nothing here can promote a verdict.

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionSourceRole, FunctionSummary, RustIndex};
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

/// Whether `body` reads the constant `name`: a bare identifier, `Self::NAME`
/// or `self::NAME` in code, or an inline format argument (`{NAME}`,
/// `{NAME:>4}`) in a format macro's string literal. Fail closed: a name
/// qualified by another path (`crate::other::NAME`, `Other::NAME`), a field or
/// method (`x.NAME`), a local `let NAME` shadow, an escaped `{{NAME}}`, a
/// comment, or a plain string is not a read.
fn function_reads(body: &str, name: &str) -> bool {
    let masked = mask_comments_and_strings(body);
    if shadows(&masked, name) {
        return false;
    }
    code_reads(&masked, name) || format_macro_reads(body, name)
}

/// A local, loop variable or parameter spelled like the constant.
fn shadows(masked: &str, name: &str) -> bool {
    if masked.contains(&format!("let {name}"))
        || masked.contains(&format!("let mut {name}"))
        || masked.contains(&format!("for {name} in"))
        || masked.contains(&format!("|{name}|"))
    {
        return true;
    }
    let bytes = masked.as_bytes();
    let mut start = 0;
    while let Some(found) = masked[start..].find(name) {
        let at = start + found;
        let end = at + name.len();
        start = end;
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && !bytes.get(end).is_some_and(|b| is_ident_byte(*b));
        if whole
            && masked[end..].trim_start().starts_with(':')
            && !masked[end..].trim_start().starts_with("::")
            && matches!(
                masked[..at].trim_end().chars().next_back(),
                Some('(' | ',' | '|')
            )
        {
            return true;
        }
    }
    false
}

/// Whether the occurrence ending at `end` is only assigned to (`NAME = x`,
/// `NAME += x`), which writes the item instead of reading its value.
fn is_assigned(masked: &str, end: usize) -> bool {
    let rest = masked[end..].trim_start();
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some('='), next) => next != Some('=') && next != Some('>'),
        (Some('+' | '-' | '*' | '/' | '%' | '^' | '|' | '&'), Some('=')) => true,
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

const FORMAT_MACROS: [&str; 12] = [
    "format",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "panic",
    "format_args",
    "assert",
    "assert_eq",
    "assert_ne",
];

/// Whether a string literal that is an argument of a format macro in `body`
/// captures `name` inline. Comments are skipped; a literal outside a format
/// macro, or an escaped `{{name}}`, does not count.
fn format_macro_reads(body: &str, name: &str) -> bool {
    let chars: Vec<char> = body.chars().collect();
    let mut stack: Vec<bool> = Vec::new();
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
                let is_macro = j > 0 && chars[j - 1] == '!' && {
                    let end = j - 1;
                    let mut k = end;
                    while k > 0 && (chars[k - 1].is_alphanumeric() || chars[k - 1] == '_') {
                        k -= 1;
                    }
                    let ident: String = chars[k..end].iter().collect();
                    FORMAT_MACROS.contains(&ident.as_str())
                };
                stack.push(is_macro);
                i += 1;
            }
            ')' => {
                stack.pop();
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
                if stack.last() == Some(&true) && literal_captures(&literal, name) {
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
