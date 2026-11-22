//! A trait impl method called on a receiver the index types.
//!
//! `impl BufMut for BytesMut { fn advance_mut }` shares its method name with
//! the trait declaration and every other impl, so a call only relates to it
//! once the receiver is known to be a `BytesMut`. The text receiver rule
//! knows three constructor names (`new`, `default`, `from`), so
//! `BytesMut::with_capacity(8).advance_mut(1)` and a binding built by any
//! other associated function stayed a name-only match. An inherent method of
//! the same type already resolves those receivers through the indexed
//! declaration of the constructor.
//!
//! This module lends that receiver authority to a trait impl method, and adds
//! the checks a trait method needs that an inherent one does not: with a
//! receiver of the impl's type, method-call syntax still runs something else
//! when the type has another method of that name, or when an impl the index
//! does not list could define one, or when a standard library trait has a
//! method of that name. Each of those keeps the name-only relation. Nothing here refuses a relation another rule grants.

use super::{
    FunctionContainer, FunctionSummary, RustIndex, TestSummary, compact_impl_type_name,
    impl_trait_name_from_path, inherent_receiver_call_resolves_with,
};

/// Whether `test` calls `owner`, a trait impl method taking `self`, on a
/// receiver the index types as the impl's self type, where that type has no
/// other method of the name that could take the call. `false` whenever any
/// part of that is not established.
pub(super) fn typed_receiver_runs_trait_impl(
    test: &TestSummary,
    owner: &FunctionSummary,
    impl_type: &str,
    index: &RustIndex,
    trust_unplaced_standard_root: bool,
) -> bool {
    let FunctionContainer::TraitImpl {
        trait_path,
        self_ty,
    } = &owner.item.container
    else {
        return false;
    };
    if !owner.item.has_self_param
        || !self_type_is_a_named_type(self_ty)
        || compact_impl_type_name(self_ty).as_deref() != Some(impl_type)
    {
        return false;
    }
    // The receiver is typed by the type's name alone, so the name must
    // declare one type.
    if super::super::owner_pin::workspace_type_name_count(impl_type, index) != 1 {
        return false;
    }
    let owner_trait = impl_trait_name_from_path(trait_path);
    if a_standard_trait_names_the_method(owner.name.as_str()) {
        return false;
    }
    if super::super::implicit_call::unindexed_impl_may_take_method_call(owner, owner_trait, index) {
        return false;
    }
    inherent_receiver_call_resolves_with(
        test,
        owner.name.as_str(),
        impl_type,
        Some(index),
        Some(owner),
        trust_unplaced_standard_root,
        None,
    ) && !another_method_may_take_the_call(owner, owner_trait, impl_type, index)
}

/// A self type written as a path (`Chain<T, U>`, `crate::Limit`). A
/// reference, pointer, slice, array, tuple or trait object is implemented
/// for another type than the one a constructor returns.
fn self_type_is_a_named_type(self_ty: &str) -> bool {
    let ty = self_ty.trim();
    !ty.is_empty()
        && !ty.starts_with(['&', '*', '[', '(', '<', '!'])
        && !["dyn ", "impl ", "fn(", "fn ", "unsafe ", "extern "]
            .iter()
            .any(|prefix| ty.starts_with(prefix))
}

/// Whether an indexed method named like `owner` could take a method call on
/// a value of `impl_type` instead of `owner`.
///
/// Another impl of the owner's own trait never does: one trait method
/// selects its impl by the receiver's type, and the declaration's default is
/// what the owner overrides. Every other same-named method is read by where
/// it sits. One on the same type takes or contests the call (an inherent
/// twin, another trait's method, a sibling instantiation). A default body of
/// another trait reaches the type through an impl block that lists no
/// method. A container the parser did not establish is not ruled out.
fn another_method_may_take_the_call(
    owner: &FunctionSummary,
    owner_trait: &str,
    impl_type: &str,
    index: &RustIndex,
) -> bool {
    let functions = index.functions();
    let name = owner.name.strip_prefix("r#").unwrap_or(&owner.name);
    let positions = index
        .call_identity_memos
        .function_positions_named(index, name);
    let rivals = || {
        positions
            .iter()
            .filter_map(|&position| functions.get(position))
            .filter(|function| function.id != owner.id)
    };
    // Two traits can share a last path segment (`Shape` and `other::Shape`).
    // The owner overrides one declaration of the name, so a second one under
    // the owner trait's name is another trait's method.
    let declared_under_the_owner_traits_name = rivals()
        .filter(|function| {
            matches!(&function.item.container, FunctionContainer::Trait { trait_name }
                if impl_trait_name_from_path(trait_name) == owner_trait)
        })
        .count();
    declared_under_the_owner_traits_name > 1
        || rivals().any(|function| match &function.item.container {
            FunctionContainer::Free | FunctionContainer::Local => false,
            FunctionContainer::Unknown => true,
            // Coherence allows the owner's trait one impl per type, so its
            // impl under another name is for another type, renamed or not.
            FunctionContainer::TraitImpl {
                trait_path,
                self_ty,
            } if impl_trait_name_from_path(trait_path) == owner_trait => {
                compact_impl_type_name(self_ty).is_none_or(|rival| rival == impl_type)
            }
            FunctionContainer::Inherent { self_ty }
            | FunctionContainer::TraitImpl { self_ty, .. } => {
                on_the_same_type(self_ty, impl_type, index)
            }
            FunctionContainer::Trait { trait_name } => {
                impl_trait_name_from_path(trait_name) != owner_trait && function.item.has_body
            }
        })
}

/// Whether a trait of the standard library has a method of this name.
///
/// The index lists workspace methods only. A standard trait reaches a
/// workspace type through an impl or a blanket impl (`Into`, `IntoIterator`
/// for an `Iterator`, `ToString` for a `Display`), and its provided methods
/// are written nowhere in the workspace. Where one takes the receiver at an
/// earlier probing step than the owner (`Iterator::count(self)` before a
/// `fn count(&self)`), the call runs it. Reading that needs the type's whole
/// impl set, so the name alone withholds the relation. An owner that is the
/// standard trait's own impl is withheld too: its trait is known here by its
/// last path segment, which a workspace trait can share. A dependency's
/// trait is not listed here and remains unread.
fn a_standard_trait_names_the_method(name: &str) -> bool {
    let name = name.strip_prefix("r#").unwrap_or(name);
    STANDARD_TRAIT_METHODS
        .iter()
        .any(|(_, methods)| methods.contains(&name))
}

/// Method names of standard library traits a workspace type commonly
/// implements or receives by a blanket impl, by the trait's last path
/// segment. Unstable methods are listed too: a name here only withholds.
const STANDARD_TRAIT_METHODS: &[(&str, &[&str])] = &[
    (
        "Iterator",
        &[
            "next",
            "next_chunk",
            "size_hint",
            "count",
            "last",
            "advance_by",
            "nth",
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
            "by_ref",
            "collect",
            "try_collect",
            "collect_into",
            "partition",
            "partition_in_place",
            "is_partitioned",
            "try_fold",
            "try_for_each",
            "fold",
            "reduce",
            "try_reduce",
            "all",
            "any",
            "find",
            "find_map",
            "try_find",
            "position",
            "rposition",
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
    (
        "DoubleEndedIterator",
        &[
            "next_back",
            "advance_back_by",
            "nth_back",
            "try_rfold",
            "rfold",
            "rfind",
        ],
    ),
    ("ExactSizeIterator", &["len", "is_empty"]),
    ("IntoIterator", &["into_iter"]),
    ("Into", &["into"]),
    ("TryInto", &["try_into"]),
    ("ToOwned", &["to_owned", "clone_into"]),
    ("ToString", &["to_string"]),
    ("Clone", &["clone", "clone_from"]),
    ("PartialEq", &["eq", "ne"]),
    ("PartialOrd", &["partial_cmp", "lt", "le", "gt", "ge"]),
    ("Ord", &["cmp", "max", "min", "clamp"]),
    ("Borrow", &["borrow"]),
    ("BorrowMut", &["borrow_mut"]),
    ("AsRef", &["as_ref"]),
    ("AsMut", &["as_mut"]),
    ("Deref", &["deref"]),
    ("DerefMut", &["deref_mut"]),
    ("Hash", &["hash", "hash_slice"]),
    (
        "Hasher",
        &[
            "finish",
            "write",
            "write_u8",
            "write_u16",
            "write_u32",
            "write_u64",
            "write_u128",
            "write_usize",
            "write_i8",
            "write_i16",
            "write_i32",
            "write_i64",
            "write_i128",
            "write_isize",
            "write_length_prefix",
            "write_str",
        ],
    ),
    ("Extend", &["extend", "extend_one", "extend_reserve"]),
    ("Any", &["type_id"]),
    ("Error", &["source", "description", "cause", "provide"]),
    ("Display", &["fmt"]),
    ("Debug", &["fmt"]),
    ("Future", &["poll"]),
    ("IntoFuture", &["into_future"]),
    ("Drop", &["drop"]),
    ("Fn", &["call"]),
    ("FnMut", &["call_mut"]),
    ("FnOnce", &["call_once"]),
    ("ToSocketAddrs", &["to_socket_addrs"]),
    // `fmt::Write` and `io::Write` share the last segment.
    (
        "Write",
        &[
            "write_str",
            "write_char",
            "write_fmt",
            "write",
            "write_vectored",
            "is_write_vectored",
            "flush",
            "write_all",
            "write_all_vectored",
            "by_ref",
        ],
    ),
    (
        "Read",
        &[
            "read",
            "read_vectored",
            "is_read_vectored",
            "read_to_end",
            "read_to_string",
            "read_exact",
            "read_buf",
            "read_buf_exact",
            "by_ref",
            "bytes",
            "chain",
            "take",
        ],
    ),
    (
        "Seek",
        &[
            "seek",
            "rewind",
            "stream_len",
            "stream_position",
            "seek_relative",
        ],
    ),
    (
        "BufRead",
        &[
            "fill_buf",
            "consume",
            "has_data_left",
            "read_until",
            "skip_until",
            "read_line",
            "split",
            "lines",
        ],
    ),
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
    ("AddAssign", &["add_assign"]),
    ("SubAssign", &["sub_assign"]),
    ("MulAssign", &["mul_assign"]),
    ("DivAssign", &["div_assign"]),
    ("RemAssign", &["rem_assign"]),
    ("BitAndAssign", &["bitand_assign"]),
    ("BitOrAssign", &["bitor_assign"]),
    ("BitXorAssign", &["bitxor_assign"]),
    ("ShlAssign", &["shl_assign"]),
    ("ShrAssign", &["shr_assign"]),
    ("Index", &["index"]),
    ("IndexMut", &["index_mut"]),
];

/// Whether a rival's self type is, or cannot be told from, `impl_type`. A
/// rival under another name is another type only when the workspace declares
/// a `struct`, `enum` or `union` of that name: an alias or a renamed import
/// (`type Tile = Square;`, `use crate::Square as Tile;`) names the same type.
fn on_the_same_type(self_ty: &str, impl_type: &str, index: &RustIndex) -> bool {
    compact_impl_type_name(self_ty).is_none_or(|rival| {
        rival == impl_type || !super::super::implicit_call::workspace_declares_type(&rival, index)
    })
}

#[cfg(test)]
mod tests;
