//! A standard library trait's method of the owner's name, on the text
//! receiver path (#16040).
//!
//! The index lists workspace methods only. `impl Peeky for Foo { fn
//! into_iter(&self) }` can be the only indexed `into_iter`, yet
//! `Foo::new(5).into_iter()` runs `IntoIterator::into_iter(self)` when `Foo`
//! is an `Iterator`: method probing tries the by-value receiver before the
//! `&` step that finds the owner. Whether a standard trait reaches the type
//! needs its whole impl set, including blanket impls, so a standard method
//! of the name at an earlier probing step than the owner's withholds the
//! relation. The typed receiver path reads every standard method of the name
//! (`trait_receiver::a_standard_trait_names_the_method`); this one, which
//! already granted the relation before #16040, reads only the probing steps
//! that can take the call.

use super::dispatch_target::{ReceiverKind, receiver_kind};
use super::{FunctionSummary, TestSummary, owner_trait_name};

/// Whether a standard trait's method of the owner's name may take the test's
/// method calls instead of this trait impl method.
///
/// Method probing tries the receiver by value, then `&`, then `&mut`. A
/// standard method found at the owner's own step would make a call with
/// both traits in scope ambiguous, so only one at an earlier step takes the
/// call: a by-value method before a `&self` owner, a by-value or `&self`
/// method before a `&mut self` owner. An owner taking `self` by value is
/// tried first and keeps the relation.
///
/// - The standard trait's own impl (`impl Iterator for Foo { fn next }`) is
///   not its own rival.
/// - A call through the owner's trait path (`Peeky::into_iter(&foo)`,
///   `<Foo as Peeky>::into_iter(&foo)`) names the owner's trait, so a test
///   with one keeps the relation.
pub(super) fn a_standard_trait_method_may_take_the_call(
    test: &TestSummary,
    owner: &FunctionSummary,
    owner_name: &str,
) -> bool {
    if !owner.item.has_self_param {
        return false;
    }
    let earlier: &[&[(&str, &[&str])]] = match receiver_kind(&owner.body) {
        Some(ReceiverKind::Shared) => &[BY_VALUE],
        Some(ReceiverKind::Exclusive) => &[BY_VALUE, BY_SHARED_REFERENCE],
        Some(ReceiverKind::Value) | None => return false,
    };
    let Some(owner_trait) = owner_trait_name(&owner.id.0) else {
        return false;
    };
    let name = owner_name.strip_prefix("r#").unwrap_or(owner_name);
    let owner_trait = owner_trait.strip_prefix("r#").unwrap_or(&owner_trait);
    earlier.iter().any(|table| {
        table
            .iter()
            .any(|(standard, methods)| *standard != owner_trait && methods.contains(&name))
    }) && !calls_through_the_trait_path(&test.body, name, owner_trait)
}

/// Standard trait methods taking `self` by value, by the trait's last path
/// segment. A workspace type reaches several through blanket impls (`Into`,
/// `TryInto`, `IntoIterator` for an `Iterator`), which the index never sees.
const BY_VALUE: &[(&str, &[&str])] = &[
    (
        "Iterator",
        &[
            "count",
            "last",
            "step_by",
            "chain",
            "zip",
            "intersperse",
            "intersperse_with",
            "map",
            "for_each",
            "filter",
            "filter_map",
            "enumerate",
            "peekable",
            "skip_while",
            "take_while",
            "map_while",
            "skip",
            "take",
            "scan",
            "flat_map",
            "flatten",
            "map_windows",
            "fuse",
            "inspect",
            "collect",
            "collect_into",
            "partition",
            "partition_in_place",
            "is_partitioned",
            "fold",
            "reduce",
            "max",
            "min",
            "max_by_key",
            "max_by",
            "min_by_key",
            "min_by",
            "rev",
            "unzip",
            "copied",
            "cloned",
            "cycle",
            "array_chunks",
            "sum",
            "product",
            "cmp",
            "cmp_by",
            "partial_cmp",
            "partial_cmp_by",
            "eq",
            "eq_by",
            "ne",
            "lt",
            "le",
            "gt",
            "ge",
            "is_sorted",
            "is_sorted_by",
            "is_sorted_by_key",
        ],
    ),
    ("DoubleEndedIterator", &["rfold"]),
    ("IntoIterator", &["into_iter"]),
    ("Into", &["into"]),
    ("TryInto", &["try_into"]),
    ("Ord", &["max", "min", "clamp"]),
    ("IntoFuture", &["into_future"]),
    ("FnOnce", &["call_once"]),
    ("Read", &["bytes", "chain", "take"]),
    ("BufRead", &["split", "lines"]),
    ("Add", &["add"]),
    ("Sub", &["sub"]),
    ("Mul", &["mul"]),
    ("Div", &["div"]),
    ("Rem", &["rem"]),
    ("Neg", &["neg"]),
    ("Not", &["not"]),
    ("BitAnd", &["bitand"]),
    ("BitOr", &["bitor"]),
    ("BitXor", &["bitxor"]),
    ("Shl", &["shl"]),
    ("Shr", &["shr"]),
];

/// Standard trait methods taking `&self`, by the trait's last path segment.
/// `Borrow`, `Any` and `ToString` (for a `Display` type) reach a workspace
/// type through blanket impls.
const BY_SHARED_REFERENCE: &[(&str, &[&str])] = &[
    ("Iterator", &["size_hint"]),
    ("ExactSizeIterator", &["len", "is_empty"]),
    ("ToOwned", &["to_owned", "clone_into"]),
    ("ToString", &["to_string"]),
    ("Clone", &["clone"]),
    ("PartialEq", &["eq", "ne"]),
    ("PartialOrd", &["partial_cmp", "lt", "le", "gt", "ge"]),
    ("Ord", &["cmp"]),
    ("Borrow", &["borrow"]),
    ("AsRef", &["as_ref"]),
    ("Deref", &["deref"]),
    ("Hash", &["hash"]),
    ("Hasher", &["finish"]),
    ("Any", &["type_id"]),
    ("Error", &["source", "description", "cause", "provide"]),
    ("Display", &["fmt"]),
    ("Debug", &["fmt"]),
    ("Fn", &["call"]),
    ("ToSocketAddrs", &["to_socket_addrs"]),
    ("Write", &["is_write_vectored"]),
    ("Read", &["is_read_vectored"]),
    ("Index", &["index"]),
    ("RangeBounds", &["start_bound", "end_bound", "contains"]),
    (
        "FileExt",
        &[
            "read_at",
            "read_vectored_at",
            "read_buf_at",
            "read_exact_at",
            "read_buf_exact_at",
            "write_at",
            "write_vectored_at",
            "write_all_at",
            "seek_read",
            "seek_write",
        ],
    ),
];

/// Whether the body calls `name` through a path ending in `owner_trait`:
/// `Trait::name(`, `path::Trait::name(` or `<T as Trait>::name(`, with any
/// whitespace between the tokens. Comments and literals do not count.
fn calls_through_the_trait_path(body: &str, name: &str, owner_trait: &str) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let plain = |ident: &str| ident.strip_prefix("r#").unwrap_or(ident).to_string();
    let name = plain(name);
    let owner_trait = plain(owner_trait);
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    masked.match_indices(name.as_str()).any(|(at, _)| {
        let after = &masked[at + name.len()..];
        if after.starts_with(word) || !after.trim_start().starts_with('(') {
            return false;
        }
        let before = &masked[..at];
        let before = before.strip_suffix("r#").unwrap_or(before);
        if before.ends_with(word) {
            return false;
        }
        let Some(path) = before.trim_end().strip_suffix("::") else {
            return false;
        };
        let path = path.trim_end();
        let (path, qualified) = match path.strip_suffix('>') {
            Some(inner) => (inner.trim_end(), true),
            None => (path, false),
        };
        let ident_start = path
            .char_indices()
            .rev()
            .find(|(_, ch)| !word(*ch))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let ident = &path[ident_start..];
        let rest = path[..ident_start]
            .strip_suffix("r#")
            .unwrap_or(&path[..ident_start]);
        if ident.is_empty() || plain(ident) != owner_trait {
            return false;
        }
        // `<T as Trait>::name(`: a bare `Type<Trait>::name(` names no trait.
        !qualified
            || rest
                .trim_end()
                .strip_suffix("as")
                .is_some_and(|head| !head.ends_with(word))
    })
}

#[cfg(test)]
mod tests;
