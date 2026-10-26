//! RIPR-SPEC-0228: a hand-written `PartialEq` can blind a whole-collection
//! comparison to the field the owner pushes.
//!
//! `assert_eq!(order.lines, expected.lines)` runs `Line::eq` for every element
//! (or the `eq` of a field type `Line` embeds). A derived `eq` compares every
//! field, so a changed pushed field fails the assertion. A hand-written `eq`
//! may ignore the field, compare it case-insensitively, or compare only a key,
//! and then the same mutant passes. The static reading cannot evaluate `eq`,
//! so the pushed-literal credit is withheld whenever the pushed element type,
//! or a type it embeds, has a hand-written `PartialEq`.
//!
//! The scan is textual and fails closed: a pushed literal whose type has no
//! `struct`/`enum` definition in the workspace (macro-generated, aliased,
//! `Self`) is treated as lenient.

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::domain::Probe;
use std::collections::BTreeSet;

/// How many levels of embedded field types are followed.
const MAX_EMBEDDING_DEPTH: usize = 4;

/// Whether the element type `owner` pushes at `probe` may compare leniently:
/// it, or a type its definition embeds, has a hand-written `PartialEq`, or the
/// element type cannot be resolved to a definition.
pub(super) fn pushed_element_may_compare_leniently(
    owner: &FunctionSummary,
    probe: &Probe,
    index: &RustIndex,
) -> bool {
    let Some(candidates) = pushed_literal_type_names(owner, probe) else {
        return true;
    };
    let lenient = handwritten_partial_eq_types(index);
    let mut defined = false;
    let mut visited = BTreeSet::new();
    let mut frontier = candidates;
    for _ in 0..=MAX_EMBEDDING_DEPTH {
        let mut next = Vec::new();
        for name in frontier {
            if lenient.contains(&name) {
                return true;
            }
            if !visited.insert(name.clone()) {
                continue;
            }
            for facts in index.files().values() {
                let raw = facts.data().source.as_ref();
                if !raw.contains(&format!("struct {name}"))
                    && !raw.contains(&format!("enum {name}"))
                {
                    continue;
                }
                if let Some(body) = item_body(&mask_comments_and_strings(raw), &name) {
                    defined = true;
                    next.extend(identifiers(body).filter(|ident| is_type_like(ident)));
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    !defined
}

/// The one or two trailing path segments of the struct literal handed to the
/// `push(` that encloses `probe` (`Line` for `Line { .. }`, `Event`/`Shipped`
/// for `Event::Shipped { .. }`). `None` when no such literal is found.
fn pushed_literal_type_names(owner: &FunctionSummary, probe: &Probe) -> Option<Vec<String>> {
    let masked = mask_comments_and_strings(owner.body.as_str());
    let probe_line = probe.location.line.checked_sub(owner.start_line)?;
    let lines: Vec<&str> = masked.lines().collect();
    let upto = probe_line.min(lines.len().checked_sub(1)?);
    let (line_index, call_end) = (0..=upto).rev().find_map(|at| {
        lines[at]
            .rfind(".push(")
            .map(|call| (at, call + ".push(".len()))
    })?;
    let argument = lines[line_index].get(call_end..)?.trim_start();
    let path_len = argument
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .unwrap_or(argument.len());
    let path = &argument[..path_len];
    if path.is_empty() || !argument[path_len..].trim_start().starts_with('{') {
        return None;
    }
    let segments: Vec<&str> = path.split("::").filter(|part| !part.is_empty()).collect();
    if segments.iter().any(|segment| *segment == "Self") {
        return None;
    }
    let tail = segments.len().saturating_sub(2);
    Some(
        segments[tail..]
            .iter()
            .map(|segment| (*segment).to_string())
            .collect(),
    )
}

/// Self-type names of every hand-written `impl PartialEq .. for T` in the
/// workspace, whatever the trait's generic argument.
fn handwritten_partial_eq_types(index: &RustIndex) -> BTreeSet<String> {
    index
        .functions()
        .iter()
        .filter(|function| matches!(function.name.as_str(), "eq" | "ne"))
        .filter_map(|function| {
            let rest = function.id.0.split("::impl ").nth(1)?;
            let (trait_ty, self_ty) = rest.rsplit_once("::")?.0.rsplit_once(" for ")?;
            let trait_name = trait_ty.split('<').next()?.rsplit("::").next()?.trim();
            (trait_name == "PartialEq").then(|| {
                self_ty
                    .trim_start_matches(['&', ' '])
                    .split('<')
                    .next()
                    .and_then(|path| path.rsplit("::").next())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// The text between the delimiters of `struct name { .. }`, `struct name(..)`
/// or `enum name { .. }` in masked `source`; `None` when it declares neither
/// (or a unit struct, which embeds nothing).
fn item_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    ["struct", "enum"].iter().find_map(|keyword| {
        let needle = format!("{keyword} {name}");
        source.match_indices(&needle).find_map(|(at, _)| {
            let before_ok = source[..at]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
            let after = &source[at + needle.len()..];
            let after_ok = after
                .chars()
                .next()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
            if !before_ok || !after_ok {
                return None;
            }
            let open = after.find(['{', '(', ';'])?;
            let (open_ch, close_ch) = match after.as_bytes()[open] {
                b'{' => ('{', '}'),
                b'(' => ('(', ')'),
                _ => return Some(""),
            };
            let mut depth = 0usize;
            for (offset, ch) in after[open..].char_indices() {
                if ch == open_ch {
                    depth += 1;
                } else if ch == close_ch {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&after[open + 1..open + offset]);
                    }
                }
            }
            None
        })
    })
}

fn identifiers(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
}

fn is_type_like(ident: &str) -> bool {
    ident.chars().next().is_some_and(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_body_reads_braced_tuple_and_enum_definitions() {
        let source = "struct Line { sku: Sku, qty: u32 }\nstruct Pair(Sku, u8);\nenum Ev { A { s: Sku }, B }\nstruct Unit;\n";
        assert!(item_body(source, "Line").is_some_and(|body| body.contains("Sku")));
        assert!(item_body(source, "Pair").is_some_and(|body| body.contains("Sku")));
        assert!(item_body(source, "Ev").is_some_and(|body| body.contains("Sku")));
        assert_eq!(item_body(source, "Unit"), Some(""));
        assert_eq!(item_body(source, "Lin"), None);
        assert_eq!(item_body(source, "Missing"), None);
    }

    #[test]
    fn item_body_keeps_to_whole_names() {
        assert_eq!(item_body("struct LineItem { a: Sku }", "Line"), None);
        assert_eq!(
            item_body("pub(crate) struct Line { a: u8 }", "Line"),
            Some(" a: u8 ")
        );
    }
}
