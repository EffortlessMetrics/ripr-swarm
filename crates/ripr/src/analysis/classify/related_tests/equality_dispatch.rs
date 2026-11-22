//! `assert_eq!` / `assert_ne!` as a call of a user `PartialEq` method (#8999).
//!
//! `assert_eq!(a, b)` expands to `*a == *b`, which runs `PartialEq::eq` of
//! the operands' type without ever spelling `eq`. A test that compares two
//! values of a type whose `impl PartialEq for T { fn eq(..) }` changed
//! therefore calls the changed function, though no call fact names it.
//!
//! The reading is deliberately narrow and fails closed:
//!
//! - the owner is `eq` (or `ne`) of a non-generic `impl PartialEq for T`
//!   with a plain, non-generic self type `T`;
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `T`;
//! - the assertion is a real `assert_eq!`, `assert_ne!`, `debug_assert_eq!`
//!   or `debug_assert_ne!` (no user macro of that name in the file), and
//!   **both** compared operands have a syntactically resolvable type `T`
//!   (a `let` binding annotated or built by `T { .. }`, `T(..)`,
//!   `T::new(..)`-style constructors, an enum variant or a unit struct; any
//!   other expression is unresolved). Two operands of type `T` can only
//!   compare through `PartialEq<T> for T`, which is the owner's impl;
//! - `assert_ne!` reaches `eq` only through the default `ne`, so a custom
//!   `ne` in the same impl block withholds it, and an owner `ne` is reached
//!   by `assert_ne!` alone.
//!
//! This only relates the test to the owner. The assertion is still credited
//! or refused by the ordinary oracle pipeline; nothing here raises a class.

use super::{
    FunctionImplContext, FunctionSummary, RustIndex, TestSummary, ident_boundary,
    initializer_is_one_head_expr, is_ident_byte, is_variant_name, last_let_statement,
    mask_comments_and_strings, type_head,
};
use crate::analysis::classify::owner_pin::{
    OwnerScope, file_renames_to, parent_chain_shadows_type, test_module_shadows_type,
};

/// Which comparison macros reach the owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EqualityOwner<'a> {
    /// Self type of the `PartialEq` impl (`Key`).
    impl_type: &'a str,
    /// `assert_eq!` runs the owner.
    via_eq: bool,
    /// `assert_ne!` runs the owner.
    via_ne: bool,
    /// File-level refusals (imports, user macros, `macro_use`) are the same
    /// for every test of the owner's file, so they are computed once per
    /// owner on first use: `true` when none of them applies.
    file_level_clear: std::cell::OnceCell<bool>,
}

/// The equality reading of `owner`, or `None` when it is not `eq`/`ne` of a
/// plain `impl PartialEq for T`. `index` is read for a custom `ne` in the same
/// impl block, which `assert_ne!` calls instead of `eq`.
pub(super) fn equality_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<EqualityOwner<'a>> {
    let impl_type = partial_eq_impl_type(owner)?;
    if !partial_eq_trait_is_std(owner, index) {
        return None;
    }
    let block_prefix = owner.id.0.strip_suffix(owner.name.as_str())?;
    let custom_ne = index.functions().iter().any(|function| {
        function.name == "ne" && function.id.0.strip_suffix("ne") == Some(block_prefix)
    });
    let (via_eq, via_ne) = if owner.name == "eq" {
        (true, !custom_ne)
    } else {
        (false, true)
    };
    Some(EqualityOwner {
        impl_type,
        via_eq,
        via_ne,
        file_level_clear: std::cell::OnceCell::new(),
    })
}

/// The trait and self type text of the trait-impl method `owner`
/// (`Add<Money>` and `Money` for `impl Add<Money> for Money { fn add }`).
/// `None` for any other function, and for an impl nested in another impl or a
/// fn body: a second `::impl ` segment means the id no longer names one type
/// unambiguously.
pub(super) fn impl_header(owner: &FunctionSummary) -> Option<(&str, &str)> {
    if owner.id.0.matches("::impl ").count() != 1 {
        return None;
    }
    let rest = owner.id.0.split("::impl ").nth(1)?;
    let head = rest.rsplit_once("::")?.0;
    let (trait_ty, self_ty) = head.rsplit_once(" for ")?;
    Some((trait_ty.trim(), self_ty.trim()))
}

/// The self type of the plain `impl PartialEq for T` whose `eq`/`ne` is
/// `owner`; `None` for any other function.
fn partial_eq_impl_type(owner: &FunctionSummary) -> Option<&str> {
    if !matches!(owner.name.as_str(), "eq" | "ne") {
        return None;
    }
    let (trait_ty, self_ty) = impl_header(owner)?;
    if !matches!(
        trait_ty,
        "PartialEq" | "cmp::PartialEq" | "std::cmp::PartialEq" | "core::cmp::PartialEq"
    ) {
        return None;
    }
    plain_type_name(self_ty)
}

/// Whether `owner` is shaped like `eq`/`ne` of a plain `impl PartialEq for T`.
/// Id text alone cannot tell the standard trait from a local one of the same
/// name, so this is only the candidate prefilter's superset test.
pub(super) fn has_partial_eq_shape(owner: &FunctionSummary) -> bool {
    partial_eq_impl_type(owner).is_some()
}

/// Whether `owner` is `eq`/`ne` of a plain `impl PartialEq for T` where
/// `PartialEq` is the standard trait: the only owners the equality reading
/// and the fixture narrowing apply to.
pub(in crate::analysis) fn is_partial_eq_method(
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    partial_eq_impl_type(owner).is_some() && partial_eq_trait_is_std(owner, index)
}

/// Whether the bare name `PartialEq` can only mean the standard trait for the
/// owner's file: no workspace file declares a `trait PartialEq`, and the
/// owner's file imports none by a non-standard path or glob. Any doubt reads
/// as a shadow.
fn partial_eq_trait_is_std(owner: &FunctionSummary, index: &RustIndex) -> bool {
    std_trait_is_unshadowed(owner, index, "PartialEq")
}

/// [`partial_eq_trait_is_std`] for any standard trait `trait_name`: the bare
/// name can only mean the standard trait for the owner's file.
pub(super) fn std_trait_is_unshadowed(
    owner: &FunctionSummary,
    index: &RustIndex,
    trait_name: &str,
) -> bool {
    let declare = format!("trait {trait_name}");
    let declares_local_trait = index.files().values().any(|facts| {
        let source = facts.data().source.as_ref();
        source.contains(&declare) && declares_trait(&mask_comments_and_strings(source), trait_name)
    });
    if declares_local_trait {
        return false;
    }
    let Some(facts) = index.files().get(&owner.file) else {
        return false;
    };
    let masked = mask_comments_and_strings(facts.data().source.as_ref());
    !use_paths(&masked).into_iter().any(|path| {
        let words = path_words(path);
        if matches!(words.first(), Some(&("std" | "core" | "alloc"))) {
            return false;
        }
        let glob = words.contains(&"*");
        words.contains(&trait_name) || (glob && !(words.len() == 2 && is_local_root(&words)))
    })
}

pub(super) fn is_local_root(words: &[&str]) -> bool {
    matches!(words.first(), Some(&("super" | "self" | "crate")))
}

pub(super) fn path_words(path: &str) -> Vec<&str> {
    path.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '*'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether the masked source declares `trait name`.
pub(super) fn declares_trait(masked: &str, name: &str) -> bool {
    declares_item(masked, &["trait"], name)
}

/// Number of whole-word `kw name` declarations (`struct Key`, ...).
pub(super) fn count_items(masked: &str, keywords: &[&str], name: &str) -> usize {
    let bytes = masked.as_bytes();
    let mut count = 0;
    for keyword in keywords {
        let needle = format!("{keyword} {name}");
        let mut from = 0usize;
        while let Some(relative) = masked[from..].find(&needle) {
            let at = from + relative;
            from = at + needle.len();
            let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
            if before_ok && ident_boundary(masked, at, at + needle.len()) {
                count += 1;
            }
        }
    }
    count
}

fn declares_item(masked: &str, keywords: &[&str], name: &str) -> bool {
    count_items(masked, keywords, name) > 0
}

/// The identifier of a single-segment type, with at most lifetime
/// parameters (`Key`, `Key<'k>`, `Key<'_>`): no path, type or const
/// parameters and no reference, so the impl is the one `PartialEq<Self>` of
/// that type and cannot be one of several specialisations.
pub(super) fn plain_type_name(ty: &str) -> Option<&str> {
    let (name, generics) = match ty.split_once('<') {
        Some((name, rest)) => (name, Some(rest.strip_suffix('>')?)),
        None => (ty, None),
    };
    let lifetimes_only = generics.is_none_or(|params| {
        !params.is_empty()
            && params.split(',').all(|param| {
                param
                    .trim()
                    .strip_prefix('\'')
                    .is_some_and(|l| !l.is_empty() && l.bytes().all(is_ident_byte))
            })
    });
    (lifetimes_only
        && !name.is_empty()
        && name.bytes().all(is_ident_byte)
        && name.starts_with(|c: char| c.is_ascii_uppercase()))
    .then_some(name)
}

/// Whether `test` compares two values of the owner's type through an
/// `assert_eq!`/`assert_ne!` that runs the owner.
pub(super) fn test_asserts_equality_through_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    equality: &EqualityOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    // Same-file tests only, with the file's own source in hand so shadowing
    // and user macros can be ruled out.
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let impl_type = equality.impl_type;
    let masked = mask_comments_and_strings(&test.body);
    // Text inside a property-test macro is opaque to every other relation
    // (an explicit `a.eq(&b)` there is refused), so it is here too.
    let opaque: Vec<std::ops::Range<usize>> =
        crate::analysis::extract::property_macros::opaque_property_macros(&test.body)
            .into_iter()
            .map(|item| item.range)
            .collect();
    let reaching: Vec<_> = comparison_macros(&masked)
        .into_iter()
        .filter(|call| !opaque.iter().any(|range| range.contains(&call.start)))
        .filter(|call| {
            (if call.negated {
                equality.via_ne
            } else {
                equality.via_eq
            }) && call.operands.len() >= 2
                && call.operands[..2].iter().all(|operand| {
                    operand_has_type(operand, impl_type, &masked[..call.start], &|name| {
                        associated_fn_returns_self(index, impl_type, name)
                    })
                })
        })
        .collect();
    if reaching.is_empty() {
        return false;
    }
    if !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *equality.file_level_clear.get_or_init(|| {
        let masked_source = mask_comments_and_strings(source);
        if imports_other_type(&masked_source, impl_type, &owner.file) {
            return false;
        }
        const NAMES: [&str; 4] = [
            "assert_eq",
            "assert_ne",
            "debug_assert_eq",
            "debug_assert_ne",
        ];
        // A user macro of the same name may be defined here, imported by `use`,
        // brought in by `#[macro_use]`, or defined in any workspace file.
        let imported = use_paths(&masked_source).into_iter().any(|path| {
            let words = path_words(path);
            !matches!(words.first(), Some(&("std" | "core" | "alloc")))
                && words.iter().any(|word| NAMES.contains(word))
        });
        let defined_anywhere = index.files().values().any(|facts| {
            let other = facts.data().source.as_ref();
            other.contains("macro_rules!")
                && NAMES
                    .iter()
                    .any(|name| defines_macro(&mask_comments_and_strings(other), name))
        });
        // `#[macro_use] extern crate` in a crate-root file reaches every module.
        let macro_use_anywhere = index
            .files()
            .values()
            .any(|facts| facts.data().source.contains("macro_use"));
        !(imported || masked_source.contains("macro_use") || macro_use_anywhere || defined_anywhere)
    })
}

/// Whether no scope between `test` and the owner's file declares or renames
/// another `impl_type`, so the name means the owner's impl type there.
///
/// #15277 review: a value item of the same name (`fn Key(..) -> Other`,
/// `const Unit: Other`, also local to a test body) shadows a tuple-struct
/// constructor or a unit struct's value in its scope without touching the
/// type namespace, so `Key(1)` there is an `Other`. A `let` binding does
/// the same for a braced struct, which has no value of its own. Any `fn`,
/// `const`, `static` or `let` of that name anywhere in the file refuses.
pub(super) fn type_scope_is_clear(
    test: &TestSummary,
    owner: &FunctionSummary,
    source: &str,
    impl_type: &str,
    index: &RustIndex,
) -> bool {
    let raw_impl_type = format!("r#{impl_type}");
    let owner_scope = OwnerScope::new(owner.name.as_str(), owner.start_line, &owner.file);
    !(test_module_shadows_type(
        test,
        source,
        impl_type,
        owner_scope.in_file(&test.file),
        index,
    ) || file_renames_to(source, impl_type)
        || file_renames_to(source, raw_impl_type.as_str())
        || parent_chain_shadows_type(test, impl_type, owner_scope, index)
        || file_declares_value_named(test, source, impl_type, index))
}

/// Whether the test file binds `name` as a value: [`declares_value_named`]
/// over its masked source, or a pattern binder ([`binds_pattern_named`]). A
/// file whose raw text has no candidate declaration skips the keyword reading
/// without masking, and the masked copy and the pattern binders are the file
/// snapshot's memoized ones (the masked copy is the one
/// `test_module_shadows_type` reads), so no reading re-masks or re-walks a
/// large test file on every check (#12947).
fn file_declares_value_named(
    test: &TestSummary,
    source: &str,
    name: &str,
    index: &RustIndex,
) -> bool {
    if name.is_empty() || !source.contains(name) {
        return false;
    }
    // A caller supplying different source bytes must never borrow the
    // indexed snapshot.
    let indexed = index
        .files()
        .get(&test.file)
        .filter(|facts| std::ptr::eq(facts.source.as_ref(), source))
        .and_then(|_| index.source_syntax.get(index, &test.file));
    let unindexed;
    let snapshot: &crate::analysis::syntax::SourceSnapshot = match &indexed {
        Some(snapshot) => snapshot,
        None => {
            unindexed = crate::analysis::syntax::SourceSnapshot::new(source);
            &unindexed
        }
    };
    let masked = || snapshot.masked_source(|| mask_comments_and_strings(source));
    (may_declare_value_named(source, name) && declares_value_named(masked(), name))
        || binds_pattern_named(snapshot.pattern_binders(), masked(), name)
}

/// Whether a pattern may bind `name` as a local value somewhere in the file
/// (#15801). A braced struct lives only in the type namespace, so a closure
/// or function parameter, a `for`, `match`, `if let` or `while let` binding,
/// or a `let` pattern such as `let (a, Name)` or `let ref Name` can carry its
/// name, and `Name(..)` in that scope then calls the local value.
///
/// Parsed code is read exactly: any identifier pattern of that name refuses.
/// Macro arguments stay tokens, so a binder there is read from the text
/// ([`binder_like_use`]), as is the whole file when it has no clean parse.
/// The textual reading is skipped when the file declares a tuple or unit
/// struct of the name: that struct owns the value name wherever the type is
/// in scope (a binder of the name is then a compile error), and a unit
/// struct's bare value (`assert_eq!(a, Name)`) must not read as a binder.
/// Over-refusal only withholds the relation.
fn binds_pattern_named(
    binders: Option<&crate::analysis::syntax::PatternBinders>,
    masked: &str,
    name: &str,
) -> bool {
    if binders.is_some_and(|binders| binders.names.contains(name)) {
        return true;
    }
    if declares_value_struct(masked, name) {
        return false;
    }
    masked.match_indices(name).any(|(at, _)| {
        binders.is_none_or(|binders| {
            let after = binders
                .macro_ranges
                .partition_point(|range| range.end <= at);
            binders
                .macro_ranges
                .get(after)
                .is_some_and(|range| range.start <= at)
        }) && binder_like_use(masked, at, name)
    })
}

/// Whether the masked source declares `struct name(..)` or `struct name;`,
/// whose constructor or value owns `name` in the value namespace.
fn declares_value_struct(masked: &str, name: &str) -> bool {
    masked.match_indices(name).any(|(at, _)| {
        ident_boundary(masked, at, at + name.len())
            && masked[..at]
                .strip_suffix("r#")
                .unwrap_or(&masked[..at])
                .trim_end()
                .strip_suffix("struct")
                .is_some_and(|head| !head.bytes().last().is_some_and(is_ident_byte))
            && masked[at + name.len()..]
                .trim_start()
                .starts_with(['(', ';'])
    })
}

/// Whether the whole-word `name` at `at` in masked text sits where a pattern
/// could bind it: it is not the head of a call, literal, path, macro or
/// generic (`Name(`, `Name {`, `Name::`, `Name!`, `Name<`), a field base
/// (`Name.`) or a generic argument (`.., Name>`), and it follows `|`, `,`,
/// `(`, `[`, `&`, `mut`, `ref`, `for` or `let`, or is followed by `@`. This
/// is a superset of the binder shapes a macro argument can hold (`|Name|`,
/// `|a, Name: T|`, `Some(Name) =>`, `for Name in`, `(a, Name)`, `[Name, ..]`,
/// `ref Name`, `mut Name`, `Name @ ..`); it also takes a bare value or type
/// use in the same position, which only over-refuses.
fn binder_like_use(masked: &str, at: usize, name: &str) -> bool {
    if !ident_boundary(masked, at, at + name.len()) {
        return false;
    }
    let after = masked[at + name.len()..].trim_start();
    if after.starts_with('@') {
        return true;
    }
    if after.starts_with(['(', '{', '!', '<', '.', '>']) || after.starts_with("::") {
        return false;
    }
    let before = masked[..at].strip_suffix("r#").unwrap_or(&masked[..at]);
    let before = before.trim_end();
    before.ends_with(['|', ',', '(', '[', '&'])
        || ["mut", "ref", "for", "let"].iter().any(|keyword| {
            before
                .strip_suffix(keyword)
                .is_some_and(|head| !head.bytes().last().is_some_and(is_ident_byte))
        })
}

/// Keywords that bind `name` in the value namespace right before it: a `fn`,
/// `const` or `static` item, or a `let` binding. A `let` binding named like a
/// braced struct (which lives only in the type namespace) is legal Rust and
/// makes `Name(..)` call that local value, not a constructor (#15277).
const VALUE_BINDER_KEYWORDS: [&str; 4] = ["fn", "const", "static", "let"];

/// A raw-text superset of [`declares_value_named`]: some whole-word `name`
/// (or `r#name`) whose preceding code, past whitespace and an optional
/// `mut`, ends with `fn`, `const`, `static` or `let`, or with a comment
/// (`*/`, or a line holding `//`) that masking would turn into whitespace
/// between the keyword and the name.
fn may_declare_value_named(raw: &str, name: &str) -> bool {
    if name.is_empty() || !raw.contains(name) {
        return false;
    }
    raw.match_indices(name).any(|(at, _)| {
        if !ident_boundary(raw, at, at + name.len()) {
            return false;
        }
        let before = raw[..at].strip_suffix("r#").unwrap_or(&raw[..at]);
        let before = before.trim_end();
        let before = match before.strip_suffix("mut") {
            Some(head) if !head.bytes().last().is_some_and(is_ident_byte) => head.trim_end(),
            _ => before,
        };
        let line = before.rsplit('\n').next().unwrap_or(before);
        before.ends_with("*/")
            || line.contains("//")
            || VALUE_BINDER_KEYWORDS.iter().any(|keyword| {
                before.strip_suffix(keyword).is_some_and(|head| {
                    !head
                        .bytes()
                        .last()
                        .is_some_and(|byte| is_ident_byte(byte) || byte == b'#')
                })
            })
    })
}

/// Whether the masked source binds a `fn`, `const`, `static` or `let` (also
/// `static mut`, `const fn`, `let mut`) named `name` or `r#name`, at any
/// depth.
pub(super) fn declares_value_named(masked: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    masked.match_indices(name).any(|(at, _)| {
        if !ident_boundary(masked, at, at + name.len()) {
            return false;
        }
        let before = masked[..at].strip_suffix("r#").unwrap_or(&masked[..at]);
        let before = before.trim_end();
        let before = match before.strip_suffix("mut") {
            Some(head) if !head.bytes().last().is_some_and(is_ident_byte) => head.trim_end(),
            _ => before,
        };
        // `let Name(..)` and `let Name { .. }` (also under `if let` or
        // `while let`) are destructuring patterns that bind nothing named
        // `Name`.
        let after = masked[at + name.len()..].trim_start();
        let destructures = after.starts_with('(') || after.starts_with('{');
        VALUE_BINDER_KEYWORDS.iter().any(|keyword| {
            if *keyword == "let" && destructures {
                return false;
            }
            before.strip_suffix(keyword).is_some_and(|head| {
                // A head ending in `'` is the lifetime `'static`, as in
                // `&'static Name`, not a `static` item.
                !head
                    .bytes()
                    .last()
                    .is_some_and(|byte| is_ident_byte(byte) || byte == b'#' || byte == b'\'')
            })
        })
    })
}

/// Whether the file may bring a different `T` into scope. The owner id
/// carries no module path, so identity is only established when the file
/// leaves no second candidate: more than one `struct`/`enum`/`union`/`type`
/// named `T` is ambiguous; a `use` naming `T` must be `super::T` (the file
/// declares `T` once, so the parent module can only mean that one) or
/// `crate::T` from a crate-root file with the single declaration; any other
/// named path, and any glob other than `super::*`/`self::*`/`crate::*` or the
/// standard library's, may name another `T`. Over-refusal only withholds the
/// relation.
pub(super) fn imports_other_type(
    masked_source: &str,
    impl_type: &str,
    file: &std::path::Path,
) -> bool {
    let declarations = count_items(
        masked_source,
        &["struct", "enum", "union", "type"],
        impl_type,
    );
    if declarations > 1 {
        return true;
    }
    let crate_root = file
        .file_name()
        .is_some_and(|name| name == "lib.rs" || name == "main.rs");
    use_paths(masked_source).into_iter().any(|path| {
        let words = path_words(path);
        if matches!(words.first(), Some(&("std" | "core" | "alloc"))) {
            return false;
        }
        let glob = words.contains(&"*");
        if words.contains(&impl_type) {
            let plain = words.len() == 2 && !glob;
            return !(plain
                && declarations == 1
                && match words.first() {
                    Some(&"super") => true,
                    Some(&"crate") => crate_root,
                    _ => false,
                });
        }
        glob && !(words.len() == 2 && is_local_root(&words))
    })
}

/// The text of every `use` item up to its `;`, from the masked source.
pub(super) fn use_paths(masked: &str) -> Vec<&str> {
    let bytes = masked.as_bytes();
    let mut paths = Vec::new();
    let mut from = 0usize;
    while let Some(relative) = masked[from..].find("use ") {
        let at = from + relative;
        from = at + 4;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        let rest = &masked[at + 4..];
        paths.push(rest.split(';').next().unwrap_or(rest).trim());
    }
    paths
}

/// Whether `name` is bound by something other than a plain `let name`
/// before `before` ends: a `for`/`if let`/`while let`/`let` pattern holding
/// it with other tokens (tuple or variant patterns), or a closure parameter.
/// Binding resolution is flow-insensitive, so any such rebinding makes the
/// last plain `let` unreliable. A match-arm pattern is read as any later
/// `=>` before the next `;`, `{` or `}`; that over-refuses (a name used in an
/// arm body before another arm) and only withholds the relation.
fn rebound_by_pattern(before: &str, name: &str) -> bool {
    let mut from = 0usize;
    while let Some(relative) = before[from..].find(name) {
        let at = from + relative;
        from = at + name.len();
        if !ident_boundary(before, at, at + name.len()) {
            continue;
        }
        if in_struct_pattern(before, at) || in_match_arm_pattern(before, at + name.len()) {
            return true;
        }
        let line_start = before[..at].rfind([';', '{', '}']).map_or(0, |i| i + 1);
        let prefix = before[line_start..at].trim_start();
        // Text between the keyword and the name is the pattern so far: an
        // `=` (let) or ` in ` (for) before the name puts it in the
        // initializer or iterator instead.
        let in_pattern = |lead: &str| {
            prefix
                .strip_prefix(lead)
                .is_some_and(|pattern| !pattern.contains('=') && !pattern.contains(" in "))
        };
        let plain_let = prefix == "let " || prefix == "let mut ";
        if !plain_let
            && (in_pattern("for ")
                || in_pattern("if let ")
                || in_pattern("while let ")
                || in_pattern("let ")
                || prefix.matches('|').count() % 2 == 1)
        {
            return true;
        }
    }
    false
}

/// Whether a `=>` follows `from` before any `;`, `{` or `}`: the name is
/// (part of) a match-arm pattern or guard.
fn in_match_arm_pattern(before: &str, from: usize) -> bool {
    let bytes = before.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b';' | b'{' | b'}' => return false,
            b'=' if bytes.get(i + 1) == Some(&b'>') => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Whether the identifier at `at` sits inside a `{ .. }` group that is a
/// destructuring pattern: the group is closed by `}` followed by a lone `=`
/// or `in`, as in `let S { a, .. } = x`, `if let S { a } = x`,
/// `for S { a } in xs`. A struct literal initialiser is followed by `;`.
fn in_struct_pattern(before: &str, at: usize) -> bool {
    let bytes = before.as_bytes();
    let mut depth = 0usize;
    let mut open = None;
    for index in (0..at).rev() {
        match bytes[index] {
            b'}' => depth += 1,
            b'{' if depth == 0 => {
                open = Some(index);
                break;
            }
            b'{' => depth -= 1,
            _ => {}
        }
    }
    let Some(open) = open else {
        return false;
    };
    let mut depth = 0usize;
    for (offset, &byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let rest = before[open + offset + 1..].trim_start();
                    return (rest.starts_with('=') && !rest.starts_with("=="))
                        || rest.starts_with("in ");
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether a `macro_rules! name` definition appears in the masked source.
fn defines_macro(masked: &str, name: &str) -> bool {
    let needle = "macro_rules!";
    let mut search = 0usize;
    while let Some(relative) = masked[search..].find(needle) {
        let after = masked[search + relative + needle.len()..].trim_start();
        if after
            .strip_prefix(name)
            .is_some_and(|rest| !rest.bytes().next().is_some_and(is_ident_byte))
        {
            return true;
        }
        search += relative + needle.len();
    }
    false
}

/// Whether `before` ends with the whole path segment `root::` (not
/// `foo_std::`) and that segment starts the path.
fn path_root_before(before: &str, root: &str) -> bool {
    let Some(head) = before.strip_suffix("::").and_then(|h| h.strip_suffix(root)) else {
        return false;
    };
    !head
        .as_bytes()
        .last()
        .is_some_and(|&b| is_ident_byte(b) || b == b':')
}

#[derive(Debug)]
struct ComparisonMacro<'a> {
    /// Offset of the macro name.
    start: usize,
    negated: bool,
    /// Top-level comma-separated arguments.
    operands: Vec<&'a str>,
}

/// Every `assert_eq!`/`assert_ne!` (and `debug_` twin) in the masked body.
fn comparison_macros(masked: &str) -> Vec<ComparisonMacro<'_>> {
    let mut found = Vec::new();
    for (name, negated) in [
        ("assert_eq!", false),
        ("assert_ne!", true),
        ("debug_assert_eq!", false),
        ("debug_assert_ne!", true),
    ] {
        let mut search = 0usize;
        while let Some(relative) = masked[search..].find(name) {
            let start = search + relative;
            search = start + name.len();
            // `debug_assert_eq!` contains no `assert_eq!` boundary hit, and
            // `my_assert_eq!` is another macro.
            if !ident_boundary(masked, start, start + name.len()) {
                continue;
            }
            let before = &masked[..start];
            if before.ends_with("::")
                && !(path_root_before(before, "std") || path_root_before(before, "core"))
            {
                continue;
            }
            let after = masked[search..].trim_start();
            let offset = masked.len() - after.len();
            let Some(&open) = after.as_bytes().first() else {
                continue;
            };
            if !matches!(open, b'(' | b'[' | b'{') {
                continue;
            }
            if let Some(operands) = split_arguments(masked, offset) {
                found.push(ComparisonMacro {
                    start,
                    negated,
                    operands,
                });
            }
        }
    }
    found
}

/// The top-level arguments of the bracket group opening at `open`, or `None`
/// when it is unbalanced. A comma inside generic angle brackets would split
/// wrongly; the resulting operand then fails to resolve, which fails closed.
fn split_arguments(masked: &str, open: usize) -> Option<Vec<&str>> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut args = Vec::new();
    let mut arg_start = open + 1;
    for (index, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let last = masked[arg_start..index].trim();
                    if !last.is_empty() {
                        args.push(last);
                    }
                    return Some(args);
                }
            }
            b',' if depth == 1 => {
                args.push(masked[arg_start..index].trim());
                arg_start = index + 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether `operand` evaluates to a value of `impl_type`. Only forms whose
/// type the syntax (or a resolved declaration) guarantees count: the unit
/// struct, `T { .. }`, `T(..)`, `T::Variant(..)`, `T::assoc(..)` whose
/// declared return type is `Self`/`T` (`returns_self`), and a binding whose
/// last `let` is annotated `: T` or initialised by one of those. A
/// constructor-shaped name is not a return type: `T::new()` may return
/// another type. `before` is the masked body up to the assertion.
pub(super) fn operand_has_type(
    operand: &str,
    impl_type: &str,
    before: &str,
    returns_self: &dyn Fn(&str) -> bool,
) -> bool {
    let mut operand = operand.trim();
    while let Some(rest) = operand
        .strip_prefix('&')
        .or_else(|| operand.strip_prefix("mut "))
    {
        operand = rest.trim_start();
    }
    if operand.is_empty() {
        return false;
    }
    if operand.bytes().all(is_ident_byte) {
        // A unit struct is its own value; any other bare name is a binding.
        return operand == impl_type
            || (!rebound_by_pattern(before, operand)
                && count_lets(before, operand) == 1
                && last_let_statement(before, operand)
                    .is_some_and(|stmt| statement_has_type(stmt, impl_type, returns_self)));
    }
    expression_has_type(operand, impl_type, returns_self)
}

/// How many `let [mut] name` statements bind `name` before the assertion.
/// More than one makes the in-scope binding ambiguous (shadowing in a nested
/// block), so the type is not resolved.
fn count_lets(before: &str, name: &str) -> usize {
    let bytes = before.as_bytes();
    let mut count = 0;
    let mut search = 0usize;
    while let Some(relative) = before[search..].find("let ") {
        let let_at = search + relative;
        search = let_at + 1;
        if let_at > 0 && is_ident_byte(bytes[let_at - 1]) {
            continue;
        }
        let mut rest = before[let_at + 4..].trim_start();
        if let Some(after) = rest.strip_prefix("mut ") {
            rest = after.trim_start();
        }
        if rest.starts_with(name) && !rest[name.len()..].bytes().next().is_some_and(is_ident_byte) {
            count += 1;
        }
    }
    count
}

fn statement_has_type(stmt: &str, impl_type: &str, returns_self: &dyn Fn(&str) -> bool) -> bool {
    let Some(eq) = stmt.find('=') else {
        return false;
    };
    let pattern = &stmt[..eq];
    if let Some(colon) = pattern.find(':') {
        return type_head(&pattern[colon + 1..])
            .is_some_and(|(segments, _)| segments.as_slice() == [impl_type]);
    }
    expression_has_type(&stmt[eq + 1..], impl_type, returns_self)
}

fn expression_has_type(expr: &str, impl_type: &str, returns_self: &dyn Fn(&str) -> bool) -> bool {
    let expr = expr.trim();
    if !initializer_is_one_head_expr(expr) || expr.contains("::<") {
        return false;
    }
    let Some((segments, next)) = type_head(expr) else {
        return false;
    };
    match (segments.as_slice(), next) {
        ([ty], Some(b'(' | b'{')) => *ty == impl_type,
        ([ty, call], Some(b'(')) if *ty == impl_type => returns_self(call) || is_variant_name(call),
        ([ty, variant], Some(b'{')) if *ty == impl_type => is_variant_name(variant),
        _ => false,
    }
}

/// Whether every function `name` defined in an `impl` of `impl_type` declares
/// `Self` or `impl_type` as its return type, and at least one exists. The
/// signature is read from the function's own file, from its line up to the
/// body or `;`.
pub(super) fn associated_fn_returns_self(index: &RustIndex, impl_type: &str, name: &str) -> bool {
    let mut found = false;
    for function in index.functions().iter() {
        let in_impl = matches!(
            &function.impl_context,
            FunctionImplContext::Impl { self_type } if self_type == impl_type
        );
        if function.name != name || !in_impl {
            continue;
        }
        let Some(facts) = index.files().get(&function.file) else {
            return false;
        };
        let masked = mask_comments_and_strings(facts.data().source.as_ref());
        if !signature_returns(&masked, function.start_line, impl_type) {
            return false;
        }
        found = true;
    }
    found
}

/// Whether the signature starting at 1-based `line` ends `-> Self` or
/// `-> ty`, with no generics or wrappers.
fn signature_returns(masked: &str, line: usize, ty: &str) -> bool {
    let text: String = masked
        .lines()
        .skip(line.saturating_sub(1))
        .skip_while(|l| l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let end = text.find(['{', ';']).unwrap_or(text.len());
    let signature = &text[..end];
    let Some((_, ret)) = signature.rsplit_once("->") else {
        return false;
    };
    let ret = ret.split(" where ").next().unwrap_or(ret).trim();
    ret == "Self" || ret == ty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macros(body: &str) -> Vec<(bool, Vec<String>)> {
        comparison_macros(body)
            .into_iter()
            .map(|call| {
                (
                    call.negated,
                    call.operands.iter().map(|a| a.to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn comparison_macros_split_top_level_operands() {
        let found = macros("assert_eq!(a, f(b, c), \"msg {}\", x);");
        assert_eq!(found.len(), 1);
        assert!(!found[0].0);
        assert_eq!(&found[0].1[..2], ["a", "f(b, c)"]);
        let found = macros("assert_ne!(Key::new(1, 2), Key { id: 3, n: 4 });");
        assert_eq!(found.len(), 1);
        assert!(found[0].0);
        assert_eq!(found[0].1, ["Key::new(1, 2)", "Key { id: 3, n: 4 }"]);
    }

    #[test]
    fn comparison_macros_ignore_other_macros_and_unbalanced_input() {
        assert!(macros("my_assert_eq!(a, b);").is_empty());
        assert!(macros("assert!(a == b);").is_empty());
        assert!(macros("assert_eq!(a, b").is_empty());
        assert_eq!(macros("debug_assert_ne!(a, b);").len(), 1);
        assert_eq!(macros("std::assert_eq!(a, b);").len(), 1);
        assert!(macros("other::assert_eq!(a, b);").is_empty());
        assert!(macros("foo_std::assert_eq!(a, b);").is_empty());
        assert!(macros("x::std::assert_eq!(a, b);").is_empty());
    }

    #[test]
    fn operands_resolve_only_to_syntactically_established_types() {
        let before = "let a = Key::new(1);\nlet b: Key = make();\nlet c = make();\nlet d = Key::new(1).id();\nlet e = Key::other(1);\n";
        let ctor = |name: &str| name == "new";
        for typed in [
            "a",
            "&a",
            "b",
            "Key::new(2)",
            "Key { id: 1 }",
            "Key(1)",
            "Key",
            "Key::Circle(1)",
        ] {
            assert!(operand_has_type(typed, "Key", before, &ctor), "{typed}");
        }
        for untyped in [
            "c",
            "d",
            "other",
            "make()",
            "Key::new(1).id()",
            "Key::make_other()",
            "Key::default()",
            "e",
            "a::Key { id: 1 }",
            "Other::new(1)",
            "String::from(\"x\")",
            "1",
            "Key::<u8>::new(1)",
            "",
        ] {
            assert!(
                !operand_has_type(untyped, "Key", before, &ctor),
                "{untyped}"
            );
        }
    }

    #[test]
    fn a_later_shadowing_let_decides_the_binding_type() {
        let before = "let a = Key::new(1);\nlet a = make();\n";
        assert!(!operand_has_type("a", "Key", before, &|_| true));
    }

    #[test]
    fn user_macro_definitions_are_found_by_exact_name() {
        assert!(defines_macro(
            "macro_rules! assert_eq { () => {} }",
            "assert_eq"
        ));
        assert!(defines_macro("macro_rules!   assert_eq{}", "assert_eq"));
        assert!(!defines_macro(
            "macro_rules! assert_eq_all { () => {} }",
            "assert_eq"
        ));
        assert!(!defines_macro("fn assert_eq() {}", "assert_eq"));
    }

    fn eq_owner(id: &str) -> FunctionSummary {
        use crate::analysis::facts::FunctionSourceRole;
        FunctionSummary {
            id: crate::domain::SymbolId(id.to_string()),
            name: "eq".to_string(),
            file: std::path::PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: String::new().into(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            returns: Vec::new(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            source_role: FunctionSourceRole::Production,
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            impl_attrs: Vec::new(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    #[test]
    fn an_impl_nested_in_another_impl_names_no_single_type() {
        let single = eq_owner("src/lib.rs::impl PartialEq for Key::eq");
        assert_eq!(partial_eq_impl_type(&single), Some("Key"));
        // Two `::impl ` segments: the id cannot say which impl owns `eq`.
        let nested =
            eq_owner("src/lib.rs::impl PartialEq for Outer::eq::impl PartialEq for Key::eq");
        assert_eq!(partial_eq_impl_type(&nested), None);
        let nested_other = eq_owner("src/lib.rs::impl Outer::run::impl PartialEq for Key::eq");
        assert_eq!(partial_eq_impl_type(&nested_other), None);
    }

    #[test]
    fn plain_type_names_reject_paths_generics_and_references() {
        for plain in ["Key", "Key<'k>", "Key<'_>", "Key<'a, 'b>"] {
            assert_eq!(plain_type_name(plain), Some("Key"), "{plain}");
        }
        for other in [
            "",
            "key",
            "a::Key",
            "Key<T>",
            "Key<u8>",
            "Key<'a, T>",
            "Key<>",
            "&Key",
            "Key ",
            "Key<'a",
        ] {
            assert_eq!(plain_type_name(other), None, "{other:?}");
        }
    }

    #[test]
    fn a_format_message_with_commas_never_changes_the_operands() {
        let masked = mask_comments_and_strings("assert_eq!(a, b, \"x,y,{}\", z);");
        let found = comparison_macros(&masked);
        assert_eq!(found.len(), 1);
        assert_eq!(&found[0].operands[..2], ["a", "b"]);
    }

    #[test]
    fn imports_that_may_name_another_key_are_refused() {
        let file = std::path::Path::new("src/inner.rs");
        for refused in [
            "struct Key; use super::b::Key;",
            "struct Key; use crate::b::Key;",
            "struct Key; use other::Key;",
            "struct Key; use super::{Key, X};",
            "struct Key; use super::b::*;",
            "struct Key; use other::*;",
            "struct Key; use crate::Key;",
            "use super::Key;",
            "struct Key; mod m { struct Key; }",
            "struct Key; enum Key {}",
        ] {
            assert!(imports_other_type(refused, "Key", file), "{refused}");
        }
        for allowed in [
            "struct Key; use super::*;",
            "struct Key; use super::Key;",
            "struct Key; use std::collections::HashMap;",
            "struct Key; use super::other::Thing;",
            "struct KeyOther; struct Key;",
        ] {
            assert!(!imports_other_type(allowed, "Key", file), "{allowed}");
        }
        let root = std::path::Path::new("src/lib.rs");
        assert!(!imports_other_type(
            "struct Key; use crate::Key;",
            "Key",
            root
        ));
        assert!(imports_other_type("use crate::Key;", "Key", root));
    }

    #[test]
    fn pattern_and_closure_rebindings_make_a_let_unreliable() {
        for rebound in [
            "let a = Key::new(1); for a in xs { ",
            "let a = Key::new(1); if let Some(a) = f() { ",
            "let a = Key::new(1); let (a, c) = f(); ",
            "let a = Key::new(1); xs.map(|a| a + 1); ",
            "let a = Key::new(1); let Wrap { a, .. } = w; ",
            "let a = Key::new(1); if let Foo { a } = x { ",
            "let a = Key::new(1); for S { a } in xs { ",
        ] {
            assert!(rebound_by_pattern(rebound, "a"), "{rebound}");
            assert!(
                !operand_has_type("a", "Key", rebound, &|_| true),
                "{rebound}"
            );
        }
        for plain in [
            "let a = Key::new(1); let b = Key::new(a); ",
            "let mut a = Key::new(1); f(a); ",
            "let a: Key = make(); for x in a.items() { ",
            "let a = Key::new(1); let b = Wrap { a }; ",
        ] {
            assert!(!rebound_by_pattern(plain, "a"), "{plain}");
        }
    }

    #[test]
    fn match_arm_patterns_and_repeated_lets_are_ambiguous() {
        assert!(rebound_by_pattern(
            "match x { Some(a) => 1, None => 2 }; ",
            "a"
        ));
        assert!(!rebound_by_pattern("let a = K::new(); foo(a); ", "a"));
        assert_eq!(
            count_lets("let a = 1; { let mut a = 2; } let ab = 3;", "a"),
            2
        );
        assert_eq!(count_lets("let a = 1;", "a"), 1);
    }

    #[test]
    fn a_value_item_of_the_type_name_shadows_its_constructor() {
        assert!(declares_value_named(
            "fn Key(v: i32) -> Other { Other(v) }",
            "Key"
        ));
        assert!(declares_value_named(
            "    const Key: Other = Other(1);",
            "Key"
        ));
        assert!(declares_value_named("static mut Key: u8 = 0;", "Key"));
        assert!(declares_value_named(
            "pub const fn Key() -> u8 { 0 }",
            "Key"
        ));
        assert!(declares_value_named("fn r#Key() {}", "Key"));
        // A `let` binding of the name shadows a braced struct's call
        // spelling: `Key(1)` then calls the local closure.
        assert!(declares_value_named("let Key = |x: i32| Other(x);", "Key"));
        assert!(declares_value_named("let mut Key = make;", "Key"));
        assert!(may_declare_value_named("let Key = make;", "Key"));
        assert!(!declares_value_named("letKey = 1;", "Key"));
        // A destructuring pattern binds nothing named `Key`, and `'static`
        // is a lifetime, not a `static` item.
        assert!(!declares_value_named("let Key(v) = k;", "Key"));
        assert!(!declares_value_named("if let Key(v) = k {}", "Key"));
        assert!(!declares_value_named("let Key { v } = k;", "Key"));
        assert!(!declares_value_named("let r: &'static Key = &K;", "Key"));
        assert!(declares_value_named(
            "let Key: fn(i32) -> Other = mk;",
            "Key"
        ));
        // A type item, a call, a longer name or another keyword is no value
        // item of that name.
        assert!(!declares_value_named("pub struct Key(pub i32);", "Key"));
        assert!(!declares_value_named("let k = Key(1);", "Key"));
        assert!(!declares_value_named("fn KeyMaker() {}", "Key"));
        assert!(!declares_value_named("fn make() -> Key { Key(1) }", "Key"));
        assert!(!declares_value_named("let myfn Key = 1;", "Key"));
        // The raw prefilter keeps every masked hit, a comment between the
        // keyword and the name included, and drops a file with no candidate.
        assert!(may_declare_value_named("fn /* c */ Key() {}", "Key"));
        assert!(may_declare_value_named("fn // c\n    Key() {}", "Key"));
        assert!(may_declare_value_named("static mut Key: u8 = 0;", "Key"));
        assert!(!may_declare_value_named(
            "let k = Key(1);\nstruct Key;",
            "Key"
        ));
        assert!(!may_declare_value_named("fn make() {}", "Key"));
    }

    /// The production reading of one file's text: its pattern binders and
    /// masked source, as `file_declares_value_named` hands them over.
    fn pattern_binds(source: &str, name: &str) -> bool {
        let snapshot = crate::analysis::syntax::SourceSnapshot::new(source);
        binds_pattern_named(
            snapshot.pattern_binders(),
            &mask_comments_and_strings(source),
            name,
        )
    }

    #[test]
    fn a_pattern_binder_of_the_type_name_shadows_its_call_spelling() {
        // #15801: every binder shape that is not `fn`/`const`/`static`/`let
        // Name` refuses, in parsed code.
        for body in [
            "let f = |Key: fn(i32) -> Other| { let x: T = Key(1).into(); };",
            "let f = |a: i32, Key| Key(a);",
            "for Key in makers { let x: T = Key(1).into(); }",
            "match maker { Key => { let x: T = Key(1).into(); } }",
            "match maker { Some(Key) => {} None => {} }",
            "if let Some(Key) = maker { let x: T = Key(1).into(); }",
            "while let Some(Key) = makers.pop() {}",
            "let (a, Key) = pair;",
            "let ref Key = maker;",
            "let [Key, ..] = makers;",
            "let Wrap { maker: Key } = wrap;",
            "let Wrap { Key } = wrap;",
            "let r#Key @ _ = maker;",
            "let Some(Key) = maker else { return; };",
        ] {
            let source = format!("struct Key {{ v: i32 }}\nfn t() {{\n    {body}\n}}\n");
            assert!(pattern_binds(&source, "Key"), "binder not read: {body}");
        }
        assert!(pattern_binds(
            "struct Key { v: i32 }\nfn t(Key: fn(i32) -> Other) { Key(1); }\n",
            "Key"
        ));
        // Uses of the type that bind nothing keep the reading.
        for body in [
            "let k = Key { v: 1 };",
            "let k: Key = make();",
            "let ks: Vec<(i32, Key)> = Vec::new();",
            "let f: fn(Key) -> Other = convert;",
            "let Key { v } = k;",
            "if let Some(Key { v }) = maybe {}",
            "let n = keyed(|k: Key| k.v);",
            "let x = make::<Key>();",
            "let key = Key::new(1);",
        ] {
            let source = format!("struct Key {{ v: i32 }}\nfn t() {{\n    {body}\n}}\n");
            assert!(!pattern_binds(&source, "Key"), "over-read: {body}");
        }
        // A comment or string naming a binder binds nothing.
        assert!(!pattern_binds(
            "struct Key { v: i32 }\nfn t() { let s = \"|Key| 1\"; /* for Key in x */ }\n",
            "Key"
        ));
    }

    #[test]
    fn a_binder_inside_macro_arguments_is_read_from_the_text() {
        // Macro arguments are tokens to the parser, so the binder is found
        // by its position.
        for body in [
            "assert_eq!(apply(|Key: fn(i32) -> Other| Key(1)), 2);",
            "assert_eq!(apply(|a, Key| Key(a)), 2);",
            "assert!(matches!(maker, Some(Key) if Key(1) == 2));",
            "check!(for Key in makers { run(Key(1)); });",
            "check!(let (a, Key) = pair);",
            "check!(let ref Key = maker);",
            "check!(|mut Key| Key(1));",
            "check!(match m { [Key, ..] => 1, _ => 2 });",
            "check!(match m { Key @ _ => 1 });",
            "check!(|&Key| Key(1));",
        ] {
            let source = format!("struct Key {{ v: i32 }}\nfn t() {{\n    {body}\n}}\n");
            assert!(
                pattern_binds(&source, "Key"),
                "macro binder not read: {body}"
            );
        }
        // Heads of a literal, call, path or generic, and a type after `:`,
        // `->`, `<` or `as`, are no binders.
        for body in [
            "assert_eq!(Key { v: 1 }.v, 1);",
            "assert_eq!(make(1, Key { v: 1 }), 1);",
            "assert_eq!(Key::new(1).v, 1);",
            "assert_eq!(apply(|k: Key| k.v), 1);",
            "assert_eq!(make::<Key>().v, 1);",
            "assert_eq!(size_of::<Map<u8, Key>>(), 8);",
            "assert_eq!((|| -> Key { make() })().v, 1);",
        ] {
            let source = format!("struct Key {{ v: i32 }}\nfn t() {{\n    {body}\n}}\n");
            assert!(!pattern_binds(&source, "Key"), "over-read: {body}");
        }
        // An attribute's arguments are no macro arguments.
        assert!(!pattern_binds(
            "#[derive(Debug, Key)]\nstruct Key { v: i32 }\n",
            "Key"
        ));
        // A tuple or unit struct owns the value name, so its bare value in
        // a macro argument is the struct, not a binder.
        assert!(!pattern_binds(
            "struct Key;\nfn t() { assert_eq!(make(), Key); }\n",
            "Key"
        ));
        assert!(!pattern_binds(
            "struct Key(i32);\nfn t() { assert_eq!(wrap(1, Key), Key(1)); }\n",
            "Key"
        ));
        assert!(declares_value_struct("pub struct r#Key (i32);", "Key"));
        assert!(!declares_value_struct("struct Key { v: i32 }", "Key"));
        assert!(!declares_value_struct("struct KeyRing(i32);", "Key"));
        // A parsed identifier pattern still refuses beside such a struct.
        assert!(pattern_binds(
            "mod a { pub struct Key(i32); }\nfn t() { let f = |Key: fn(i32) -> i32| Key(1); }\n",
            "Key"
        ));
    }

    #[test]
    fn a_file_without_a_clean_parse_is_read_from_the_text() {
        let broken = "struct Key { v: i32 }\nfn t() { let f = |Key: fn(i32) -> Other| Key(1); \n";
        let snapshot = crate::analysis::syntax::SourceSnapshot::new(broken);
        assert!(snapshot.pattern_binders().is_none());
        assert!(pattern_binds(broken, "Key"));
        assert!(!pattern_binds(
            "struct Key { v: i32 }\nfn t() { let k: Key = make(); \n",
            "Key"
        ));
    }
}
