//! RIPR-SPEC-0228: a hand-written `PartialEq` can blind a whole-collection
//! comparison to the field the owner pushes.
//!
//! `assert_eq!(order.lines, expected.lines)` runs `Line::eq` for every element
//! (or the `eq` of a field type `Line` embeds). A derived `eq` compares every
//! field, so a changed pushed field fails the assertion. A hand-written `eq`
//! may ignore the field, compare it case-insensitively, or compare a key only,
//! and then the same mutant passes. The static reading cannot evaluate `eq`,
//! so the pushed-literal credit is withheld whenever the pushed element type,
//! or a type it embeds, has a hand-written `PartialEq`.
//!
//! The scan is textual and fails closed: a pushed literal whose type has no
//! `struct`/`enum` definition in the workspace (macro-generated, aliased,
//! `Self`, a foreign type that only shares a variant name) is treated as
//! lenient.

use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::RustIndex;
use std::collections::BTreeSet;

/// How many levels of embedded field types are followed.
const MAX_EMBEDDING_DEPTH: usize = 4;

/// Whether the element type of the pushed literal `literal` (`Line`,
/// `Event::Shipped`, `model::Line`) may compare leniently: it, or a type its
/// definition embeds, has a hand-written `PartialEq`, or the element type
/// cannot be resolved to a workspace definition.
pub(super) fn pushed_element_may_compare_leniently(literal: &str, index: &RustIndex) -> bool {
    let Some(element) = element_type_names(literal) else {
        return true;
    };
    let lenient = handwritten_partial_eq_types(index);
    let mut defined = false;
    let mut visited = BTreeSet::new();
    let mut frontier = vec![element.type_name.clone()];
    frontier.extend(element.variant);
    for depth in 0..=MAX_EMBEDDING_DEPTH {
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
                    // Only the element type itself proves the literal is a
                    // workspace type: a variant name can collide with an
                    // unrelated struct (`foreign::Event::Shipped` beside a
                    // local `struct Shipped`).
                    defined |= depth == 0 && name == element.type_name;
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

/// The element type a pushed literal builds, and its variant when the literal
/// is `Enum::Variant { .. }`.
struct ElementType {
    type_name: String,
    variant: Option<String>,
}

/// Names the type of the literal path: module segments (lowercase) are
/// skipped, `Self` is unresolvable, and two trailing type-like segments read
/// as `Enum::Variant`.
fn element_type_names(literal: &str) -> Option<ElementType> {
    let types: Vec<&str> = literal
        .split("::")
        .filter(|segment| !segment.is_empty())
        .filter(|segment| is_type_like(segment))
        .collect();
    if types.contains(&"Self") {
        return None;
    }
    let tail = types.len().saturating_sub(2);
    let types = &types[tail..];
    let type_name = (*types.first()?).to_string();
    Some(ElementType {
        type_name,
        variant: types.get(1).map(|variant| (*variant).to_string()),
    })
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
/// or `enum name { .. }` in masked `source`, past any generic parameters and
/// `where` clause (`struct Line<F: Fn() + PartialEq> { .. }`); `None` when it
/// declares neither (a unit struct embeds nothing and reads as empty).
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
            let after = skip_generics(after);
            let open = if after.trim_start().starts_with('(') {
                after.find('(')?
            } else {
                after.find(['{', ';'])?
            };
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

/// `text` past a leading balanced `<..>` generic parameter list, when there is
/// one (`->` inside a bound does not close it).
fn skip_generics(text: &str) -> &str {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('<') {
        return text;
    }
    let mut depth = 0usize;
    let mut previous = ' ';
    for (offset, ch) in trimmed.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' if previous != '-' => {
                depth -= 1;
                if depth == 0 {
                    return &trimmed[offset + 1..];
                }
            }
            _ => {}
        }
        previous = ch;
    }
    text
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
    fn item_body_skips_generic_bounds_and_where_clauses() {
        let generic = "struct Line<F: Fn() + PartialEq> { sku: Sku, callback: F }";
        assert!(item_body(generic, "Line").is_some_and(|body| body.contains("Sku")));
        let arrow = "struct Line<F: Fn() -> u8> { sku: Sku, callback: F }";
        assert!(item_body(arrow, "Line").is_some_and(|body| body.contains("Sku")));
        let bounded = "struct Line<F> where F: Fn() { sku: Sku, callback: F }";
        assert!(item_body(bounded, "Line").is_some_and(|body| body.contains("Sku")));
        let tuple = "struct Pair<T>(Sku, T);";
        assert!(item_body(tuple, "Pair").is_some_and(|body| body.contains("Sku")));
    }

    #[test]
    fn item_body_keeps_to_whole_names() {
        assert_eq!(item_body("struct LineItem { a: Sku }", "Line"), None);
        assert_eq!(
            item_body("pub(crate) struct Line { a: u8 }", "Line"),
            Some(" a: u8 ")
        );
    }

    #[test]
    fn element_type_names_separate_the_type_from_its_variant() {
        let names = |literal: &str| {
            element_type_names(literal).map(|element| (element.type_name, element.variant))
        };
        assert_eq!(names("Line"), Some(("Line".to_string(), None)));
        assert_eq!(names("model::Line"), Some(("Line".to_string(), None)));
        assert_eq!(
            names("Event::Shipped"),
            Some(("Event".to_string(), Some("Shipped".to_string())))
        );
        assert_eq!(
            names("foreign::Event::Shipped"),
            Some(("Event".to_string(), Some("Shipped".to_string())))
        );
        assert_eq!(names("Self"), None);
        assert_eq!(names("Self::Shipped"), None);
        assert!(names("lower::path").is_none());
    }
}
