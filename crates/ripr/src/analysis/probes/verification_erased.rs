//! Verification-erased source (RIPR-SPEC-0368).
//!
//! Some Rust text exists only for an external verifier. Contract attributes
//! (`#[requires]`, `#[ensures]`, `#[kani::requires]`, ...) state a claim a
//! test never evaluates, and Creusot/Prusti ghost items (`#[logic]` functions,
//! `proof_assert!`, `snapshot!`, `ghost!`, `pearlite!`, `prusti_assert!`,
//! `body_invariant!`) are erased from an ordinary build, so no test can reach
//! or observe them. A changed line of this kind is not a runtime behavior to
//! probe, and "add a co-located test" would be a wrong repair signal for it.
//!
//! This module only recognises such lines. Runtime code that merely carries a
//! contract attribute (`#[check(ghost)]`, `#[trusted]`, Prusti `#[pure]`) is
//! not matched and stays ordinary. Matching is lexical and fail-closed: a line
//! that also holds runtime tokens (`#[requires(x)] fn f() {`, a ghost macro in
//! a `let` initialiser) is never classified here.

use super::diff::macro_invocation_extents;
use crate::analysis::diff::ChangedFile;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{RustIndex, find_file_facts};
use std::collections::BTreeSet;

/// Specification attributes distinctive enough to trust bare. The runtime
/// `contracts` crate shares these names, so a file that names it is excluded.
const CONTRACT_ATTRIBUTES: &[&str] = &["requires", "ensures", "modifies", "decreases"];

/// Generic words that user code routinely defines as its own attributes; bare,
/// they count only when the file names a verifier crate.
const GATED_CONTRACT_ATTRIBUTES: &[&str] = &["invariant", "variant", "spec"];

/// Items a Creusot build erases outside verification (needs a Creusot path).
const GHOST_ITEM_ATTRIBUTES: &[&str] = &["logic"];

/// Verifier crates an attribute may be spelled through explicitly. Any other
/// prefix (including the runtime `contracts` crate, whose attributes expand to
/// `debug_assert!` a test can observe) names a different macro and is left
/// alone.
const ATTRIBUTE_PREFIXES: &[&str] = &[
    "kani",
    "prusti_contracts",
    "creusot_contracts",
    "creusot_std",
    "flux_rs",
];

/// Which verifier crates a file actually names in code (comments and strings
/// are masked), and whether it names the runtime `contracts` crate.
struct Verifiers {
    creusot: bool,
    any: bool,
    runtime_contracts: bool,
}

impl Verifiers {
    fn of(masked: &str) -> Self {
        let creusot =
            names_crate(masked, "creusot_contracts") || names_crate(masked, "creusot_std");
        let any = creusot
            || names_crate(masked, "prusti_contracts")
            || names_crate(masked, "kani")
            || names_crate(masked, "flux_rs");
        Self {
            creusot,
            any,
            runtime_contracts: names_crate(masked, "contracts"),
        }
    }
}

/// True when `name` appears as a whole identifier in `masked`.
fn names_crate(masked: &str, name: &str) -> bool {
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    masked.match_indices(name).any(|(at, _)| {
        let before = at.checked_sub(1).map(|i| masked.as_bytes()[i]);
        let after = masked.as_bytes().get(at + name.len()).copied();
        !before.is_some_and(ident) && !after.is_some_and(ident)
    })
}

/// Statement macros whose expansion is empty or dead in an ordinary build.
const GHOST_MACROS: &[&str] = &[
    "proof_assert",
    "pearlite",
    "prusti_assert",
    "prusti_assert_eq",
    "prusti_assert_ne",
    "prusti_assume",
    "body_invariant",
];

/// Names too generic to trust without a Creusot import in the same file.
const CREUSOT_GATED_MACROS: &[&str] = &["snapshot", "ghost"];

/// 1-based lines of `source` that are verification-erased text.
pub(super) fn verification_only_lines(source: &str) -> BTreeSet<usize> {
    let masked = mask_comments_and_strings(source);
    let bytes = masked.as_bytes();
    let mut line_starts = vec![0usize];
    line_starts.extend(
        bytes
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'\n')
            .map(|(at, _)| at + 1),
    );
    let line_of = |at: usize| line_starts.partition_point(|&start| start <= at);
    let line_end = |at: usize| masked[at..].find('\n').map_or(masked.len(), |n| at + n);
    let mut lines = BTreeSet::new();
    let verifiers = Verifiers::of(&masked);

    for &start in &line_starts {
        let Some(offset) = masked[start..]
            .char_indices()
            .find(|(_, ch)| !ch.is_whitespace() || *ch == '\n')
        else {
            continue;
        };
        let at = start + offset.0;
        if !masked[at..].starts_with("#[") {
            continue;
        }
        let Some(close) = closing_bracket(bytes, at + 1) else {
            continue;
        };
        let Some(name) = attribute_name(&masked[at + 2..close]) else {
            continue;
        };
        if is_contract(&name, &verifiers) {
            // A runtime token after the `]` keeps the whole line ordinary.
            if masked[close + 1..line_end(close)].trim().is_empty() {
                lines.extend(line_of(at)..=line_of(close));
            }
        } else if is_ghost_item(&name, &verifiers)
            && let Some(end) = item_end(bytes, close + 1)
        {
            lines.extend(line_of(at)..=line_of(end));
        }
    }

    let creusot_file = verifiers.creusot;
    let names = GHOST_MACROS
        .iter()
        .chain(CREUSOT_GATED_MACROS.iter().filter(|_| creusot_file))
        .copied()
        .collect::<Vec<_>>();
    for (at, end) in macro_invocation_extents(&masked, &names) {
        if end >= masked.len() {
            continue;
        }
        let line_start = line_starts[line_of(at) - 1];
        let leading_blank = masked[line_start..at].trim().is_empty();
        let trailing = masked[end + 1..line_end(end)].trim();
        if leading_blank && matches!(trailing, "" | ";") {
            lines.extend(line_of(at)..=line_of(end));
        }
    }
    lines
}

/// Count of `changed` lines (added, on the new side; removed, on the old side)
/// that are verification-erased text.
pub(crate) fn changed_verification_only_lines(index: &RustIndex, changed: &ChangedFile) -> usize {
    let added = find_file_facts(index, &changed.path)
        .map(|facts| verification_only_lines(&facts.source))
        .unwrap_or_default();
    let removed = super::classify::old_source(index, changed)
        .map(|old| verification_only_lines(&old))
        .unwrap_or_default();
    changed
        .added_lines
        .iter()
        .filter(|line| added.contains(&line.new_side_line))
        .count()
        + changed
            .removed_lines
            .iter()
            .filter(|line| removed.contains(&line.line))
            .count()
}

/// Byte offset of the `]` matching the `[` at `open`.
fn closing_bracket(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The whitespace-free attribute path of `#[path(...)]` / `#[path = ..]`.
fn attribute_name(body: &str) -> Option<String> {
    let path = body
        .trim_start()
        .split(['(', '=', '[', '{'])
        .next()?
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    (!path.is_empty()).then_some(path)
}

/// Split `path` into its crate prefix (empty when bare) and attribute name.
/// `None` when the prefix is not a recognised verifier crate.
fn split_path(path: &str) -> Option<(&str, &str)> {
    let (prefix, name) = path.rsplit_once("::").unwrap_or(("", path));
    (prefix.is_empty() || ATTRIBUTE_PREFIXES.contains(&prefix)).then_some((prefix, name))
}

fn is_contract(path: &str, verifiers: &Verifiers) -> bool {
    let Some((prefix, name)) = split_path(path) else {
        return false;
    };
    if !prefix.is_empty() {
        return CONTRACT_ATTRIBUTES.contains(&name) || GATED_CONTRACT_ATTRIBUTES.contains(&name);
    }
    (CONTRACT_ATTRIBUTES.contains(&name) && !verifiers.runtime_contracts)
        || (GATED_CONTRACT_ATTRIBUTES.contains(&name)
            && verifiers.any
            && !verifiers.runtime_contracts)
}

fn is_ghost_item(path: &str, verifiers: &Verifiers) -> bool {
    let Some((prefix, name)) = split_path(path) else {
        return false;
    };
    GHOST_ITEM_ATTRIBUTES.contains(&name) && (!prefix.is_empty() || verifiers.creusot)
}

/// Byte offset of the last byte of the item that starts at `from`: the `;` of
/// a bodyless item or the `}` closing its body. Attributes between the
/// annotation and the item are skipped over as part of the scan.
fn item_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes[from..].iter().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => return Some(from + offset),
            b'{' if depth == 0 => {
                let mut braces = 0usize;
                for (inner, byte) in bytes[from + offset..].iter().enumerate() {
                    match byte {
                        b'{' => braces += 1,
                        b'}' => {
                            braces = braces.saturating_sub(1);
                            if braces == 0 {
                                return Some(from + offset + inner);
                            }
                        }
                        _ => {}
                    }
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(source: &str) -> Vec<usize> {
        verification_only_lines(source).into_iter().collect()
    }

    #[test]
    fn contract_attributes_span_their_lines_and_only_theirs() {
        assert_eq!(lines("#[requires(x > 0)]\nfn f(x: i32) {}\n"), [1]);
        assert_eq!(
            lines("#[ensures(\n    result > 0\n)]\nfn f() -> i32 { 1 }\n"),
            [1, 2, 3]
        );
        assert_eq!(lines("#[kani::requires(x > 0)]\nfn f(x: i32) {}\n"), [1]);
        assert_eq!(
            lines("use creusot_std::prelude::*;\n#[variant(n - i)]\nfn f() {}\n"),
            [2]
        );
    }

    #[test]
    fn runtime_items_that_carry_verifier_words_stay_ordinary() {
        for source in [
            "#[check(ghost)]\nfn f() -> i32 { 1 }\n",
            "#[trusted]\nfn f() -> i32 { 1 }\n",
            "#[pure]\nfn f() -> i32 { 1 }\n",
            "#[requires(x)] fn f() { g(); }\n",
            "#[other::requires(x)]\nfn f() {}\n",
            "#[contracts::requires(x)]\nfn f() {}\n",
            "use contracts::*;\n#[requires(x > 0)]\nfn f() {}\n",
            "#[logic]\nfn f() -> i32 { 1 }\n",
            "#[invariant(x)]\nfn f() {}\n",
            "#[spec]\nfn f() {}\n",
            "// use creusot_std::prelude::*;\n#[logic]\nfn f() -> i32 { 1 }\n",
            "#[variant(n)]\nfn f() {}\n",
            "#[test]\nfn t() { assert!(true); }\n",
        ] {
            assert!(lines(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn logic_items_are_erased_through_their_closing_brace() {
        let source = "use creusot_std::prelude::*;\nfn run() {}\n#[logic]\npub fn spec(x: i32) -> i32 {\n    if x > 3 { x } else { 0 }\n}\nfn after() {}\n";
        assert_eq!(lines(source), [3, 4, 5, 6]);
        assert_eq!(
            lines("#[creusot_contracts::logic(open)]\nfn p() -> bool { true }\n"),
            [1, 2]
        );
        assert_eq!(
            lines("use creusot_std::prelude::*;\n#[logic]\nfn decl() -> i32;\nfn run() {}\n"),
            [2, 3]
        );
    }

    #[test]
    fn ghost_macros_are_erased_only_in_statement_position() {
        assert_eq!(
            lines("fn f(x: i32) {\n    proof_assert!(x > 0);\n    g(x);\n}\n"),
            [2]
        );
        assert_eq!(
            lines("fn f() {\n    prusti_assert!(\n        a == b\n    );\n}\n"),
            [2, 3, 4]
        );
        // A runtime token on the same line keeps it ordinary.
        assert!(lines("fn f() {\n    let n = proof_assert!(true) + g();\n}\n").is_empty());
        // Generic names need a Creusot import in the file.
        assert!(lines("fn f() {\n    snapshot!(x);\n}\n").is_empty());
        assert_eq!(
            lines(
                "use creusot_std::prelude::*;\nfn f() {\n    snapshot!(x);\n    ghost!{ y };\n}\n"
            ),
            [3, 4]
        );
    }
}
