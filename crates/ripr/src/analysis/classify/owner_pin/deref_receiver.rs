//! #15277: a test receiver whose type is a wrapper that derefs to the owner's
//! type (`Guard(Inner { cap: 3 }).limit()`, with `impl Deref for Guard {
//! type Target = Inner; .. }`).
//!
//! Method lookup on a `Guard` receiver tries `Guard`, `&Guard` and
//! `&mut Guard` (inherent methods, then every trait in scope, including std,
//! derived and blanket impls) before it derefs to `Inner`. So the reading
//! credits an inherent `&self` method of `Inner` through a `Guard` receiver
//! only when nothing on the way can answer the call first, and every gate
//! fails closed:
//!
//! - the owner is an inherent method of a plain-named type whose first
//!   parameter is exactly `&self`: a `&mut self` call needs `DerefMut` and a
//!   mutable place, a by-value one moves out of the deref;
//! - the method name is not one that `Guard`'s std, derived or blanket impls
//!   may answer ([`DEREF_RECEIVER_METHODS`] and the by-value prelude names),
//!   and the workspace spells `fn <method>` exactly once: any other
//!   definition, inherent or trait, blanket or not, could take the call;
//! - the workspace sources are closed
//!   ([`closed_dispatch::sources_closed`](super::closed_dispatch::sources_closed)):
//!   every member indexed, no proc-macro member, a dependency closure of
//!   workspace path packages only (no renamed `package = ..` dependency),
//!   every `use` and macro path rooted at `std`, `core`, `alloc` or the
//!   workspace, no `macro_rules!`, `include!`, `extern crate` or `#[path]`,
//!   and only compiler built-in attributes and std derives, since a macro
//!   may emit an impl ripr cannot read;
//! - the index holds the whole workspace: every Rust file the workspace walk
//!   finds under the root is indexed (whatever the mode indexed);
//! - every spelling of `Deref` or `DerefMut` in the workspace is an exact
//!   std import (`use std::ops::Deref;`, `use core::ops::{Deref, DerefMut};`)
//!   or an unattributed `impl <std path or imported name> for <Type>` header.
//!   A workspace trait, renamed import, bound, alias or macro argument
//!   spelling either name refuses, so `Deref` is the std trait wherever it
//!   is implemented;
//! - exactly one `impl Deref for ..` has `type Target = <owner type>;`,
//!   spelled exactly, and it is the only `Deref` impl of its wrapper. Its
//!   body is exactly that `type Target` and a `fn deref(&self)` returning
//!   `&self.<field>`, so the deref runs no other code and hands the call the
//!   value the test built. Every other `Deref` impl in the workspace
//!   declares a plain `type Target` naming some other type: not the owner,
//!   not `Target` or `Self`, and not a name a `type` item or an `as` rename
//!   introduces, whatever its `fn deref` does (a deref returning `&OTHER`
//!   still hands a coerced call an owner value). No `impl` header names
//!   `Borrow`, so a std `Cow` field derefs without workspace code. Neither
//!   the owner nor the wrapper name is a `type` alias or an `as` rename
//!   (`use std::string::String as Inner;`), and no `const`, `static`, `fn`
//!   or `let` binds the wrapper's name, so `Guard(..)` is its constructor.
//!   Neither name is a std prelude or primitive type name, and no `use`
//!   outside `crate`/`self`/`super` spells either name or imports a glob,
//!   so both names denote the workspace types everywhere;
//! - the wrapper is declared exactly once in the workspace, as a non-generic
//!   `struct` whose only attributes are std derives; no function shares its
//!   name; no `type` item mentions it; it is never renamed (`Guard as G`);
//!   it is never an argument of a macro other than the std assertion,
//!   formatting and `vec!` macros; and its only `impl` headers are the
//!   `Deref`/`DerefMut` ones and plain inherent `impl Guard {` blocks.
//!
//! Only the wrapper's one `Deref` hop is read: a nested chain (`Outer` to
//! `Guard` to `Inner`) is not followed, and `Deref::deref` itself is never
//! credited this way (its owner is a trait-impl method, not an inherent one).

use super::{
    OwnerPinSyntax, RustIndex, is_ident_byte, is_plain_identifier, matching_close,
    method_may_dispatch_through_prelude, whole_word_offsets, workspace_type_name_count,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionContainer;
use crate::analysis::rust_index::FunctionSummary;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Method names a wrapper may answer before the deref: its derived std
/// traits (`Clone`, `Debug`, `PartialEq`, `PartialOrd`, `Ord`, `Hash`,
/// `Default`), the `Deref` family itself, the blanket impls on every `T`
/// and `&T` (`Any`, `Borrow`, `Into`, `TryInto`, `ToOwned`, `ToString`,
/// `AsRef`), and the conversion names those cover.
const DEREF_RECEIVER_METHODS: &[&str] = &[
    "as_mut",
    "as_ref",
    "borrow",
    "borrow_mut",
    "clamp",
    "clone",
    "clone_from",
    "clone_into",
    "cmp",
    "default",
    "deref",
    "deref_mut",
    "eq",
    "fmt",
    "from",
    "ge",
    "gt",
    "hash",
    "hash_slice",
    "into",
    "le",
    "lt",
    "max",
    "min",
    "ne",
    "partial_cmp",
    "to_owned",
    "to_string",
    "try_from",
    "try_into",
    "type_id",
];

/// Derives whose generated methods all appear in [`DEREF_RECEIVER_METHODS`].
const STD_DERIVES: &[&str] = &[
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "Eq",
    "Hash",
    "Ord",
    "PartialEq",
    "PartialOrd",
];

/// Macros whose arguments may spell the wrapper: they evaluate their operands
/// and generate no items. The `use` gate keeps them the std macros.
const OPERAND_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "eprintln",
    "format",
    "println",
    "vec",
];

/// The `use` items that may spell `Deref` or `DerefMut`, whitespace removed.
const STD_DEREF_IMPORTS: &[&str] = &[
    "usestd::ops::Deref;",
    "usecore::ops::Deref;",
    "usestd::ops::DerefMut;",
    "usecore::ops::DerefMut;",
    "usestd::ops::{Deref,DerefMut};",
    "usecore::ops::{Deref,DerefMut};",
    "usestd::ops::{DerefMut,Deref};",
    "usecore::ops::{DerefMut,Deref};",
];

/// The std paths an `impl` header may name the traits by.
const STD_PATH_PREFIXES: &[&str] = &["std::ops::", "core::ops::", "::std::ops::", "::core::ops::"];

/// The workspace-wide half of the reading, computed once per run: every
/// owner-independent gate in the module docs, the `Deref` impls they admit
/// and the `fn` definition counts the per-owner name gate reads.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis::classify) struct DerefWorkspace {
    /// Owner type to the one wrapper that derefs to it and passes every
    /// wrapper gate. A type two admitted `Deref` impls target is absent.
    wrappers: BTreeMap<String, String>,
    /// How often each name is spelled `fn <name>` across the workspace.
    fn_definitions: BTreeMap<String, usize>,
}

/// The one wrapper type whose receivers reach `owner` through its `Deref`
/// impl, when every gate in the module docs holds. Memoized per owner on
/// the index's run-scoped receiver syntax, which both the owner pin and the
/// inherent receiver relation read, over a workspace verdict that is
/// computed at most once per run.
pub(in crate::analysis::classify) fn deref_wrapper(
    owner: &FunctionSummary,
    index: &RustIndex,
) -> Option<String> {
    deref_wrapper_for(&owner.file, owner.start_line, &owner.name, index, || {
        Some(owner)
    })
}

/// [`deref_wrapper`] for an owner known by its (file, start line, name):
/// the workspace verdict and the per-owner memo are read before `owner`
/// is asked to find the indexed function.
pub(in crate::analysis::classify) fn deref_wrapper_for<'i>(
    file: &Path,
    start_line: usize,
    name: &str,
    index: &'i RustIndex,
    owner: impl FnOnce() -> Option<&'i FunctionSummary>,
) -> Option<String> {
    let syntax = &index.call_identity_memos.receiver_pin_syntax;
    let workspace = syntax.deref_workspace(index)?;
    if workspace.wrappers.is_empty() {
        return None;
    }
    let key = (file.to_path_buf(), start_line, name.to_string());
    if let Some(known) = syntax.deref_wrappers.borrow().get(&key) {
        return known.clone();
    }
    let wrapper = owner().and_then(|owner| owner_deref_wrapper(owner, index, workspace));
    syntax.deref_wrappers.borrow().insert(key, wrapper.clone());
    wrapper
}

/// The per-owner gates over the memoized workspace verdict.
fn owner_deref_wrapper(
    owner: &FunctionSummary,
    index: &RustIndex,
    workspace: &DerefWorkspace,
) -> Option<String> {
    let FunctionContainer::Inherent { self_ty } = &owner.item.container else {
        return None;
    };
    let method = owner.name.as_str();
    let wrapper = workspace.wrappers.get(self_ty.as_str())?;
    (is_plain_identifier(self_ty)
        && owner.item.has_self_param
        && owner.item.has_body
        && is_plain_identifier(method)
        && !DEREF_RECEIVER_METHODS.contains(&method)
        && !method_may_dispatch_through_prelude(method)
        && workspace.fn_definitions.get(method).copied() == Some(1)
        && first_param_is_shared_self(&owner.body, method)
        && workspace_type_name_count(self_ty, index) == 1
        && index
            .functions()
            .iter()
            .filter(|function| function.name == method)
            .count()
            == 1)
        .then(|| wrapper.clone())
}

/// The workspace verdict, or `None` when an owner-independent gate refuses
/// (or no file spells `Deref` at all, which skips the scan).
pub(super) fn deref_workspace(
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<DerefWorkspace> {
    // The textual `Deref` check is the cheap skip; the shared closed-sources
    // verdict is memoized for the run and shared with the `Box<dyn>` reading.
    if !index
        .files()
        .values()
        .any(|facts| facts.source.contains("Deref"))
        || !super::closed_dispatch::sources_closed(index, syntax)
    {
        return None;
    }
    let mut fn_definitions: BTreeMap<String, usize> = BTreeMap::new();
    let mut deref_impls: Vec<DerefImpl> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    for facts in index.files().values() {
        let masked = mask_comments_and_strings(&facts.source);
        for at in whole_word_offsets(&masked, "fn") {
            let after = masked[at + "fn".len()..].trim_start();
            let after = after.strip_prefix("r#").unwrap_or(after);
            let end = after
                .bytes()
                .take_while(|&byte| is_ident_byte(byte))
                .count();
            if end > 0 {
                *fn_definitions.entry(after[..end].to_string()).or_default() += 1;
            }
        }
        deref_impls.extend(file_deref_impls(&masked)?);
        files.push(masked);
    }
    // Last, because it walks the disk: every Rust file under the workspace
    // root must be one the index read, whatever the mode indexed.
    if !workspace_sources_indexed(index) {
        return None;
    }
    let mut wrappers = BTreeMap::new();
    // A workspace `impl Borrow` may make a std `Cow` field's deref run
    // workspace code, so the wrapper's projection would not hand the call
    // the value the test built.
    if files
        .iter()
        .any(|masked| impl_header_names(masked, "Borrow"))
    {
        return None;
    }
    let aliases: BTreeSet<String> = files
        .iter()
        .flat_map(|masked| alias_names(masked))
        .collect();
    for (position, item) in deref_impls.iter().enumerate() {
        let Some(target) = item.target.as_deref() else {
            continue;
        };
        let wrapper = &item.wrapper;
        let own_impls = deref_impls
            .iter()
            .filter(|other| other.deref && other.wrapper == *wrapper)
            .count();
        let tuple_wrapper = files
            .iter()
            .any(|masked| declares_tuple_struct(masked, wrapper));
        if no_other_deref_may_reach(&deref_impls, position, target, &aliases)
            && !aliases.contains(target)
            && !aliases.contains(wrapper)
            && !name_may_bind_outside_workspace(&files, target)
            && !name_may_bind_outside_workspace(&files, wrapper)
            && own_impls == 1
            && wrapper != target
            && files
                .iter()
                .all(|masked| wrapper_value_unshadowed(masked, wrapper, tuple_wrapper))
            && workspace_type_name_count(wrapper, index) == 1
            && !index
                .functions()
                .iter()
                .any(|function| function.name == *wrapper)
            && files
                .iter()
                .all(|masked| wrapper_spellings_inert(masked, wrapper))
            && files
                .iter()
                .filter(|masked| declares_plain_struct(masked, wrapper))
                .count()
                == 1
        {
            wrappers.insert(target.to_string(), wrapper.clone());
        }
    }
    Some(DerefWorkspace {
        wrappers,
        fn_definitions,
    })
}

/// Whether every Rust source the workspace walk finds under the index's
/// root is an indexed file. A mode that indexes part of the workspace, an
/// unreadable directory, a non-UTF-8 source or an index without a root
/// refuses: an unread file may hold an impl on the wrapper or a trait with
/// a blanket impl.
fn workspace_sources_indexed(index: &RustIndex) -> bool {
    let Some(authority) = index.workspace_authority.as_ref() else {
        return false;
    };
    let Ok((files, unreadable)) =
        crate::analysis::workspace::discover_rust_files_tolerating_unreadable(&authority.root)
    else {
        return false;
    };
    let indexed: BTreeSet<&Path> = index
        .files()
        .iter()
        .map(|(path, _)| path.strip_prefix("./").unwrap_or(path))
        .collect();
    unreadable.is_empty()
        && files
            .iter()
            .all(|file| indexed.contains(file.strip_prefix("./").unwrap_or(file)))
}

/// Whether no `Deref` impl other than `deref_impls[position]` may hand a
/// call `target`. Every other `Deref` impl must declare a plain-identifier
/// `type Target` that is neither `target`, `Target` or `Self`, nor a name
/// some `type` item declares or some `as` introduces (either may name
/// `target`). An impl whose `Target` cannot be
/// read refuses whatever its body: a deref that runs other code (`&OTHER`)
/// still reaches `target`, and the projection's field coerces through it.
fn no_other_deref_may_reach(
    deref_impls: &[DerefImpl],
    position: usize,
    target: &str,
    aliases: &BTreeSet<String>,
) -> bool {
    deref_impls
        .iter()
        .enumerate()
        .filter(|(other, item)| *other != position && item.deref)
        .all(|(_, item)| {
            item.declared.as_deref().is_some_and(|declared| {
                declared != target
                    && declared != "Target"
                    && declared != "Self"
                    && !aliases.contains(declared)
            })
        })
}

/// Every name one masked file may use for another type: each name a `type`
/// item declares (`Target` aside, the associated type every `Deref` impl
/// declares; a declared `Target` refuses on its own) and each name after
/// `as` (`use crate::Inner as Other;`, or a cast, which only refuses more).
fn alias_names(masked: &str) -> Vec<String> {
    let name_after = |keyword: &'static str| {
        whole_word_offsets(masked, keyword)
            .into_iter()
            .filter_map(move |at| {
                let rest = masked[at + keyword.len()..].trim_start();
                let rest = rest.strip_prefix("r#").unwrap_or(rest);
                let end = rest.bytes().take_while(|&byte| is_ident_byte(byte)).count();
                (end > 0).then(|| rest[..end].to_string())
            })
    };
    name_after("type")
        .filter(|name| name != "Target")
        .chain(name_after("as"))
        .collect()
}

/// Whether any `impl` header in one masked file spells `name` as a word.
fn impl_header_names(masked: &str, name: &str) -> bool {
    whole_word_offsets(masked, "impl").into_iter().any(|at| {
        let rest = &masked[at + "impl".len()..];
        let header = &rest[..rest.find(['{', ';']).unwrap_or(rest.len())];
        !whole_word_offsets(header, name).is_empty()
    })
}

/// The plain-identifier `type Target` a `Deref` impl body declares exactly
/// once, whatever its `fn deref` does; `None` for any other declaration.
fn declared_target(body: &str) -> Option<String> {
    let compact: String = body.split_whitespace().collect();
    let mut found = compact.match_indices("typeTarget=");
    let (at, _) = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let rest = &compact[at + "typeTarget=".len()..];
    let target = &rest[..rest.find(';')?];
    is_plain_identifier(target).then(|| target.to_string())
}

/// One `impl Deref for W` or `impl DerefMut for W` header the gates admit.
#[derive(Clone, Debug)]
struct DerefImpl {
    /// `Deref` rather than `DerefMut`.
    deref: bool,
    wrapper: String,
    /// The `type Target` of a `Deref` impl whose body passes
    /// [`deref_body_target`]; `None` for any other body.
    target: Option<String>,
    /// The plain `type Target` a `Deref` impl's body declares, whatever its
    /// `fn deref` does ([`declared_target`]); `None` when unreadable.
    declared: Option<String>,
}

/// Whether the first parameter of `fn method(..)` in `body` is exactly
/// `&self`.
fn first_param_is_shared_self(body: &str, method: &str) -> bool {
    let masked = mask_comments_and_strings(body);
    let Some(at) = whole_word_offsets(&masked, "fn").into_iter().find(|&at| {
        masked[at + "fn".len()..]
            .trim_start()
            .strip_prefix(method)
            .is_some_and(|rest| !rest.bytes().next().is_some_and(is_ident_byte))
    }) else {
        return false;
    };
    let Some(open) = masked[at..].find('(').map(|offset| at + offset) else {
        return false;
    };
    let Some(close) = matching_close(&masked, open, b'(', b')') else {
        return false;
    };
    let first = masked[open + 1..close].split(',').next().unwrap_or("");
    first.split_whitespace().collect::<String>() == "&self"
}

/// Whether `character` may be part of an identifier (Unicode identifiers
/// included, so a scan never stops inside a multi-byte character).
fn is_ident_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The byte offset where the trailing run of identifier characters (and
/// `extra`) of `text` starts, always a character boundary.
fn ident_run_start(text: &str, extra: &[char]) -> usize {
    text.char_indices()
        .rev()
        .find(|&(_, character)| !(is_ident_char(character) || extra.contains(&character)))
        .map_or(0, |(at, character)| at + character.len_utf8())
}

/// Every `Deref`/`DerefMut` impl header of one masked file, or `None` when
/// the file spells either name anywhere else: an import other than an exact
/// std one, a bound, a declaration, a rename, an attributed or generic
/// header, or a bare-named header without an exact std import in the file.
fn file_deref_impls(masked: &str) -> Option<Vec<DerefImpl>> {
    let spellings =
        whole_word_offsets(masked, "Deref").len() + whole_word_offsets(masked, "DerefMut").len();
    if spellings == 0 {
        return Some(Vec::new());
    }
    let mut accounted = 0usize;
    let mut imports_deref = false;
    let mut imports_deref_mut = false;
    for at in whole_word_offsets(masked, "use") {
        let rest = &masked[at..];
        let item = &rest[..rest.find(';').map_or(rest.len(), |end| end + 1)];
        let names = whole_word_offsets(item, "Deref").len();
        let mut_names = whole_word_offsets(item, "DerefMut").len();
        if names + mut_names == 0 {
            continue;
        }
        let compact: String = item.split_whitespace().collect();
        if !STD_DEREF_IMPORTS.contains(&compact.as_str()) {
            return None;
        }
        imports_deref |= names > 0;
        imports_deref_mut |= mut_names > 0;
        accounted += names + mut_names;
    }
    let mut impls = Vec::new();
    for at in whole_word_offsets(masked, "impl") {
        let rest = &masked[at + "impl".len()..];
        let header_end = rest.find(['{', ';']).unwrap_or(rest.len());
        let header = &rest[..header_end];
        let names = whole_word_offsets(header, "Deref").len();
        let mut_names = whole_word_offsets(header, "DerefMut").len();
        if names + mut_names == 0 {
            continue;
        }
        if names + mut_names != 1 || masked[..at].trim_end().ends_with(']') {
            return None;
        }
        let compact: String = header.split_whitespace().collect();
        let (trait_path, wrapper) = compact.split_once("for")?;
        let deref = names == 1;
        let name = if deref { "Deref" } else { "DerefMut" };
        let std_path = STD_PATH_PREFIXES
            .iter()
            .any(|prefix| trait_path.strip_prefix(prefix) == Some(name));
        let imported = trait_path == name
            && if deref {
                imports_deref
            } else {
                imports_deref_mut
            };
        // `for` must be the keyword, so the trait path ends right before it.
        if !(std_path || imported)
            || !is_plain_identifier(wrapper)
            || !header.split_whitespace().any(|word| word == "for")
        {
            return None;
        }
        let body = if deref && rest.as_bytes().get(header_end) == Some(&b'{') {
            let open = at + "impl".len() + header_end;
            matching_close(masked, open, b'{', b'}').map(|close| &masked[open + 1..close])
        } else {
            None
        };
        impls.push(DerefImpl {
            deref,
            wrapper: wrapper.to_string(),
            target: body.and_then(deref_body_target),
            declared: body.and_then(declared_target),
        });
        accounted += 1;
    }
    (accounted == spellings).then_some(impls)
}

/// The `Target` of a `Deref` impl body that is exactly `type Target = T;`
/// and `fn deref(&self) -> &T { &self.field }` (in either order, `&T` also
/// spelled `&Self::Target`), with `T` a plain identifier.
fn deref_body_target(body: &str) -> Option<String> {
    let compact: String = body.split_whitespace().collect();
    let type_end = compact.find(';')?;
    let (type_item, function) = if compact.starts_with("typeTarget=") {
        (&compact[..=type_end], &compact[type_end + 1..])
    } else {
        let type_at = compact.find("typeTarget=")?;
        if !compact[type_at..].ends_with(';') || compact[type_at..].matches(';').count() != 1 {
            return None;
        }
        (&compact[type_at..], &compact[..type_at])
    };
    let target = type_item
        .strip_prefix("typeTarget=")?
        .strip_suffix(';')?
        .to_string();
    if !is_plain_identifier(&target) {
        return None;
    }
    let rest = function.strip_prefix("fnderef(&self)->&")?;
    let rest = rest
        .strip_prefix(target.as_str())
        .or_else(|| rest.strip_prefix("Self::Target"))?;
    let field = rest.strip_prefix("{&self.")?.strip_suffix('}')?;
    (!field.is_empty() && field.bytes().all(is_ident_byte)).then_some(target)
}

/// Whether every spelling of `wrapper` in one masked file is inert for method
/// lookup on it: no `impl` header names it other than a `Deref`/`DerefMut`
/// one or a plain inherent `impl wrapper {`, no `type` item mentions it, it
/// is never renamed with `as`, and no macro other than [`OPERAND_MACROS`]
/// takes it as an argument.
fn wrapper_spellings_inert(masked: &str, wrapper: &str) -> bool {
    let spellings = whole_word_offsets(masked, wrapper);
    if spellings.is_empty() {
        return true;
    }
    let headers_inert = whole_word_offsets(masked, "impl").into_iter().all(|at| {
        let rest = &masked[at + "impl".len()..];
        let header = &rest[..rest.find(['{', ';']).unwrap_or(rest.len())];
        if whole_word_offsets(header, wrapper).is_empty() {
            return true;
        }
        let compact: String = header.split_whitespace().collect();
        compact == wrapper
            || (compact.ends_with(&format!("for{wrapper}"))
                && (!whole_word_offsets(header, "Deref").is_empty()
                    || !whole_word_offsets(header, "DerefMut").is_empty()))
    });
    let types_inert = whole_word_offsets(masked, "type").into_iter().all(|at| {
        let rest = &masked[at + "type".len()..];
        let item = &rest[..rest.find([';', '{']).unwrap_or(rest.len())];
        whole_word_offsets(item, wrapper).is_empty()
    });
    let renamed = spellings.iter().any(|&at| {
        masked[at + wrapper.len()..]
            .trim_start()
            .strip_prefix("as")
            .is_some_and(|rest| !rest.bytes().next().is_some_and(is_ident_byte))
    });
    headers_inert && types_inert && !renamed && !inside_item_macro(masked, &spellings)
}

/// Whether any of `offsets` lies inside the token tree of a macro call
/// (`name!(..)`, `name![..]`, `name! {..}`) that is not one of
/// [`OPERAND_MACROS`]. An unmatched bracket refuses.
fn inside_item_macro(masked: &str, offsets: &[usize]) -> bool {
    let bytes = masked.as_bytes();
    masked.match_indices('!').any(|(bang, _)| {
        let name_start = ident_run_start(&masked[..bang], &[]);
        let name = &masked[name_start..bang];
        if name.is_empty() || name.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        let after = &masked[bang + 1..];
        let open = bang + 1 + (after.len() - after.trim_start().len());
        let (open_byte, close_byte) = match bytes.get(open) {
            Some(b'(') => (b'(', b')'),
            Some(b'[') => (b'[', b']'),
            Some(b'{') => (b'{', b'}'),
            _ => return false,
        };
        if OPERAND_MACROS.contains(&name) {
            return false;
        }
        match matching_close(masked, open, open_byte, close_byte) {
            Some(close) => offsets.iter().any(|&at| open < at && at < close),
            None => true,
        }
    })
}

/// Whether the masked file declares `struct wrapper` as a non-generic struct
/// (tuple, record or unit, no `where`) whose only attributes are
/// `#[derive(..)]` lists of [`STD_DERIVES`].
fn declares_plain_struct(masked: &str, wrapper: &str) -> bool {
    whole_word_offsets(masked, "struct").into_iter().any(|at| {
        let after = masked[at + "struct".len()..].trim_start();
        let Some(rest) = after.strip_prefix(wrapper) else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with(['(', '{', ';']) {
            return false;
        }
        if rest.starts_with('(') {
            let open = masked.len() - rest.len();
            let Some(close) = matching_close(masked, open, b'(', b')') else {
                return false;
            };
            if !masked[close + 1..].trim_start().starts_with(';') {
                return false;
            }
        }
        let item_start = masked[..at].rfind([';', '{', '}']).map_or(0, |end| end + 1);
        attributes_are_std_derives(&masked[item_start..at])
    })
}

/// Std prelude and primitive type names: a `type Target = String;` names
/// the std type wherever no workspace `String` is in scope, whatever a
/// workspace type of that name declares.
const PRELUDE_TYPE_NAMES: &[&str] = &[
    "Box", "Option", "Result", "String", "Vec", "bool", "char", "f32", "f64", "i8", "i16", "i32",
    "i64", "i128", "isize", "str", "u8", "u16", "u32", "u64", "u128", "usize",
];

/// Whether `name` may denote a type from outside the workspace somewhere: a
/// std prelude or primitive name, or a `use` item not rooted at `crate`,
/// `self` or `super` that spells `name` or imports a glob
/// (`use std::string::String;`, `use std::collections::*;`). Such a name
/// may resolve to the std type in one module and to the workspace type in
/// another, so matching it to the workspace owner by name alone may credit
/// a call that runs std code.
fn name_may_bind_outside_workspace(files: &[String], name: &str) -> bool {
    PRELUDE_TYPE_NAMES.contains(&name)
        || files.iter().any(|masked| {
            whole_word_offsets(masked, "use").into_iter().any(|at| {
                let rest = &masked[at + "use".len()..];
                let item = &rest[..rest.find(';').unwrap_or(rest.len())];
                let compact: String = item.split_whitespace().collect();
                let local = ["crate::", "self::", "super::"]
                    .iter()
                    .any(|root| compact.starts_with(root));
                !local && (compact.contains('*') || !whole_word_offsets(item, name).is_empty())
            })
        })
}

/// Whether one masked file declares `wrapper` as a tuple struct
/// (`struct Guard(..)`), whose constructor owns the value namespace.
fn declares_tuple_struct(masked: &str, wrapper: &str) -> bool {
    whole_word_offsets(masked, "struct").into_iter().any(|at| {
        masked[at + "struct".len()..]
            .trim_start()
            .strip_prefix(wrapper)
            .is_some_and(|rest| rest.trim_start().starts_with('('))
    })
}

/// Whether no value item or binding of one masked file may answer a
/// `wrapper(..)` call in place of the wrapper's constructor: no `const`,
/// `static`, `fn`, `let` or `mut` spelling binds the name (a block item may
/// shadow a tuple constructor). A wrapper that is not a tuple struct has no
/// constructor call at all, so any `wrapper(` outside its `struct` item
/// calls some other value and refuses.
fn wrapper_value_unshadowed(masked: &str, wrapper: &str, tuple_wrapper: bool) -> bool {
    whole_word_offsets(masked, wrapper).into_iter().all(|at| {
        let before = &masked[..at];
        let before = before.strip_suffix("r#").unwrap_or(before).trim_end();
        let previous = &before[ident_run_start(before, &[])..];
        if ["const", "static", "fn", "let", "mut"].contains(&previous) {
            return false;
        }
        tuple_wrapper
            || previous == "struct"
            || !masked[at + wrapper.len()..].trim_start().starts_with('(')
    })
}

/// Whether the text before a `struct` keyword holds only a visibility and
/// `#[derive(..)]` attributes of [`STD_DERIVES`].
fn attributes_are_std_derives(prefix: &str) -> bool {
    let mut compact: String = prefix.split_whitespace().collect();
    for visibility in ["pub(crate)", "pub(super)", "pub(self)", "pub"] {
        if let Some(rest) = compact.strip_suffix(visibility) {
            compact = rest.to_string();
            break;
        }
    }
    let mut rest = compact.as_str();
    while !rest.is_empty() {
        let Some(list) = rest.strip_prefix("#[derive(") else {
            return false;
        };
        let Some(end) = list.find(")]") else {
            return false;
        };
        if !list[..end]
            .split(',')
            .filter(|name| !name.is_empty())
            .all(|name| STD_DERIVES.contains(&name))
        {
            return false;
        }
        rest = &list[end + 2..];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        DerefImpl, alias_names, attributes_are_std_derives, declares_tuple_struct,
        deref_body_target, file_deref_impls, first_param_is_shared_self, ident_run_start,
        impl_header_names, inside_item_macro, name_may_bind_outside_workspace,
        no_other_deref_may_reach, wrapper_spellings_inert, wrapper_value_unshadowed,
    };
    use std::collections::BTreeSet;

    fn deref_impls(text: &str) -> Vec<DerefImpl> {
        file_deref_impls(text).unwrap_or_default()
    }

    #[test]
    fn any_other_deref_that_may_reach_the_target_refuses() {
        let none = BTreeSet::new();
        let admitted = |text: &str, aliases: &BTreeSet<String>| {
            let impls = deref_impls(text);
            let position = impls
                .iter()
                .position(|item| item.wrapper == "Guard")
                .unwrap_or(usize::MAX);
            impls.get(position).and_then(|item| item.target.as_deref()) == Some("Inner")
                && no_other_deref_may_reach(&impls, position, "Inner", aliases)
        };
        let guard = "use std::ops::Deref; \
            impl Deref for Guard { type Target = Inner; fn deref(&self) -> &Inner { &self.0 } } ";
        assert!(admitted(guard, &none));
        assert!(admitted(
            &format!(
                "{guard} impl Deref for Mid {{ type Target = Other; fn deref(&self) -> &Other {{ &OTHER }} }}"
            ),
            &none
        ));
        for other in [
            // A deref that runs other code still hands the call an `Inner`.
            "impl Deref for Mid { type Target = Inner; fn deref(&self) -> &Inner { &OTHER } }",
            "impl Deref for Mid { type Target = Inner; fn deref(&self) -> &Inner { &self.0 } }",
            "impl Deref for Mid { type Target = crate::Inner; fn deref(&self) -> &crate::Inner { &self.0 } }",
            "impl Deref for Mid { #[allow(x)] fn deref(&self) -> &Self::Target { &self.0 } type Target = Inner; type Target = Inner; }",
            "impl Deref for Mid;",
        ] {
            assert!(!admitted(&format!("{guard} {other}"), &none), "{other}");
        }
        let alias = format!(
            "{guard} impl Deref for Mid {{ type Target = Alias; fn deref(&self) -> &Alias {{ &OTHER }} }}"
        );
        let aliases: BTreeSet<String> = alias_names("type Alias = Inner;").into_iter().collect();
        assert!(!admitted(&alias, &aliases));
        let renamed = format!(
            "{guard} impl Deref for Mid {{ type Target = Other; fn deref(&self) -> &Other {{ &OTHER }} }}"
        );
        let aliases: BTreeSet<String> = alias_names("use crate::Inner as Other;")
            .into_iter()
            .collect();
        assert!(!admitted(&renamed, &aliases));
        let target_alias = format!(
            "{guard} impl Deref for Mid {{ type Target = Target; fn deref(&self) -> &Target {{ &OTHER }} }}"
        );
        assert!(!admitted(&target_alias, &none));
        assert_eq!(
            alias_names("type Target = Inner; type r#A = B; let n = x as u32;"),
            ["A", "u32"]
        );
        assert!(impl_header_names(
            "impl Borrow<Inner> for Other { }",
            "Borrow"
        ));
        assert!(!impl_header_names(
            "fn f(x: &impl Display) { x.borrow() }",
            "Borrow"
        ));
        assert!(declares_tuple_struct(
            "pub struct Guard(pub Inner);",
            "Guard"
        ));
        assert!(!declares_tuple_struct(
            "struct Guard { inner: Inner }",
            "Guard"
        ));
        let call = "fn t() { assert_eq!(Guard(Inner { cap: 3 }).limit(), 7); }";
        assert!(wrapper_value_unshadowed(
            &format!("struct Guard(Inner); {call}"),
            "Guard",
            true
        ));
        assert!(wrapper_value_unshadowed(
            "struct Guard { inner: Inner } let g = Guard { inner };",
            "Guard",
            false
        ));
        for text in [
            "const Guard: fn(Inner) -> [u8; 7] = unrelated;",
            "pub static Guard: u8 = 0;",
            "fn t() { fn Guard(i: Inner) -> Other { todo() } }",
            "let mut Guard = |i| i;",
            "mod tests { use super::*; const r#Guard: fn(Inner) -> u8 = f; }",
        ] {
            assert!(!wrapper_value_unshadowed(text, "Guard", true), "{text}");
        }
        assert!(!wrapper_value_unshadowed(
            &format!("struct Guard {{ inner: Inner }} {call}"),
            "Guard",
            false
        ));
    }

    #[test]
    fn a_name_a_non_workspace_import_or_the_prelude_may_bind_refuses() {
        let files =
            |texts: &[&str]| -> Vec<String> { texts.iter().map(|text| text.to_string()).collect() };
        let owner = "use std::ops::Deref; use super::*; use crate::model::Inner; struct Inner;";
        assert!(!name_may_bind_outside_workspace(&files(&[owner]), "Inner"));
        for text in [
            "use other::Inner;",
            "pub use other_crate::{Inner, Thing};",
            "use std::collections::*;",
        ] {
            assert!(
                name_may_bind_outside_workspace(&files(&[owner, text]), "Inner"),
                "{text}"
            );
        }
        assert!(name_may_bind_outside_workspace(
            &files(&["use std::string::String; struct Guard(String);"]),
            "String"
        ));
        assert!(name_may_bind_outside_workspace(&files(&[owner]), "Vec"));
    }

    #[test]
    fn a_non_ascii_identifier_before_a_bang_is_scanned_by_character() {
        let source = "pub fn größe_ok(größe: u32) -> bool { größe!=0 }";
        assert!(!inside_item_macro(source, &[source.len() - 2]));
        assert_eq!(ident_run_start("let x = größe", &[]), "let x = ".len());
        assert_eq!(ident_run_start("é::größe", &[':']), 0);
        let foreign = "fn f() { äxt::make!(Guard) }";
        assert!(inside_item_macro(foreign, &[foreign.len() - 7]));
    }

    #[test]
    fn only_a_field_projection_body_names_a_target() {
        for body in [
            " type Target = Inner; fn deref(&self) -> &Inner { &self.0 } ",
            " fn deref(&self) -> &Self::Target { &self.inner } type Target = Inner; ",
        ] {
            assert_eq!(deref_body_target(body).as_deref(), Some("Inner"), "{body}");
        }
        for body in [
            " type Target = Inner; fn deref(&self) -> &Inner { log(); &self.0 } ",
            " type Target = Inner; fn deref(&self) -> &Inner { &OTHER } ",
            " type Target = crate::Inner; fn deref(&self) -> &Inner { &self.0 } ",
            " type Target = Box<Inner>; fn deref(&self) -> &Box<Inner> { &self.0 } ",
            " type Target = Inner; fn deref(&self) -> &Inner { &self.0.inner } ",
        ] {
            assert_eq!(deref_body_target(body), None, "{body}");
        }
    }

    #[test]
    fn deref_spellings_must_be_std_imports_or_headers() {
        let impls = |text: &str| file_deref_impls(text).map(|found| found.len());
        assert_eq!(
            impls("use std::ops::Deref; impl Deref for Guard { }"),
            Some(1)
        );
        assert_eq!(impls("impl std::ops::Deref for Guard { }"), Some(1));
        assert_eq!(impls("fn f() {}"), Some(0));
        for text in [
            "impl Deref for Guard { }",
            "use std::ops::Deref as D; impl D for Guard { }",
            "use crate::Deref; impl Deref for Guard { }",
            "trait Deref {} impl Deref for Guard { }",
            "use std::ops::Deref; impl<T> Deref for Guard<T> { }",
            "use std::ops::Deref; #[cfg(test)] impl Deref for Guard { }",
            "use std::ops::Deref; fn f<T: Deref>(t: T) {}",
            "use std::ops::*; impl Deref for Guard { }",
        ] {
            assert_eq!(impls(text), None, "{text}");
        }
    }

    #[test]
    fn a_wrapper_spelling_outside_inert_positions_refuses() {
        for text in [
            "struct Guard(Inner); impl Guard { } fn t() { assert_eq!(Guard(x).limit(), 7); }",
            "impl std::ops::Deref for Guard { } let g: Guard = Guard(x);",
        ] {
            assert!(wrapper_spellings_inert(text, "Guard"), "{text}");
        }
        for text in [
            "impl Display for Guard { }",
            "impl Ext for &Guard { }",
            "type G = Guard;",
            "use crate::Guard as G;",
            "make_limit!(Guard);",
            "impl<T> Ext<Guard> for T { }",
        ] {
            assert!(!wrapper_spellings_inert(text, "Guard"), "{text}");
        }
        assert!(!inside_item_macro("assert!(Guard(x).ok())", &[8]));
    }

    #[test]
    fn only_a_shared_self_receiver_and_std_derives_pass() {
        assert!(first_param_is_shared_self(
            "pub fn limit(&self) -> u32 { 1 }",
            "limit"
        ));
        assert!(first_param_is_shared_self(
            "fn limit( & self, n: u32) {}",
            "limit"
        ));
        for body in [
            "fn limit(&mut self) {}",
            "fn limit(self) {}",
            "fn limit(self: &Self) {}",
            "fn limit(&'a self) {}",
        ] {
            assert!(!first_param_is_shared_self(body, "limit"), "{body}");
        }
        assert!(attributes_are_std_derives(" #[derive(Debug, Clone)] pub "));
        assert!(attributes_are_std_derives(""));
        assert!(!attributes_are_std_derives(" #[derive(Debug, Limits)] "));
        assert!(!attributes_are_std_derives(" #[my_attr] "));
    }
}
