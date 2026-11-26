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
/// - An owner that is a standard trait's own method (`impl Iterator for Foo
///   { fn next }`, `impl PartialEq for Foo { fn eq }`) keeps the relation:
///   another standard trait shares its name only for a type implementing
///   both, which this does not read.
/// - A call through the owner's trait path (`Peeky::into_iter(&foo)`,
///   `<Foo as crate::Peeky>::into_iter(&foo)`) names the owner's trait, so a
///   test with one keeps the relation.
/// - A method call whose receiver is written borrowed, `(&foo).into_iter()`
///   for a `&self` owner or `(&mut foo).into_iter()` for a `&mut self` owner,
///   starts probing at the owner's own step, where a standard method of the
///   name would be ambiguous. A test whose every method call of the name is
///   written so keeps the relation; a borrow bound earlier (`let r = &foo;`)
///   is not read and still withholds it.
pub(super) fn a_standard_trait_method_may_take_the_call(
    test: &TestSummary,
    owner: &FunctionSummary,
    owner_name: &str,
) -> bool {
    if !owner.item.has_self_param {
        return false;
    }
    let Some(kind) = receiver_kind(&owner.body) else {
        return false;
    };
    let earlier: &[&[(&str, &[&str])]] = match kind {
        ReceiverKind::Shared => &[BY_VALUE],
        ReceiverKind::Exclusive => &[BY_VALUE, BY_SHARED_REFERENCE],
        ReceiverKind::Value => return false,
    };
    let Some(owner_trait) = owner_trait_name(&owner.id.0) else {
        return false;
    };
    let name = owner_name.strip_prefix("r#").unwrap_or(owner_name);
    let owner_trait = owner_trait.strip_prefix("r#").unwrap_or(&owner_trait);
    // `impl PartialEq for Foo { fn eq(&self, ..) }` is a standard trait's own
    // method; `Iterator::eq(self, ..)` shares the name only for a type that
    // is an `Iterator` too, which this does not read.
    let lists = |table: &[(&str, &[&str])], rival: bool| {
        table.iter().any(|(standard, methods)| {
            (*standard != owner_trait) == rival && methods.contains(&name)
        })
    };
    let own_standard_method = [BY_VALUE, BY_SHARED_REFERENCE]
        .into_iter()
        .any(|table| lists(table, false));
    if own_standard_method || !earlier.iter().any(|table| lists(table, true)) {
        return false;
    }
    let masked = crate::analysis::extract::mask_comments_and_strings(&test.body);
    !calls_through_the_trait_path(&masked, name, owner_trait)
        && !every_method_call_borrows_its_receiver(&masked, name, kind)
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

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn plain(ident: &str) -> &str {
    ident.strip_prefix("r#").unwrap_or(ident)
}

/// The text before each call of `name` in the masked body: `name` as a whole
/// word (raw or not) followed by `(`, or by a method turbofish and `(`
/// (`name::<u8>(`).
fn calls_of<'a>(masked: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    masked.match_indices(name).filter_map(move |(at, _)| {
        let after = &masked[at + name.len()..];
        if after.starts_with(is_word) || !past_turbofish(after).starts_with('(') {
            return None;
        }
        let before = &masked[..at];
        let before = before.strip_suffix("r#").unwrap_or(before);
        (!before.ends_with(is_word)).then_some(before)
    })
}

/// The text after a leading method turbofish (`::<u8>`), trimmed; `text`
/// trimmed when it has none or the turbofish does not close.
fn past_turbofish(text: &str) -> &str {
    let text = text.trim_start();
    let Some(args) = text
        .strip_prefix("::")
        .map(str::trim_start)
        .filter(|args| args.starts_with('<'))
    else {
        return text;
    };
    let mut depth = 0usize;
    let mut previous = None;
    for (index, ch) in args.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' if previous != Some('-') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return args[index + 1..].trim_start();
                }
            }
            _ => {}
        }
        previous = Some(ch);
    }
    text
}

/// Whether the masked body calls `name` through a path naming `owner_trait`:
/// `Trait::name(`, `path::Trait::name(`, `Trait::<U>::name(`,
/// `<T as Trait>::name(` or `<T as path::Trait<U>>::name(`, with any
/// whitespace between the tokens.
fn calls_through_the_trait_path(masked: &str, name: &str, owner_trait: &str) -> bool {
    let name = plain(name);
    let owner_trait = plain(owner_trait);
    calls_of(masked, name).any(|before| {
        before
            .trim_end()
            .strip_suffix("::")
            .is_some_and(|path| path_names_trait(path, owner_trait))
    })
}

/// Whether the path before a call's last `::` names `owner_trait`.
fn path_names_trait(path: &str, owner_trait: &str) -> bool {
    let path = path.trim_end();
    if !path.ends_with('>') {
        return last_segment_is(path, owner_trait);
    }
    let Some(open) = matching_open_angle(path) else {
        return false;
    };
    let head = path[..open].trim_end();
    if let Some(trait_path) = head.strip_suffix("::") {
        // `Trait::<U>::name(`
        return last_segment_is(trait_path, owner_trait);
    }
    if head.ends_with(is_word) {
        // `Type<Trait>::name(` names no trait.
        return false;
    }
    // `<T as path::Trait<U>>::name(`
    let inner = &path[open + 1..path.len() - 1];
    let Some(at) = inner
        .rmatch_indices("as")
        .map(|(at, _)| at)
        .find(|&at| !inner[..at].ends_with(is_word) && !inner[at + 2..].starts_with(is_word))
    else {
        return false;
    };
    let trait_path = inner[at + 2..].trim_end();
    let trait_path = match trait_path.ends_with('>') {
        true => match matching_open_angle(trait_path) {
            Some(open) => trait_path[..open].trim_end(),
            None => return false,
        },
        false => trait_path,
    };
    last_segment_is(trait_path, owner_trait)
}

/// Whether the path's last identifier, raw or not, is `owner_trait`.
fn last_segment_is(path: &str, owner_trait: &str) -> bool {
    let start = path
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_word(*ch))
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    let ident = &path[start..];
    !ident.is_empty() && plain(ident) == owner_trait
}

/// The byte index of the `<` matching the `>` that ends `text`, skipping the
/// `>` of `->`.
fn matching_open_angle(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut previous = None;
    for (index, ch) in text.char_indices().rev() {
        let arrow = ch == '-' && previous == Some('>');
        match ch {
            '>' => depth += 1,
            '-' if arrow => depth = depth.saturating_sub(1),
            '<' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        previous = Some(ch);
    }
    None
}

/// Whether the masked body makes at least one `.name(` method call and every
/// one has a parenthesized receiver written as a borrow of the owner's kind:
/// `(&foo)` for a `&self` owner, `(&mut foo)` for a `&mut self` owner.
fn every_method_call_borrows_its_receiver(masked: &str, name: &str, kind: ReceiverKind) -> bool {
    let name = plain(name);
    let mut receivers = calls_of(masked, name).filter_map(|before| {
        let before = before.trim_end();
        before
            .strip_suffix('.')
            .filter(|receiver| !receiver.ends_with('.'))
    });
    let mut any = false;
    let all = receivers.all(|receiver| {
        any = true;
        borrowed_receiver(receiver.trim_end(), kind)
    });
    any && all
}

/// Whether `receiver` ends in a parenthesized borrow of the owner's kind.
fn borrowed_receiver(receiver: &str, kind: ReceiverKind) -> bool {
    let Some(open) = matching_open_paren(receiver) else {
        return false;
    };
    let head = receiver[..open].trim_end();
    if head.ends_with(|ch: char| is_word(ch) || matches!(ch, '!' | ')' | '>' | ']' | '?')) {
        // `call(x).name(`, `mac!(x).name(`, `Foo::new::<u8>(&x).name(`: the
        // parentheses are a call's arguments, not the receiver.
        return false;
    }
    let Some(borrowed) = receiver[open + 1..receiver.len() - 1]
        .trim_start()
        .strip_prefix('&')
    else {
        return false;
    };
    let borrowed = borrowed.trim_start();
    let exclusive = borrowed
        .strip_prefix("mut")
        .is_some_and(|rest| !rest.starts_with(is_word));
    match kind {
        ReceiverKind::Shared => !exclusive && !borrowed.starts_with('&'),
        ReceiverKind::Exclusive => exclusive,
        ReceiverKind::Value => false,
    }
}

/// The byte index of the `(` matching the `)` that ends `text`.
fn matching_open_paren(text: &str) -> Option<usize> {
    if !text.ends_with(')') {
        return None;
    }
    let mut depth = 0usize;
    for (index, ch) in text.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;
