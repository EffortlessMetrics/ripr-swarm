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
    // A generic trait (`Measure<u8>`) can be implemented for the type again
    // under other arguments, each with or without the method, and the call's
    // argument types select among them; the name-typed receiver reads none
    // of that.
    if a_standard_trait_names_the_method(owner.name.as_str())
        || generic_arguments(trait_path).is_some()
    {
        return false;
    }
    if super::super::implicit_call::unindexed_impl_may_take_method_call(owner, owner_trait, index) {
        return false;
    }
    if a_sibling_specialization_may_inherit_the_default(owner, owner_trait, self_ty, index)
        || the_type_derefs(impl_type, index)
        || a_borrowed_receiver_meets_a_by_value_owner(test, owner)
        || a_variant_shaped_path_names_no_declared_variant(test, impl_type, index)
    {
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

/// Whether the owner's impl is for one specialization of a generic type
/// (`impl Shape for Square<u32>`) while its trait has a default the index
/// can see, or a declaration it cannot. A sibling `impl Shape for
/// Square<u8> {}` lists no method, so no indexed row names it, yet
/// `Square::with_side(3u8).area()` runs the default. The receiver is typed
/// by the type's name alone and cannot tell the two apart. An impl whose
/// every generic argument is a lifetime or one of the impl's own type
/// parameters (`impl<T, U> BufMut for Chain<T, U>`) covers the whole type,
/// so coherence leaves it the only impl of the trait for that type. That
/// reading needs the owner's own impl header: distinct parameters with no
/// bound and no `where` clause. `impl<T> Shape for Pair<T, T>` leaves
/// `Pair<u8, u16>` to another impl, and a bound narrows the same way.
fn a_sibling_specialization_may_inherit_the_default(
    owner: &FunctionSummary,
    owner_trait: &str,
    self_ty: &str,
    index: &RustIndex,
) -> bool {
    let Some(arguments) = generic_arguments(self_ty) else {
        return false;
    };
    let covers_the_whole_type = index.files().get(&owner.file).is_some_and(|facts| {
        unbounded_impl_parameters(
            &facts.data().source,
            ImplSite {
                line: owner.start_line,
                function: &owner.name,
                trait_name: owner_trait,
                self_ty,
            },
        )
        .is_some_and(|parameters| {
            let mut seen = Vec::new();
            arguments.iter().all(|argument| {
                argument.starts_with('\'')
                    || (parameters.contains(argument) && !seen.contains(argument) && {
                        seen.push(argument.clone());
                        true
                    })
            })
        })
    });
    if covers_the_whole_type {
        return false;
    }
    let name = owner.name.strip_prefix("r#").unwrap_or(&owner.name);
    let mut declarations = index.functions().iter().filter(|function| {
        function.name.strip_prefix("r#").unwrap_or(&function.name) == name
            && matches!(&function.item.container, FunctionContainer::Trait { trait_name }
                if impl_trait_name_from_path(trait_name) == owner_trait)
    });
    match declarations.next() {
        None => true,
        Some(first) => first.item.has_body || declarations.any(|other| other.item.has_body),
    }
}

/// Where an owner method sits: its 1-based start line, its name, and the
/// trait and self type of the impl that holds it.
struct ImplSite<'a> {
    line: usize,
    function: &'a str,
    trait_name: &'a str,
    self_ty: &'a str,
}

/// The type parameters of the impl block that holds the owner at `site`,
/// when its header is `impl<..> Trait for self_ty` with every type parameter
/// a bare name: no bound, default or `const`, and no `where` clause.
/// Lifetimes are left out. The header is the nearest item-level `impl`
/// before the owner's `fn`, so a later impl on the same line is never read.
/// `None` whenever the header is not read that way, including a nearest
/// `impl` for another trait or self type.
fn unbounded_impl_parameters(source: &str, site: ImplSite<'_>) -> Option<Vec<String>> {
    let masked = super::mask_comments_and_strings(source);
    let line_start = masked
        .split_inclusive('\n')
        .take(site.line.checked_sub(1)?)
        .map(str::len)
        .sum::<usize>();
    // The owner's start line may hold its attributes, so its `fn` is the
    // first one of its name from that line on.
    let from_owner = masked.get(line_start..)?;
    let function = site.function.strip_prefix("r#").unwrap_or(site.function);
    let end = line_start
        + super::super::owner_pin::whole_word_offsets(from_owner, function)
            .into_iter()
            .find(|&at| {
                from_owner[..at]
                    .trim_end()
                    .strip_suffix("fn")
                    .is_some_and(|before| {
                        !before.ends_with(|ch: char| ch.is_alphanumeric() || ch == '_')
                    })
            })?;
    // An item-level `impl` starts a statement; `-> impl Iterator` and an
    // argument's `impl Trait` do not.
    let header_at = super::super::owner_pin::whole_word_offsets(masked.get(..end)?, "impl")
        .into_iter()
        .rev()
        .find(|&offset| {
            let before = masked[..offset].trim_end();
            before.is_empty()
                || before.ends_with(['}', ';', '{', ']'])
                || before.ends_with("unsafe")
                || before.ends_with("default")
        })?;
    let after = &masked[header_at + "impl".len()..];
    let header = &after[..after.find('{')?];
    let header = header.trim_start();
    let (generics, rest) = match header.strip_prefix('<') {
        Some(inner) => {
            let mut depth = 1usize;
            let bytes = inner.as_bytes();
            let close = (0..bytes.len()).find(|&at| match bytes[at] {
                b'<' => {
                    depth += 1;
                    false
                }
                b'>' if at > 0 && bytes[at - 1] == b'-' => false,
                b'>' => {
                    depth -= 1;
                    depth == 0
                }
                _ => false,
            })?;
            (&inner[..close], &inner[close + 1..])
        }
        None => ("", header),
    };
    let (trait_part, self_part) = rest.split_once(" for ")?;
    let trait_part = trait_part.trim();
    let trait_name = trait_part
        .split('<')
        .next()
        .unwrap_or(trait_part)
        .rsplit("::")
        .next()
        .unwrap_or(trait_part)
        .trim();
    if trait_name != site.trait_name {
        return None;
    }
    let squeeze = |text: &str| text.split_whitespace().collect::<String>();
    if !super::super::owner_pin::whole_word_offsets(self_part, "where").is_empty()
        || squeeze(self_part) != squeeze(site.self_ty)
    {
        return None;
    }
    let mut parameters = Vec::new();
    for parameter in generics.split(',').map(str::trim) {
        if parameter.is_empty() || parameter.starts_with('\'') {
            continue;
        }
        if !parameter
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_')
        {
            return None;
        }
        parameters.push(parameter.to_string());
    }
    Some(parameters)
}

/// Whether the test writes `impl_type::Name` with a capitalised `Name` that
/// no `enum impl_type` in the workspace declares as a variant. Such a path
/// is read as a variant constructor by the receiver rule, yet it may be an
/// associated function a trait provides that returns another type, or an
/// associated constant. A declared variant of the enum is the type itself.
fn a_variant_shaped_path_names_no_declared_variant(
    test: &TestSummary,
    impl_type: &str,
    index: &RustIndex,
) -> bool {
    let body = test.body.as_str();
    let prefix = format!("{impl_type}::");
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    body.match_indices(&prefix).any(|(at, _)| {
        if body[..at].ends_with(ident) {
            return false;
        }
        let rest = &body[at + prefix.len()..];
        let end = rest.find(|ch: char| !ident(ch)).unwrap_or(rest.len());
        let name = &rest[..end];
        name.starts_with(|ch: char| ch.is_uppercase())
            && !rest[end..].starts_with("::")
            && !an_enum_declares_the_variant(impl_type, name, index)
    })
}

/// Whether some `enum impl_type { .. }` in an indexed file lists `variant`.
fn an_enum_declares_the_variant(impl_type: &str, variant: &str, index: &RustIndex) -> bool {
    index.files().values().any(|facts| {
        let source = &facts.data().source;
        if !source.contains(impl_type) || !source.contains(variant) {
            return false;
        }
        let masked = super::mask_comments_and_strings(source);
        super::super::owner_pin::whole_word_offsets(&masked, "enum")
            .into_iter()
            .any(|offset| {
                let rest = masked[offset + "enum".len()..].trim_start();
                let Some(after_name) = rest.strip_prefix(impl_type) else {
                    return false;
                };
                if after_name.starts_with(|ch: char| ch.is_alphanumeric() || ch == '_') {
                    return false;
                }
                let Some(open) = after_name.find('{') else {
                    return false;
                };
                let bytes = after_name.as_bytes();
                let mut depth = 0usize;
                let close = (open..bytes.len()).find(|&at| match bytes[at] {
                    b'{' => {
                        depth += 1;
                        false
                    }
                    b'}' => {
                        depth -= 1;
                        depth == 0
                    }
                    _ => false,
                });
                close.is_some_and(|close| declares_a_variant(&after_name[open + 1..close], variant))
            })
    })
}

/// Whether the inside of an enum's braces declares `variant`: the name at
/// the top level, right after the opening brace, a comma or an attribute.
/// A field type (`Square(Make)`) or a discriminant expression is not a
/// declaration.
fn declares_a_variant(body: &str, variant: &str) -> bool {
    super::super::owner_pin::whole_word_offsets(body, variant)
        .into_iter()
        .any(|offset| {
            let bytes = body.as_bytes();
            let mut depth = 0usize;
            for at in 0..offset {
                match bytes[at] {
                    b'(' | b'[' | b'{' | b'<' => depth += 1,
                    b'>' if at > 0 && bytes[at - 1] == b'-' => {}
                    b')' | b']' | b'}' | b'>' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            let before = body[..offset].trim_end();
            depth == 0 && (before.is_empty() || before.ends_with([',', ']']))
        })
}

/// The top-level generic arguments of a self type (`Square<u32>` gives
/// `["u32"]`), or `None` when it has none.
fn generic_arguments(self_ty: &str) -> Option<Vec<String>> {
    let open = self_ty.find('<')?;
    let close = self_ty.rfind('>')?;
    let inner = self_ty.get(open + 1..close)?;
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (at, ch) in inner.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                arguments.push(inner[start..at].trim().to_string());
                start = at + 1;
            }
            _ => {}
        }
    }
    arguments.push(inner[start..].trim().to_string());
    arguments.retain(|argument| !argument.is_empty());
    (!arguments.is_empty()).then_some(arguments)
}

/// Whether the workspace implements `Deref` for the type. Without the
/// owner's trait in scope, method-call syntax keeps probing through the
/// dereference target, whose own method of the name then runs; this does
/// not read which traits the test has in scope. A `Deref` impl whose self
/// type name the workspace does not declare as one type may name the type
/// through an alias (`type View = Square; impl Deref for View`).
fn the_type_derefs(impl_type: &str, index: &RustIndex) -> bool {
    index.functions().iter().any(|function| {
        function.name == "deref"
            && matches!(&function.item.container, FunctionContainer::TraitImpl { trait_path, self_ty }
                if impl_trait_name_from_path(trait_path) == "Deref"
                    && on_the_same_type(self_ty, impl_type, index))
    })
}

/// Whether the owner takes `self` by value and some call of its name has a
/// receiver written as a borrow (`(&mut sq).area()`). Probing starts at the
/// borrow, where an impl of the trait for `&mut T` takes `self` by value
/// first, and such an impl may be written by a macro the index does not
/// list.
fn a_borrowed_receiver_meets_a_by_value_owner(test: &TestSummary, owner: &FunctionSummary) -> bool {
    if super::dispatch_target::receiver_kind(&owner.body)
        != Some(super::dispatch_target::ReceiverKind::Value)
    {
        return false;
    }
    let name = owner.name.strip_prefix("r#").unwrap_or(&owner.name);
    let body = test.body.as_str();
    let bytes = body.as_bytes();
    // `).name(`, with any whitespace or line break between the tokens.
    let closes = body.match_indices(name).filter_map(|(at, _)| {
        let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
        let before = &body[..at];
        let after = &body[at + name.len()..];
        if before.chars().next_back().is_some_and(word) || !after.trim_start().starts_with('(') {
            return None;
        }
        let before = before.trim_end().strip_suffix('.')?.trim_end();
        before.ends_with(')').then(|| before.len() - 1)
    });
    closes.into_iter().any(|close| {
        let mut depth = 0usize;
        let open = (0..=close).rev().find(|&at| match bytes[at] {
            b')' => {
                depth += 1;
                false
            }
            b'(' => {
                depth = depth.saturating_sub(1);
                depth == 0
            }
            _ => false,
        });
        open.is_some_and(|open| body[open + 1..close].trim_start().starts_with('&'))
    })
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
            // impl under another declared type's name is for another type. An
            // alias may name another instantiation of the owner's generic type
            // (`type Tile = Square<u16>; impl Measure for Tile`), which the
            // name-typed receiver cannot tell apart.
            FunctionContainer::TraitImpl {
                trait_path,
                self_ty,
            } if impl_trait_name_from_path(trait_path) == owner_trait => {
                compact_impl_type_name(self_ty).is_none_or(|rival| {
                    rival == impl_type
                        || super::super::implicit_call::workspace_renames_type(&rival, index)
                })
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
    ("RangeBounds", &["start_bound", "end_bound", "contains"]),
    // `std::os::unix::fs::FileExt` provides `read_exact_at` and
    // `write_all_at` over the two required methods; the Windows trait of the
    // same name has `seek_read` and `seek_write`.
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
    ("IndexMut", &["index_mut"]),
];

/// Whether a rival's self type is, or cannot be told from, `impl_type`. A
/// rival under another name is another type only when the workspace declares
/// a `struct`, `enum` or `union` of that name and no alias or renamed import
/// of it: `type Tile = Square;` or `use crate::Square as Tile;` names the same
/// type, also when some other module declares a `struct Tile`.
fn on_the_same_type(self_ty: &str, impl_type: &str, index: &RustIndex) -> bool {
    compact_impl_type_name(self_ty).is_none_or(|rival| {
        rival == impl_type || !super::super::implicit_call::workspace_declares_type(&rival, index)
    })
}

#[cfg(test)]
mod tests;
