//! #12040: a changed line that a compile-time assertion constrains.
//!
//! A changed `const` has no owner function, so it reads `no_changed_rust_line`
//! even when an item-level assertion in the same file names it
//! (`const _: () = assert!(N <= 16);`): a violating edit fails to build, not a
//! test. A changed continuation line of a multi-line assertion has the same
//! shape. This only picks the stop reason; the class stays `static_unknown`.
//! Fail closed on anything not spelled plainly: the assertion must be an item
//! in the probe's own file, and a name declared more than once is not linked.

use super::const_readers::{declaration_count, is_ident_byte};
use super::decision::is_compile_time_assertion;

/// Whether the changed line `probe_line` (1-based) of `masked_source` (comments
/// and strings already blanked, lines preserved) sits inside a compile-time
/// assertion item, or declares `const_name` that such an item names.
pub(in crate::analysis) fn constrained_by_compile_time_assertion(
    masked_source: &str,
    probe_line: usize,
    const_name: Option<&str>,
) -> bool {
    let spans = assertion_spans(masked_source);
    if spans.iter().any(|span| {
        line_of(masked_source, span.0) <= probe_line && probe_line <= line_of(masked_source, span.1)
    }) {
        return true;
    }
    let Some(name) = const_name else {
        return false;
    };
    declaration_count(masked_source, name) == 1
        && spans
            .iter()
            .any(|span| mentions_word(&masked_source[span.0..span.1], name))
}

fn line_of(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

/// Byte ranges `[start, end)` of each item-level assertion: from its head to
/// the `;` that ends it at bracket depth zero.
fn assertion_spans(masked: &str) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if !is_ident_byte(bytes[at]) || (at > 0 && is_ident_byte(bytes[at - 1])) {
            at += 1;
            continue;
        }
        let word_end = masked[at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(masked.len(), |len| at + len);
        let word = &masked[at..word_end];
        let after = masked[word_end..].trim_start();
        let is_head = match word {
            "const" => after.starts_with('_') && is_underscore_const(after),
            "const_assert" | "const_assert_eq" | "const_assert_ne" => after.starts_with('!'),
            _ => false,
        };
        if is_head
            && let Some(end) = item_end(masked, word_end)
            && is_compile_time_assertion(&masked[at..end])
        {
            spans.push((at, end));
            at = end;
        } else {
            at = word_end;
        }
    }
    spans
}

/// `_: ..` after `const`, not `_NAME`.
fn is_underscore_const(after: &str) -> bool {
    after[1..].trim_start().starts_with(':')
}

fn item_end(masked: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, character) in masked[from..].char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            ';' if depth == 0 => return Some(from + offset + 1),
            _ => {}
        }
    }
    None
}

fn mentions_word(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(found) = text[start..].find(name) {
        let at = start + found;
        let end = at + name.len();
        start = end;
        if (at == 0 || !is_ident_byte(bytes[at - 1]))
            && !bytes.get(end).is_some_and(|b| is_ident_byte(*b))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::constrained_by_compile_time_assertion as guard;

    const SRC: &str = "pub const N: usize = 12;\nconst _: () = assert!(N <= 12);\npub const M: usize = 3;\npub fn n() -> usize { N }\n";

    #[test]
    fn same_file_assertion_naming_the_const_constrains_it() {
        assert!(guard(SRC, 1, Some("N")));
    }

    #[test]
    fn const_no_assertion_names_is_not_constrained() {
        assert!(!guard(SRC, 3, Some("M")));
        assert!(!guard(SRC, 4, None));
    }

    #[test]
    fn a_longer_name_is_not_a_mention() {
        let src = "pub const N: usize = 1;\nconst _: () = assert!(N_MAX <= 4);\n";
        assert!(!guard(src, 1, Some("N")));
    }

    #[test]
    fn a_name_declared_twice_is_not_linked() {
        let src = "const N: usize = 1;\nmod m { pub const N: usize = 2; }\nconst _: () = assert!(N <= 4);\n";
        assert!(!guard(src, 1, Some("N")));
    }

    #[test]
    fn every_line_of_a_multi_line_assertion_is_constrained() {
        let src = "pub const N: usize = 12;\nconst _: () = assert!(\n    N <= 12\n);\npub const M: usize = 3;\n";
        for line in 2..=4 {
            assert!(guard(src, line, None), "line {line}");
        }
        assert!(!guard(src, 5, Some("M")));
    }

    #[test]
    fn braced_block_assertions_and_static_assertions_count() {
        let block = "const K: usize = 2;\nconst _: () = {\n    assert!(K > 0);\n};\n";
        assert!(guard(block, 1, Some("K")));
        assert!(guard(block, 3, None));
        let macro_form = "const K: usize = 2;\nstatic_assertions::const_assert!(K > 0);\n";
        assert!(guard(macro_form, 1, Some("K")));
    }

    #[test]
    fn a_non_assertion_underscore_const_is_not_a_guard() {
        let src = "const K: usize = 2;\nconst _: usize = K;\n";
        assert!(!guard(src, 1, Some("K")));
        assert!(!guard(src, 2, None));
        let named = "const _X: usize = 2;\nconst _X2: () = assert!(true);\n";
        assert!(!guard(named, 2, None));
    }
}
