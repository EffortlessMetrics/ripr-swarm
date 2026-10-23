//! #9871: the production functions that read a changed `const`/`static`.
//!
//! A changed constant sits outside every function, so the probe has no owner
//! and the related-test finder has no name to search for: a test that calls
//! the function reading the constant was reported as "no static test path".
//! This module names the same-file readers so the classifier can relate tests
//! that call them. It only widens which tests are *related*; the probe stays
//! `static_unknown`, so nothing here can promote a verdict.

use super::reveal::contains_as_whole_word;
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

/// Whether `body` names `name` in code, or as an inline format argument
/// (`format!("{NAME}")`, `{NAME:>4}`), which string masking would hide.
fn function_reads(body: &str, name: &str) -> bool {
    contains_as_whole_word(&mask_comments_and_strings(body), name)
        || body.contains(&format!("{{{name}}}"))
        || body.contains(&format!("{{{name}:"))
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
    fn reads_include_inline_format_arguments_but_not_prefixes_or_comments() {
        assert!(function_reads("{ attempt < LIMIT }", "LIMIT"));
        assert!(function_reads("{ format!(\"{LIMIT}\\n\") }", "LIMIT"));
        assert!(function_reads("{ format!(\"{LIMIT:>4}\") }", "LIMIT"));
        assert!(!function_reads("{ attempt < LIMIT_MAX }", "LIMIT"));
        assert!(!function_reads("{ attempt } // LIMIT", "LIMIT"));
        assert!(!function_reads("{ \"LIMIT\" }", "LIMIT"));
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
    }
}
