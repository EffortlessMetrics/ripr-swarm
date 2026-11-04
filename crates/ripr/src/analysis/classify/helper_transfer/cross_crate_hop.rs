//! A hop whose call is spelled through another workspace crate (#11401).
//!
//! `app::checkout` calls `wscore::discount(price)`: the call is a path
//! qualified invocation, and neither the bare name nor the spelled path alone
//! is callee identity. The hop binds here only when the manifests say that
//! the caller's package depends, in its `[dependencies]`, on the package that
//! holds the unique callee, under the callee crate's own name.
//!
//! Fail-closed admit, all of:
//!
//! - the callee is the one function of its name in the workspace (the chain
//!   resolver checks this first), a body-carrying free `pub fn` declared at
//!   the top level of its package's library root file or in a chain of public
//!   modules below it, so the path `crate_name::callee` (or
//!   `crate_name::module::callee`) names it ([`super::cross_crate_path`]);
//! - the manifests give exactly one name for the callee's library in the
//!   caller's package: a `[dependencies]` path entry (never a
//!   `[dev-dependencies]` or `[build-dependencies]` one, which library code
//!   cannot see), keyed by the callee's library identifier or renamed with
//!   `package = ".."` (the key is then the crate name), between two
//!   different packages;
//! - the caller mentions the callee name exactly once, in one call fact, as
//!   `<crate_name>::<modules>::<callee>(..)` with exactly those qualifier
//!   segments;
//! - nothing in the caller's package can rebind `<crate_name>`: no import,
//!   alias, glob, block-local item or module of that name, and no item that
//!   may be spliced in by a macro or an `include`;
//! - the caller invokes no macro that could hide an early return or a
//!   rebinding.
//!
//! A bare call after the caller's file imports the callee by name
//! (`use wscore::discount;`, `use wscore::{discount, ..}`, with the callee's
//! public modules in the path) binds the same way
//! under [`cross_crate_imported_free_hop`]; a glob import or an alias does
//! not. A dependency renamed with `package = ".."` is named by its key.
//!
//! The hop carries reach and argument bindings exactly like a same-crate
//! direct call hop; every later rule that decides whether a result is
//! forwarded still reads the caller body.

use super::{HelperHop, split_call_arguments_at_paren};
use crate::analysis::classify::{call_identity, related_tests};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, RustIndex};

/// What both call spellings need before the call shape is read: the unique
/// callee, the one manifest name for its library, and the caller's source.
struct Admitted {
    crate_name: String,
    /// The public modules between the crate name and the callee.
    modules: Vec<String>,
    source: String,
}

impl Admitted {
    /// The path the callee is spelled through: `wscore` or `wscore::pricing`.
    fn qualifier(&self) -> String {
        std::iter::once(self.crate_name.as_str())
            .chain(self.modules.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::")
    }
}

fn admit(caller: &FunctionSummary, callee_name: &str, index: &RustIndex) -> Option<Admitted> {
    let mut candidates = index
        .functions()
        .iter()
        .filter(|function| function.name == callee_name);
    let callee = candidates.next()?;
    if candidates.next().is_some()
        || callee.item.container != FunctionContainer::Free
        || !callee.item.has_body
        || callee.file == caller.file
        || !index.member_crates.has_manifest_root()
    {
        return None;
    }
    if related_tests::path_is_absolute_form(&callee.file)
        || related_tests::path_is_absolute_form(&caller.file)
        || call_identity::under_tests_dir(&callee.file)
        || call_identity::under_tests_dir(&caller.file)
        || caller.source_role.registers_executable_test()
    {
        return None;
    }
    let source = index.files().get(&caller.file)?.source.to_string();
    let public = super::cross_crate_path::public_path(index, callee)?;
    let crate_name = index
        .member_crates
        .library_dependency_name(&caller.file, &public.root)?;
    Some(Admitted {
        crate_name,
        modules: public.modules,
        source,
    })
}

pub(super) fn cross_crate_free_hop(
    caller: &FunctionSummary,
    callee_name: &str,
    index: &RustIndex,
) -> Option<HelperHop> {
    let admitted = admit(caller, callee_name, index)?;
    let qualifier = admitted.qualifier();
    let Admitted {
        crate_name, source, ..
    } = admitted;

    let mut mentions = caller.calls.iter().filter(|call| call.name == callee_name);
    let call = mentions.next()?;
    if mentions.next().is_some() {
        return None;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&call.text);
    let paren = sole_crate_qualified_call(&masked, &qualifier, callee_name)?;
    if super::caller_invokes_opaque_macro(&caller.body, true) {
        return None;
    }
    if super::qualified_hop::namespace_may_rebind(&source, &crate_name)
        || package_declares_name(index, caller, &crate_name)
    {
        return None;
    }
    let arguments = split_call_arguments_at_paren(&call.text, paren)?;
    Some(HelperHop {
        caller: caller.clone(),
        call_text: call.text.to_string(),
        arguments,
        receiver: None,
        through_macro: false,
        module_qualified: true,
        crate_qualifier: Some(qualifier),
        transparent_macros_rebound: super::transparent_macro_name_rebound(&caller.body, index),
        diverging_handlers: Vec::new(),
    })
}

/// The same hop for a call spelled by its bare name after the caller's file
/// imports it by name (`use wscore::discount;` or `use wscore::{discount, ..}`,
/// #11401). `sites` are the direct call sites of the bare name in the caller.
///
/// On top of the shared admission this needs, all of:
///
/// - the caller is a top-level function of its file and the bare name is
///   mentioned exactly once in its body, in one direct call;
/// - exactly one `use` in the file mentions the callee name; it is a plain,
///   top-level, attribute-free and visibility-free `use` whose path is
///   `<crate>::<callee>` or whose list holds the bare `callee` under
///   `<crate>`: no alias, no glob form of the name, no other import of it;
/// - no other item, binding or parameter in the file is named like the
///   callee, no `extern crate`, and nothing can be spliced in by a macro or
///   an `include`;
/// - nothing in the caller's package can rebind `<crate>` (`mod <crate>`).
///
/// A glob import (`use wscore::*;`) never names the callee and is not followed.
pub(super) fn cross_crate_imported_free_hop(
    caller: &FunctionSummary,
    callee_name: &str,
    index: &RustIndex,
    sites: &[(String, Vec<String>)],
) -> Option<HelperHop> {
    let [(call_text, arguments)] = sites else {
        return None;
    };
    let admitted = admit(caller, callee_name, index)?;
    let qualifier = admitted.qualifier();
    let Admitted {
        crate_name,
        modules,
        source,
    } = admitted;
    let mut mentions = caller.calls.iter().filter(|call| call.name == callee_name);
    mentions.next()?;
    if mentions.next().is_some()
        || word_count(
            &crate::analysis::language::mask_rust_comments_and_strings(&caller.body),
            callee_name,
        ) != 1
        || !starts_at_file_root(&source, caller.start_line)
        || super::caller_invokes_opaque_macro(&caller.body, true)
        || package_declares_name(index, caller, &crate_name)
        || !file_imports_only_by_name(&source, &crate_name, &modules, callee_name)
    {
        return None;
    }
    Some(HelperHop {
        caller: caller.clone(),
        call_text: call_text.clone(),
        arguments: arguments.clone(),
        receiver: None,
        through_macro: false,
        module_qualified: true,
        crate_qualifier: Some(qualifier),
        transparent_macros_rebound: super::transparent_macro_name_rebound(&caller.body, index),
        diverging_handlers: Vec::new(),
    })
}

fn word_count(masked: &str, word: &str) -> usize {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || !ch.is_ascii();
    masked
        .match_indices(word)
        .filter(|(at, _)| {
            !masked[..*at].chars().next_back().is_some_and(is_word)
                && !masked[at + word.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_word)
        })
        .count()
}

/// Whether the item starting at 1-based `start_line` sits outside every
/// block of the file.
fn starts_at_file_root(source: &str, start_line: usize) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(source);
    masked
        .lines()
        .take(start_line.saturating_sub(1))
        .flat_map(str::chars)
        .fold(0usize, |depth, ch| match ch {
            '{' => depth + 1,
            '}' => depth.saturating_sub(1),
            _ => depth,
        })
        == 0
}

/// Whether `source` brings `callee` into scope only through one plain
/// top-level `use` of `<crate_name>::<modules>::<callee>`, and declares nothing
/// that can give the callee or the crate name another meaning.
///
/// Every other `use` is read for the crate name too: one that is a glob or a
/// rename, or that mentions the crate name anywhere but as the first segment
/// of its own path (`use legacy as wscore;`, `use crate::shim::wscore;`), can
/// rebind it. A plain `use wscore::other;` beside the import does not.
fn file_imports_only_by_name(
    source: &str,
    crate_name: &str,
    modules: &[String],
    callee: &str,
) -> bool {
    use ra_ap_syntax::{AstNode, SyntaxKind, ast, ast::HasAttrs, ast::HasName, ast::HasVisibility};
    if call_identity::source_may_splice_items(source, true) {
        return false;
    }
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
        return false;
    };
    let tree = parse.tree();
    let ident_is = |token: &ra_ap_syntax::SyntaxToken, text: &str| {
        token.kind() == SyntaxKind::IDENT && token.text().trim_start_matches("r#") == text
    };
    let named = |token: &ra_ap_syntax::SyntaxToken| ident_is(token, callee);
    let named_crate = |token: &ra_ap_syntax::SyntaxToken| ident_is(token, crate_name);
    let squeeze = |text: String| text.split_whitespace().collect::<String>();
    let mut importing = Vec::new();
    let mut other_uses = Vec::new();
    for node in tree.syntax().descendants() {
        if ast::ExternCrate::can_cast(node.kind()) {
            return false;
        }
        if let Some(item) = ast::Use::cast(node.clone()) {
            if item
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| named(&token))
            {
                importing.push(item);
            } else {
                other_uses.push(item);
            }
            continue;
        }
        if node
            .ancestors()
            .skip(1)
            .any(|up| ast::Use::can_cast(up.kind()))
        {
            continue;
        }
        // A field name is not a declaration in any namespace a call reads.
        if ast::RecordField::can_cast(node.kind()) {
            continue;
        }
        let declares = ast::AnyHasName::cast(node)
            .and_then(|item| item.name())
            .is_some_and(|name| {
                name.syntax()
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .any(|token| named(&token) || named_crate(&token))
            });
        if declares {
            return false;
        }
    }
    let [item] = importing.as_slice() else {
        return false;
    };
    // The crate name may open a `use` path but never be bound by one: no
    // glob, and no mention of it but the first token of the path (an alias,
    // a list entry or a longer path ending in it would bind it again).
    let only_opens_with_crate = |other: &ast::Use| {
        let tokens: Vec<_> = other
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .collect();
        let mentions = tokens.iter().filter(|token| named_crate(token)).count();
        tokens.iter().all(|token| token.kind() != SyntaxKind::STAR)
            && (mentions == 0
                || mentions == 1
                    && other
                        .use_tree()
                        .and_then(|tree| tree.syntax().first_token())
                        .is_some_and(|first| named_crate(&first)))
    };
    if !other_uses.iter().all(only_opens_with_crate) || !only_opens_with_crate(item) {
        return false;
    }
    if item.attrs().next().is_some()
        || item.visibility().is_some()
        || item
            .syntax()
            .parent()
            .is_none_or(|parent| parent.kind() != SyntaxKind::SOURCE_FILE)
    {
        return false;
    }
    let Some(use_tree) = item.use_tree() else {
        return false;
    };
    if use_tree.rename().is_some() || use_tree.star_token().is_some() {
        return false;
    }
    let Some(path) = use_tree
        .path()
        .map(|path| squeeze(path.syntax().text().to_string()))
    else {
        return false;
    };
    let qualifier = std::iter::once(crate_name)
        .chain(modules.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::");
    match use_tree.use_tree_list() {
        None => path == format!("{qualifier}::{callee}"),
        Some(list) => {
            path == qualifier
                && list.use_trees().any(|entry| {
                    entry.rename().is_none()
                        && entry.star_token().is_none()
                        && entry.use_tree_list().is_none()
                        && entry
                            .path()
                            .map(|path| squeeze(path.syntax().text().to_string()))
                            .is_some_and(|path| path == callee)
                })
        }
    }
}

/// The byte index of the argument-list paren of the only mention of `callee`
/// in `masked`, when that mention is exactly `<qualifier>::<callee>(` (the
/// qualifier is the crate name and any public modules) with nothing before it
/// that makes it a path continuation, a method call or a generic argument.
fn sole_crate_qualified_call(masked: &str, qualifier: &str, callee: &str) -> Option<usize> {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || !ch.is_ascii();
    let mut found = None;
    for (at, _) in masked.match_indices(callee) {
        if masked[..at].chars().next_back().is_some_and(is_word)
            || masked[at + callee.len()..]
                .chars()
                .next()
                .is_some_and(is_word)
        {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(at);
    }
    let at = found?;
    if !masked[at + callee.len()..].starts_with('(') {
        return None;
    }
    let head = masked[..at].strip_suffix("::")?;
    let before_name = head.strip_suffix(qualifier)?;
    if before_name.chars().next_back().is_some_and(is_word)
        || before_name.ends_with([':', '.', '>', '<', '#'])
    {
        return None;
    }
    Some(at + callee.len())
}

/// Whether any file of the caller's package declares a module, function or
/// other item that could give `name` a different meaning than the extern
/// crate: a conservative scan of the masked sources for `mod <name>`.
fn package_declares_name(index: &RustIndex, caller: &FunctionSummary, name: &str) -> bool {
    let package = related_tests::package_prefix(&caller.file);
    index.files().iter().any(|(path, facts)| {
        if related_tests::package_prefix(path) != package {
            return false;
        }
        let masked = crate::analysis::extract::mask_comments_and_strings(&facts.source);
        masked
            .match_indices(name)
            .any(|(at, _)| names_a_module(&masked, at, name.len()))
    })
}

/// Whether the identifier at `[at, at + len)` follows the `mod` keyword.
fn names_a_module(masked: &str, at: usize, len: usize) -> bool {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_' || !ch.is_ascii();
    if masked[at + len..].chars().next().is_some_and(is_word) {
        return false;
    }
    let before = masked[..at].trim_end();
    before
        .strip_suffix("mod")
        .is_some_and(|rest| !rest.chars().next_back().is_some_and(is_word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_sole_call_spelled_through_the_crate_name_binds() {
        let call = |text: &str| sole_crate_qualified_call(text, "wscore", "discount");
        assert_eq!(call("wscore::discount(price)"), Some(16));
        assert_eq!(call("    wscore::discount(price)"), Some(20));
        let nested = |text: &str| sole_crate_qualified_call(text, "wscore::pricing", "discount");
        assert_eq!(nested("wscore::pricing::discount(p)"), Some(25));
        assert_eq!(nested("wscore::discount(p)"), None);
        assert_eq!(nested("wscore::other::discount(p)"), None);
        assert_eq!(nested("app::wscore::pricing::discount(p)"), None);
        assert_eq!(nested("pricing::discount(p)"), None);
        for (label, text) in [
            ("bare", "discount(price)"),
            ("method", "x.discount(price)"),
            ("other crate", "legacy::discount(price)"),
            ("longer path", "app::wscore::discount(price)"),
            ("leading colons", "::wscore::discount(price)"),
            ("prefixed name", "xwscore::discount(price)"),
            ("generic argument", "Vec<wscore::discount(price)"),
            ("two calls", "wscore::discount(a) + wscore::discount(b)"),
            ("not called", "wscore::discount"),
            ("longer callee", "wscore::discounted(price)"),
        ] {
            assert_eq!(call(text), None, "{label}");
        }
    }

    #[test]
    fn a_module_named_like_the_crate_is_seen_but_a_longer_name_is_not() {
        let named = |source: &str| {
            source
                .match_indices("wscore")
                .any(|(at, _)| names_a_module(source, at, "wscore".len()))
        };
        assert!(named("mod wscore {}"));
        assert!(named("pub mod wscore;"));
        assert!(!named("mod wscore_ext {}"));
        assert!(!named("let wscore = 1;"));
        assert!(!named("module wscore"));
    }

    #[test]
    fn only_one_plain_top_level_use_by_name_imports_the_callee() {
        let imports = |source: &str| file_imports_only_by_name(source, "wscore", &[], "discount");
        let nested = |source: &str| {
            file_imports_only_by_name(source, "wscore", &["pricing".to_string()], "discount")
        };
        let call = "pub fn checkout(p: u32) -> u32 { discount(p) }\n";
        for source in [
            "use wscore::discount;\n",
            "use wscore::{discount, other};\n",
            "use wscore::{self, discount};\n",
            "use wscore::{\n    other,\n    discount,\n};\n",
            "use wscore::other;\nuse wscore::discount;\n",
            "use wscore::{discount, other::thing};\n",
            "use wscore::{discount, x as y};\nuse wscore::more::{a, b};\n",
            "struct Rates {\n    discount: u32,\n}\nuse wscore::discount;\n",
        ] {
            assert!(imports(&format!("{source}{call}")), "{source}");
        }
        for source in [
            "use wscore::pricing::discount;\n",
            "use wscore::pricing::{discount, other};\n",
        ] {
            assert!(nested(&format!("{source}{call}")), "{source}");
        }
        for source in [
            "use wscore::discount;\n",
            "use wscore::other::discount;\n",
            "use wscore::pricing::deeper::discount;\n",
            "use wscore::{pricing::discount};\n",
            "use wscore::pricing::discount as d;\n",
        ] {
            assert!(!nested(&format!("{source}{call}")), "{source}");
        }
        for (label, source) in [
            ("none", String::new()),
            ("glob", "use wscore::*;\n".to_string()),
            ("alias", "use wscore::discount as d;\n".to_string()),
            (
                "aliased in list",
                "use wscore::{discount as d};\n".to_string(),
            ),
            ("other crate", "use legacy::discount;\n".to_string()),
            ("submodule", "use wscore::pricing::discount;\n".to_string()),
            (
                "rebind in the group",
                "use wscore::{discount, inner as wscore};\n".to_string(),
            ),
            (
                "rebind by alias",
                "use legacy as wscore;\nuse wscore::discount;\n".to_string(),
            ),
            (
                "rebind by path",
                "use crate::shim::wscore;\nuse wscore::discount;\n".to_string(),
            ),
            (
                "rebind by self path",
                "use self::wscore;\nuse wscore::discount;\n".to_string(),
            ),
            (
                "rebind by renamed sibling",
                "use wscore::inner as wscore;\nuse wscore::discount;\n".to_string(),
            ),
            (
                "glob beside",
                "use other::*;\nuse wscore::discount;\n".to_string(),
            ),
            (
                "struct of the crate name",
                "struct wscore;\nuse wscore::discount;\n".to_string(),
            ),
            ("leading colons", "use ::wscore::discount;\n".to_string()),
            ("visible", "pub use wscore::discount;\n".to_string()),
            ("cfg", "#[cfg(test)]\nuse wscore::discount;\n".to_string()),
            (
                "twice",
                "use wscore::discount;\nuse legacy::discount;\n".to_string(),
            ),
            (
                "inside a function",
                "fn f() {\n    use wscore::discount;\n}\n".to_string(),
            ),
            (
                "inside a module",
                "mod m {\n    use wscore::discount;\n}\n".to_string(),
            ),
            (
                "local function",
                "use wscore::discount;\nfn discount() {}\n".to_string(),
            ),
            (
                "local binding",
                "use wscore::discount;\nfn f() { let discount = 1; }\n".to_string(),
            ),
            (
                "extern crate",
                "extern crate wscore;\nuse wscore::discount;\n".to_string(),
            ),
        ] {
            assert!(!imports(&format!("{source}{call}")), "{label}");
        }
    }

    #[test]
    fn a_word_is_counted_only_as_a_whole_identifier() {
        assert_eq!(word_count("discount(a) + discounted(b)", "discount"), 1);
        assert_eq!(word_count("a.discount(b) + discount(c)", "discount"), 2);
        assert_eq!(word_count("x_discount(1)", "discount"), 0);
    }

    #[test]
    fn only_an_item_outside_every_block_starts_at_the_file_root() {
        let source = "fn a() {\n}\nmod m {\n    fn b() {}\n}\n";
        assert!(starts_at_file_root(source, 1));
        assert!(starts_at_file_root(source, 3));
        assert!(!starts_at_file_root(source, 4));
    }
}
