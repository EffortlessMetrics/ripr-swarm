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
use std::collections::{BTreeMap, BTreeSet};

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
    let mut shadowed = BTreeMap::new();
    let mut frontier = vec![element.type_name.clone()];
    frontier.extend(element.variant);
    for depth in 0..=MAX_EMBEDDING_DEPTH {
        let mut next = Vec::new();
        for path in frontier {
            // A workspace definition is found by its last segment; whether a
            // type with no definition is std depends on the whole path.
            let name = path.rsplit("::").next().unwrap_or("").to_string();
            if lenient.contains(&name) {
                return true;
            }
            if !visited.insert(path.clone()) {
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
                    let generics: BTreeSet<String> =
                        generic_parameters(item.generics).into_iter().collect();
                    next.extend(field_type_paths(&item).into_iter().filter(|field| {
                        field.contains("::") || !generics.contains(field.as_str())
                    }));
                }
            }
            // An embedded type that no workspace file defines and std does
            // not own may carry a hand-written `eq` the index cannot read.
            if depth > 0 && !found && !is_std_type_path(&path, index, &mut shadowed) {
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

/// The type paths an item's fields name, with attributes, array lengths,
/// enum variant names and discriminants left out: only a field's type can
/// carry a `PartialEq` that is not derived.
fn field_type_paths(item: &ItemParts<'_>) -> Vec<String> {
    let body = strip_array_lengths(&strip_attributes(item.body));
    if !item.is_enum {
        return type_paths(&body);
    }
    entries(&body)
        .iter()
        .flat_map(|entry| {
            let entry = entry.trim_start();
            // The variant name is the entry's first identifier.
            let payload = entry
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .map_or("", |end| &entry[end..]);
            // `= DISCRIMINANT` follows any payload, outside its delimiters.
            let mut depth = 0usize;
            let end = payload
                .char_indices()
                .find(|&(_, ch)| {
                    match ch {
                        '{' | '(' | '[' | '<' => depth += 1,
                        '}' | ')' | ']' | '>' => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    ch == '=' && depth == 0
                })
                .map_or(payload.len(), |(at, _)| at);
            type_paths(&payload[..end])
        })
        .collect()
}

/// The top-level comma-separated entries of an enum body.
fn entries(body: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut entry = String::new();
    for ch in body.chars() {
        match ch {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(std::mem::take(&mut entry));
                continue;
            }
            _ => {}
        }
        entry.push(ch);
    }
    entries.push(entry);
    entries
}

/// `text` without its `#[..]` and `#![..]` attributes, whose arguments are
/// not field types.
fn strip_attributes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '#' {
            out.push(ch);
            continue;
        }
        if chars.peek() == Some(&'!') {
            chars.next();
        }
        if chars.peek() != Some(&'[') {
            out.push(ch);
            continue;
        }
        let mut depth = 0usize;
        for inner in chars.by_ref() {
            match inner {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(' ');
    }
    out
}

/// `text` without the length expression of each array type (`[u8; SIZE]`
/// keeps `u8`): a constant is not a field type.
fn strip_array_lengths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // For each open `[`, whether its `;` has been seen.
    let mut brackets: Vec<bool> = Vec::new();
    for ch in text.chars() {
        match ch {
            '[' => brackets.push(false),
            ';' if brackets.last() == Some(&false) => {
                if let Some(last) = brackets.last_mut() {
                    *last = true;
                }
                continue;
            }
            ']' => {
                brackets.pop();
            }
            _ => {}
        }
        if brackets.last() != Some(&true) || ch == ']' {
            out.push(ch);
        }
    }
    out
}

/// The type-like paths in `text`: a chain of identifiers joined by `::`
/// whose last segment starts uppercase (`Sku`, `outside::Sku`).
fn type_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut path = String::new();
    let mut word = String::new();
    let flush = |path: &mut String, paths: &mut Vec<String>| {
        if path.rsplit("::").next().is_some_and(is_type_like) {
            paths.push(std::mem::take(path));
        }
        path.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
            continue;
        }
        if !word.is_empty() {
            if !path.is_empty() && !path.ends_with("::") {
                // A new identifier without a joining `::` starts a new path.
                flush(&mut path, &mut paths);
            }
            path.push_str(&word);
            word.clear();
        }
        if ch == ':' && chars.peek() == Some(&':') && !path.is_empty() {
            chars.next();
            path.push_str("::");
        } else {
            if path.ends_with("::") {
                path.truncate(path.len() - 2);
            }
            flush(&mut path, &mut paths);
        }
    }
    if !word.is_empty() {
        if !path.is_empty() && !path.ends_with("::") {
            flush(&mut path, &mut paths);
        }
        path.push_str(&word);
    }
    if path.ends_with("::") {
        path.truncate(path.len() - 2);
    }
    flush(&mut path, &mut paths);
    paths
}

/// Whether `path` names a std/core/alloc type: a qualified path must start at
/// one of those crates, and a bare name must not be imported from elsewhere
/// (`use outside::String;` makes `String` a foreign type).
fn is_std_type_path(path: &str, index: &RustIndex, shadowed: &mut BTreeMap<String, bool>) -> bool {
    let mut segments = path.split("::");
    let root = segments.next().unwrap_or("");
    let name = path.rsplit("::").next().unwrap_or("");
    if !is_known_std_type(name) {
        return false;
    }
    if path.contains("::") {
        return matches!(root, "std" | "core" | "alloc");
    }
    !*shadowed
        .entry(name.to_string())
        .or_insert_with(|| import_shadows(index, name))
}

/// Whether some workspace `use` brings a non-std `name` into scope, by name,
/// alias or a glob from a crate outside the workspace's own paths.
fn import_shadows(index: &RustIndex, name: &str) -> bool {
    index.files().values().any(|facts| {
        let raw = facts.data().source.as_ref();
        if !raw.contains("use ") {
            return false;
        }
        let masked = mask_comments_and_strings(raw);
        masked.match_indices("use ").any(|(at, _)| {
            let starts_word = masked[..at]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
            if !starts_word {
                return false;
            }
            let statement = masked[at + 4..].split(';').next().unwrap_or("");
            use_leaves(statement).iter().any(|leaf| {
                let root = leaf
                    .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                    .find(|word| !word.is_empty())
                    .unwrap_or("");
                let aliased_to_name = leaf
                    .rsplit_once(" as ")
                    .is_some_and(|(_, alias)| alias.trim() == name);
                if matches!(root, "std" | "core" | "alloc") {
                    return aliased_to_name;
                }
                let glob =
                    leaf.trim_end().ends_with('*') && !matches!(root, "crate" | "self" | "super");
                glob || identifiers(leaf).any(|word| word == name)
            })
        })
    })
}

/// The leaf paths of a `use` tree with each group prefix applied:
/// `std::{io, fmt::Display}` reads as `std::io` and `std::fmt::Display`, and a
/// rootless group (`{std::io, outside::String}`) splits into its own roots, so
/// one foreign leaf cannot hide behind a std sibling or a std prefix.
fn use_leaves(tree: &str) -> Vec<String> {
    fn walk(tree: &str, prefix: &str, out: &mut Vec<String>) {
        let tree = tree.trim();
        let Some(open) = tree.find('{') else {
            out.push(format!("{prefix}{}", tree.trim_matches(':')));
            return;
        };
        let head = &tree[..open];
        let close = tree.rfind('}').unwrap_or(tree.len());
        let inner = &tree[open + 1..close];
        let prefix = format!("{prefix}{head}");
        let (mut depth, mut start) = (0usize, 0usize);
        for (at, ch) in inner.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    walk(&inner[start..at], &prefix, out);
                    start = at + 1;
                }
                _ => {}
            }
        }
        walk(&inner[start..], &prefix, out);
    }
    let mut out = Vec::new();
    walk(tree, "", &mut out);
    out
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
    fn generic_parameters_are_read_from_the_declaration() {
        assert_eq!(generic_parameters("<T, F: Fn() -> u8>"), ["T", "F"]);
        assert_eq!(
            generic_parameters("<'a, T: Clone + PartialEq, const N: usize>"),
            ["a", "T", "N"]
        );
        assert!(generic_parameters("").is_empty());
    }

    fn field_paths(source: &str, name: &str) -> Vec<String> {
        item_parts(source, name)
            .map(|item| field_type_paths(&item))
            .unwrap_or_default()
    }

    #[test]
    fn field_type_paths_keep_qualified_paths_and_drop_non_types() {
        assert_eq!(
            field_paths(
                "struct L { a: outside::Sku, b: Vec<std::string::String>, c: u8 }",
                "L"
            ),
            ["outside::Sku", "Vec", "std::string::String"]
        );
        // Variant names, stacked attributes and discriminants are not types.
        assert_eq!(
            field_paths(
                "enum K { #[a] #[b(X)] A { s: Sku }, B = FIRST, C(Sku, u8) = 4, D }",
                "K"
            ),
            ["Sku", "Sku"]
        );
        // A variant that shares its payload's name still names the payload.
        assert_eq!(field_paths("enum S { Circle(Circle) }", "S"), ["Circle"]);
        // An array length is a constant, not a type.
        assert_eq!(
            field_paths("struct B { a: [u8; SIZE], b: [Sku; N] }", "B"),
            ["Sku"]
        );
    }

    /// #10684: the std allowlist applies to std paths, not to a familiar
    /// basename a foreign type happens to share.
    #[test]
    fn a_foreign_type_named_like_a_std_type_may_compare_leniently() {
        let lenient =
            |source: &str| pushed_element_may_compare_leniently("Line", &index_with(source));
        assert!(!lenient("pub struct Line { sku: String }\n"));
        assert!(!lenient("pub struct Line { sku: std::string::String }\n"));
        assert!(lenient("pub struct Line { sku: outside::String }\n"));
        assert!(lenient("pub struct Line { sku: Vec<outside::String> }\n"));
        assert!(lenient("pub struct Line { sku: crate_x::Option<u8> }\n"));
        assert!(lenient(
            "use outside::String;\npub struct Line { sku: String }\n"
        ));
        assert!(lenient(
            "use outside::{Sku as String, Other};\npub struct Line { sku: String }\n"
        ));
        assert!(lenient(
            "use outside::*;\npub struct Line { sku: String }\n"
        ));
        // A braced group is read by its names, so a std path beside a foreign
        // one cannot hide the foreign `String`.
        assert!(lenient(
            "use {std::io, outside::String};\npub struct Line { sku: String }\n"
        ));
        assert!(lenient(
            "use {std::io, outside::String};\nuse std::collections::{HashMap, HashSet};\npub struct Line { sku: String }\n"
        ));
        assert!(!lenient(
            "use std::collections::{HashMap, HashSet};\nuse std::{fmt, path::PathBuf};\npub struct Line { sku: String, m: HashMap<u8, u8>, p: PathBuf }\n"
        ));
        assert!(lenient(
            "use std::{fmt::Display as String};\npub struct Line { sku: String }\n"
        ));
        assert!(!lenient(
            "use std::{io, collections::BTreeMap};\npub struct Line { sku: String }\n"
        ));
        // Workspace-relative globs and std imports keep the std reading.
        assert!(!lenient(
            "use super::*;\nuse std::collections::BTreeMap;\npub struct Line { sku: String, m: BTreeMap<u8, u8> }\n"
        ));
    }

    /// #10684: constants and attributes inside an enum do not read as
    /// unknown field types.
    #[test]
    fn enum_discriminants_and_attributes_are_not_field_types() {
        let lenient =
            |source: &str| pushed_element_may_compare_leniently("Line", &index_with(source));
        assert!(!lenient(
            "pub enum Kind { A = FIRST, B = 2 }\npub struct Line { kind: Kind }\n"
        ));
        assert!(!lenient(
            "pub enum Kind { #[serde(skip)] #[doc(alias = ALIAS)] A, B }\npub struct Line { kind: Kind }\n"
        ));
        assert!(!lenient(
            "pub struct Line { buf: [u8; SIZE], name: String }\n"
        ));
        assert!(lenient(
            "pub enum Kind { #[a] #[b] A { s: outside::Sku } }\npub struct Line { kind: Kind }\n"
        ));
    }
}
