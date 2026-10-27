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
//! lenient. So is an embedded field type that has no workspace definition and
//! is not a known std/core type: its `PartialEq` may live in a dependency the
//! index does not read (#10684).

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
            let mut found = false;
            for facts in index.files().values() {
                let raw = facts.data().source.as_ref();
                if !raw.contains(&format!("struct {name}"))
                    && !raw.contains(&format!("enum {name}"))
                {
                    continue;
                }
                let masked = mask_comments_and_strings(raw);
                if let Some(item) = item_parts(&masked, &name) {
                    found = true;
                    // Only the element type itself proves the literal is a
                    // workspace type: a variant name can collide with an
                    // unrelated struct (`foreign::Event::Shipped` beside a
                    // local `struct Shipped`).
                    defined |= depth == 0 && name == element.type_name;
                    // The item's own generic parameters are not field types.
                    // Each variant name accounts for one occurrence, its own
                    // declaration; a further occurrence is a field type to
                    // follow (`enum Shape { Circle(Circle) }`).
                    let generics: BTreeSet<String> =
                        generic_parameters(item.generics).into_iter().collect();
                    let mut pending_variants: BTreeSet<String> = if item.is_enum {
                        variant_names(item.body).into_iter().collect()
                    } else {
                        BTreeSet::new()
                    };
                    next.extend(identifiers(item.body).filter(|ident| {
                        is_type_like(ident)
                            && !generics.contains(ident)
                            && !pending_variants.remove(ident)
                    }));
                }
            }
            // An embedded type that no workspace file defines and std does
            // not own may carry a hand-written `eq` the index cannot read.
            if depth > 0 && !found && !is_known_std_type(&name) {
                return true;
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

/// One `struct`/`enum` definition read from masked source.
struct ItemParts<'a> {
    /// The `<..>` generic parameter list, empty when there is none.
    generics: &'a str,
    /// The text between the item's delimiters.
    body: &'a str,
    is_enum: bool,
}

/// The text between the delimiters of `struct name { .. }`, `struct name(..)`
/// or `enum name { .. }` in masked `source`, past any generic parameters and
/// `where` clause (`struct Line<F: Fn() + PartialEq> { .. }`); `None` when it
/// declares neither (a unit struct embeds nothing and reads as empty).
#[cfg(test)]
fn item_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    item_parts(source, name).map(|item| item.body)
}

fn item_parts<'a>(source: &'a str, name: &str) -> Option<ItemParts<'a>> {
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
            let after_generics = skip_generics(after);
            let generics = &after[..after.len() - after_generics.len()];
            let after = after_generics;
            let is_enum = *keyword == "enum";
            let open = if after.trim_start().starts_with('(') {
                after.find('(')?
            } else {
                after.find(['{', ';'])?
            };
            let (open_ch, close_ch) = match after.as_bytes()[open] {
                b'{' => ('{', '}'),
                b'(' => ('(', ')'),
                _ => {
                    return Some(ItemParts {
                        generics,
                        body: "",
                        is_enum,
                    });
                }
            };
            let mut depth = 0usize;
            for (offset, ch) in after[open..].char_indices() {
                if ch == open_ch {
                    depth += 1;
                } else if ch == close_ch {
                    depth -= 1;
                    if depth == 0 {
                        return Some(ItemParts {
                            generics,
                            body: &after[open + 1..open + offset],
                            is_enum,
                        });
                    }
                }
            }
            None
        })
    })
}

/// The type parameter names a `<..>` list declares: each identifier that
/// opens a top-level parameter (`T`, `F: Fn()`), not a lifetime or a bound.
fn generic_parameters(generics: &str) -> Vec<String> {
    let inner = generics.trim().trim_start_matches('<');
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut segment_start = true;
    let mut word = String::new();
    let mut previous = ' ';
    for ch in inner.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if segment_start {
                word.push(ch);
            }
        } else {
            if !word.is_empty() {
                // `const N: usize` declares `N`; the keyword itself is not a name.
                if word == "const" {
                    word.clear();
                } else {
                    names.push(std::mem::take(&mut word));
                    segment_start = false;
                }
            }
            match ch {
                '<' | '(' | '[' => depth += 1,
                '>' if previous != '-' => depth = depth.saturating_sub(1),
                ')' | ']' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => segment_start = true,
                _ => {}
            }
        }
        previous = ch;
    }
    if !word.is_empty() && word != "const" {
        names.push(word);
    }
    names
}

/// The variant names of an enum body: the first identifier of each top-level
/// comma-separated entry, past any `#[..]` attribute.
fn variant_names(body: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut entry = String::new();
    let mut flush = |entry: &mut String| {
        let text = entry.trim();
        let text = if text.starts_with("#[") {
            text.split_once(']')
                .map_or("", |(_, rest)| rest.trim_start())
        } else {
            text
        };
        if let Some(name) = identifiers(text).next() {
            names.push(name);
        }
        entry.clear();
    };
    for ch in body.chars() {
        match ch {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                flush(&mut entry);
                continue;
            }
            _ => {}
        }
        entry.push(ch);
    }
    flush(&mut entry);
    names
}

/// A std/core/alloc type whose `PartialEq` compares what it holds, so only
/// its type arguments can carry a lenient `eq`.
fn is_known_std_type(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "String"
            | "Vec"
            | "VecDeque"
            | "LinkedList"
            | "Option"
            | "Result"
            | "Box"
            | "Rc"
            | "Arc"
            | "Cell"
            | "RefCell"
            | "Cow"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "BinaryHeap"
            | "PhantomData"
            | "Duration"
            | "Instant"
            | "SystemTime"
            | "PathBuf"
            | "Path"
            | "OsString"
            | "Ordering"
            | "Reverse"
            | "Wrapping"
            | "Range"
            | "RangeInclusive"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
    )
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

    fn index_with(source: &str) -> RustIndex {
        let mut index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex::default());
        index.insert_file_only(
            std::path::PathBuf::from("src/lib.rs"),
            crate::analysis::facts::FileFacts {
                source: source.into(),
                ..Default::default()
            },
        );
        index
    }

    /// #10684: an embedded type no workspace file defines may carry an `eq`
    /// the index cannot read; std types and workspace types do not.
    #[test]
    fn an_embedded_type_from_outside_the_workspace_may_compare_leniently() {
        let lenient =
            |source: &str| pushed_element_may_compare_leniently("Line", &index_with(source));
        // The control: every embedded type is a workspace type or std.
        assert!(!lenient(
            "pub struct Sku(pub String);\npub struct Line { sku: Sku, qty: u32 }\n"
        ));
        assert!(!lenient(
            "pub struct Line { sku: Option<String>, tags: Vec<String>, qty: u32 }\n"
        ));
        assert!(!lenient("pub struct Line<T> { sku: String, extra: T }\n"));
        assert!(!lenient(
            "pub struct Sku(String);\npub enum Kind { Plain, Tagged { sku: Sku } }\npub struct Line { kind: Kind }\n"
        ));
        // A type the workspace does not define, bare or qualified, nested in
        // std containers, or reached through a workspace type, is lenient.
        assert!(lenient(
            "use outside::Sku;\npub struct Line { sku: Sku, qty: u32 }\n"
        ));
        assert!(lenient("pub struct Line { sku: outside::Sku }\n"));
        assert!(lenient(
            "pub struct Line { skus: Vec<Option<outside::Sku>> }\n"
        ));
        assert!(lenient(
            "pub struct Inner { sku: outside::Sku }\npub struct Line { inner: Inner }\n"
        ));
        assert!(lenient(
            "pub enum Kind { Tagged { sku: outside::Sku } }\npub struct Line { kind: Kind }\n"
        ));
        // A variant that shares its name with its payload type still follows
        // the payload.
        assert!(lenient(
            "pub struct Circle { r: outside::R }\npub enum Shape { Circle(Circle) }\npub struct Line { shape: Shape }\n"
        ));
        assert!(!lenient(
            "pub struct Circle { r: u32 }\npub enum Shape { Circle(Circle) }\npub struct Line { shape: Shape }\n"
        ));
        // A trait object cannot derive `PartialEq`; it stays unresolved.
        assert!(lenient("pub struct Line { sku: Box<dyn Hashed> }\n"));
    }

    #[test]
    fn generic_parameters_and_variant_names_are_read_from_the_declaration() {
        assert_eq!(generic_parameters("<T, F: Fn() -> u8>"), ["T", "F"]);
        assert_eq!(
            generic_parameters("<'a, T: Clone + PartialEq, const N: usize>"),
            ["a", "T", "N"]
        );
        assert!(generic_parameters("").is_empty());
        assert_eq!(
            variant_names(" A { s: Sku }, B, #[serde(skip)] C(Sku, u8) "),
            ["A", "B", "C"]
        );
    }
}
