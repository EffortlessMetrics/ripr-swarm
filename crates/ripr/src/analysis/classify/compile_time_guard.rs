//! #12040: a changed line that a compile-time assertion constrains.
//!
//! A changed `const` has no owner function, so it reads `no_changed_rust_line`
//! even when an item-level assertion in the same file names it
//! (`const _: () = assert!(N <= 16);`): a violating edit fails to build, not a
//! test. A changed continuation line of a multi-line assertion has the same
//! shape. The class never rises above `static_unknown`: a ruled-out reach is
//! withheld as unknown by the classifier.
//! Fail closed on anything not spelled plainly: the assertion must be an item
//! in the probe's own file, and a name declared more than once is not linked.

use super::const_readers::{declaration_count, is_ident_byte, shadows};
use super::decision::is_compile_time_assertion;

/// Whether the changed line `probe_line` (1-based) of `masked_source` (comments
/// and strings already blanked, lines preserved) sits inside a compile-time
/// assertion item, or declares `const_name` that such an item names.
pub(in crate::analysis) fn constrained_by_compile_time_assertion(
    masked_source: &str,
    probe_line: usize,
    const_name: Option<&str>,
) -> bool {
    // A `macro_rules!` template may hold an assertion that is never built as
    // written; fail closed rather than guess.
    if masked_source.contains("macro_rules!") {
        return false;
    }
    let spans = assertion_spans(masked_source);
    if spans.iter().any(|span| {
        line_of(masked_source, span.0) <= probe_line && probe_line <= line_of(masked_source, span.1)
    }) {
        return true;
    }
    let Some(name) = const_name else {
        return false;
    };
    let declaration_offset = masked_source
        .split_inclusive('\n')
        .take(probe_line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    // A declaration inside a module may resolve differently from a same-named
    // import in a sibling scope; only top-level items are linked.
    let line_rest = &masked_source[declaration_offset.min(masked_source.len())..];
    let line = line_rest.lines().next().unwrap_or("");
    // Braces earlier on the declaration's own line (`mod m { const N: ..`) count.
    let at_name = declaration_offset + line.find(name).unwrap_or(0);
    brace_depth(masked_source, at_name) == 0
        && declaration_count(masked_source, name) == 1
        && spans
            .iter()
            .any(|span| mentions_word(&masked_source[span.0..span.1], name))
}

/// Open `{` blocks before `offset`; zero means a top-level item.
fn brace_depth(masked: &str, offset: usize) -> usize {
    let mut depth = 0usize;
    for byte in masked.as_bytes()[..offset].iter() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
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
            && brace_depth(masked, at) == 0
            && !cfg_gated(masked, at)
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

/// The attributes between the previous item and this one mention `cfg`
/// (single-line, multi-line or on the item's own line): the assertion may not
/// be built, so it is not credited as a guard.
fn cfg_gated(masked: &str, at: usize) -> bool {
    let head = &masked[..at];
    let start = head.rfind([';', '{', '}']).map_or(0, |n| n + 1);
    head[start..].contains("cfg")
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
        let before = text[..at].trim_end();
        let qualified = before.ends_with("::") || before.ends_with('.');
        if (at == 0 || !is_ident_byte(bytes[at - 1]))
            && !bytes.get(end).is_some_and(|b| is_ident_byte(*b))
            && !qualified
        {
            return !shadows(text, name);
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
    fn a_path_field_or_local_binding_is_not_a_mention() {
        for assertion in [
            "assert!(other::N <= 4)",
            "assert!(cfg.N <= 4)",
            "{ let N = 3; assert!(N > 0); }",
        ] {
            let src = format!("pub const N: usize = 1;\nconst _: () = {assertion};\n");
            assert!(!guard(&src, 1, Some("N")), "{assertion}");
        }
    }

    #[test]
    fn module_scoped_assertions_and_consts_are_not_linked() {
        let gated = "pub const N: usize = 1;\n#[cfg(test)]\nmod checks {\n    const _: () = assert!(N <= 4);\n}\n";
        assert!(!guard(gated, 1, Some("N")));
        assert!(!guard(gated, 4, None));
        let imported = "mod changed {\n    pub const N: usize = 2;\n}\nmod check {\n    use external::N;\n    const _: () = assert!(N < 4);\n}\n";
        assert!(!guard(imported, 2, Some("N")));
    }

    #[test]
    fn a_one_line_module_declaration_is_not_top_level() {
        let src = "mod m { pub const N: usize = 2; }\nconst _: () = assert!(N <= 4);\n";
        assert!(!guard(src, 1, Some("N")));
    }

    #[test]
    fn cfg_gated_and_macro_template_assertions_are_not_guards() {
        let gated =
            "pub const N: usize = 1;\n#[cfg(feature = \"x\")]\nconst _: () = assert!(N <= 4);\n";
        assert!(!guard(gated, 1, Some("N")));
        assert!(!guard(gated, 3, None));
        let multi = "pub const N: usize = 1;\n#[cfg(\n    feature = \"x\"\n)]\nconst _: () = assert!(N <= 4);\n";
        assert!(!guard(multi, 1, Some("N")));
        let same_line = "pub const N: usize = 1;\n#[cfg(x)] const _: () = assert!(N <= 4);\n";
        assert!(!guard(same_line, 1, Some("N")));
        let other_attr = "pub const N: usize = 1;\n#[inline]\nconst _: () = assert!(N <= 4);\n";
        assert!(guard(other_attr, 1, Some("N")));
        let template = "pub const N: usize = 1;\nmacro_rules! m { () => { const _: () = assert!(N <= 4); }; }\n";
        assert!(!guard(template, 1, Some("N")));
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
