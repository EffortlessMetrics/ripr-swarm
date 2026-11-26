//! #10357: path-qualified audited macros in a helper wrapper.
//!
//! Each audited macro expands to a value built from its arguments, with no
//! `return`, `break`, `continue` or `?` of its own, so the wrapper keeps the
//! helper result the return scan reads. The expansions were read in the
//! published sources of the audited releases, which live in the drop-in
//! authority's `AUDITED_LOGGING_VERSIONS` and `AUDITED_VALUE_MACRO_VERSIONS`:
//! - `tracing::info!(..)` and `log::warn!(..)` (and the other leveled
//!   macros) build an event, from `src/macros.rs` of tracing and log;
//! - `anyhow::anyhow!(..)` builds an `anyhow::Error`, from anyhow's
//!   `src/macros.rs` (`bail!` and `ensure!` return, so they are not audited);
//! - `serde_json::json!(..)` builds a `serde_json::Value` through
//!   `json_internal!`, from serde_json's `src/macros.rs`.
//!
//! Such a path is trusted only when its root is that crate. That needs three
//! things:
//! - the drop-in manifest authority verifies the caller's file: a plain
//!   registry dependency, locked at an audited version;
//! - the caller's file has no `use` that binds the root name to anything
//!   else, no module of that name, no item-position macro call that could
//!   emit one, and no glob import that could bring one in, and no indexed
//!   file renames a crate to that name (`extern crate self as tracing;` at a
//!   crate root reaches every file of the crate);
//! - the macro's arguments do not mention the callee. A logging macro
//!   evaluates its arguments only when its level is enabled, so a call in
//!   them may not run, and in any audited macro a call there is not the
//!   call whose result the wrapper forwards.
//!
//! Anything else keeps the path opaque, as before.

use ra_ap_syntax::{AstNode, SyntaxKind, ast, ast::HasName};

use crate::analysis::facts::{FunctionSummary, RustIndex};

/// The leveled macros tracing and log each export under these names.
const LEVELED_LOGGING_MACROS: &[&str] = &["trace", "debug", "info", "warn", "error"];

/// The crate roots with audited macros, and the macros audited under each.
pub(super) const AUDITED_MACRO_ROOTS: &[(&str, &[&str])] = &[
    ("tracing", LEVELED_LOGGING_MACROS),
    ("log", LEVELED_LOGGING_MACROS),
    ("anyhow", &["anyhow"]),
    ("serde_json", &["json"]),
];

/// The [`AUDITED_MACRO_ROOTS`] that `caller` spells as a macro path root
/// and that name the registry crate in its file. Empty for a caller that
/// spells none, without reading any manifest.
pub(super) fn verified_macro_roots(
    caller: &FunctionSummary,
    index: &RustIndex,
) -> Vec<&'static str> {
    let body: &str = &caller.body;
    let spelled: Vec<&'static str> = AUDITED_MACRO_ROOTS
        .iter()
        .map(|(root, _)| *root)
        .filter(|root| body.contains(&format!("{root}::")))
        .collect();
    if spelled.is_empty() {
        return spelled;
    }
    let Some(file) = index.files().get(&caller.file) else {
        return Vec::new();
    };
    let source = file.source.clone();
    let parsed = ast::SourceFile::parse(&source, ra_ap_syntax::Edition::CURRENT).tree();
    spelled
        .into_iter()
        .filter(|root| {
            !root_may_be_rebound(parsed.syntax(), root)
                && !extern_crate_alias_anywhere(index, root)
                && index.drop_in_manifests.verified(&caller.file, root)
        })
        .collect()
}

/// Whether any indexed file renames a crate to `root` with `extern crate ..
/// as root;`. At a crate root such a rename (`extern crate self as tracing;`)
/// enters the extern prelude, so `tracing::` names it in every file of that
/// crate, not only the one that declares it. The scan does not work out
/// which crate a file belongs to: any such rename in the workspace refuses.
fn extern_crate_alias_anywhere(index: &RustIndex, root: &str) -> bool {
    index.files().values().any(|file| {
        let source = &file.source;
        source.contains("extern") && source.contains(root) && {
            let parsed = ast::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT).tree();
            parsed
                .syntax()
                .descendants()
                .filter_map(ast::ExternCrate::cast)
                .any(|krate| {
                    krate
                        .rename()
                        .and_then(|rename| rename.name())
                        .is_some_and(|name| name.text().trim_start_matches("r#") == root)
                })
        }
    })
}

/// Whether `root::` in `file` may name something other than the extern
/// crate `root`. Any binding anywhere in the file refuses, without resolving
/// which module it sits in.
fn root_may_be_rebound(file: &ra_ap_syntax::SyntaxNode, root: &str) -> bool {
    let names = |name: Option<ast::Name>| {
        name.is_some_and(|name| name.text().trim_start_matches("r#") == root)
    };
    crate::analysis::syntax::use_rebinds_crate_name(file, root)
        || file.descendants().any(|node| {
            if let Some(module) = ast::Module::cast(node.clone()) {
                return names(module.name());
            }
            if let Some(krate) = ast::ExternCrate::cast(node.clone()) {
                return names(krate.rename().and_then(|rename| rename.name()));
            }
            if let Some(tree) = ast::UseTree::cast(node.clone()) {
                return tree.star_token().is_some() && !glob_reads_in_file_module(&tree);
            }
            ast::MacroCall::can_cast(node.kind())
                && node.parent().is_some_and(|parent| {
                    matches!(
                        parent.kind(),
                        SyntaxKind::SOURCE_FILE | SyntaxKind::ITEM_LIST
                    )
                })
        })
}

/// `use self::*;`, or `use super::*;` inside an inline module: a glob whose
/// source module is in this file, so any binding it brings in is one the
/// whole-file scan already reads. Any other glob shape, including one nested
/// in a use list, may bring a binding of the root from elsewhere.
fn glob_reads_in_file_module(tree: &ast::UseTree) -> bool {
    let Some(path) = tree.path() else {
        return false;
    };
    if !tree
        .syntax()
        .parent()
        .is_some_and(|parent| ast::Use::can_cast(parent.kind()))
    {
        return false;
    }
    match path.syntax().text().to_string().as_str() {
        "self" => true,
        "super" => tree
            .syntax()
            .ancestors()
            .filter_map(ast::Module::cast)
            .any(|module| module.item_list().is_some()),
        _ => false,
    }
}

/// Whether the path `segments` (with no leading `::`) is an audited macro
/// under one of `verified_roots`. Spelling is exact: a raw identifier or any
/// leaf not audited under that root stays opaque.
pub(super) fn is_verified_audited_macro(segments: &[&str], verified_roots: &[&str]) -> bool {
    let [root, name] = segments else {
        return false;
    };
    verified_roots.contains(root)
        && AUDITED_MACRO_ROOTS
            .iter()
            .any(|(audited, macros)| audited == root && macros.contains(name))
}

/// Whether the delimited arguments that open at `open` in `masked` mention
/// `callee` as a whole word. An unbalanced argument list counts as a mention.
pub(super) fn arguments_mention(masked: &str, open: usize, callee: &str) -> bool {
    let Some(arguments) = delimited(masked, open) else {
        return true;
    };
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    arguments.match_indices(callee).any(|(at, _)| {
        let before = arguments[..at].chars().next_back();
        let after = arguments[at + callee.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// The text between the delimiter at `open` and its matching close.
fn delimited(masked: &str, open: usize) -> Option<&str> {
    let bytes = masked.as_bytes();
    let (opener, closer) = match bytes.get(open)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (at, byte) in bytes.iter().enumerate().skip(open) {
        if *byte == opener {
            depth += 1;
        } else if *byte == closer {
            depth -= 1;
            if depth == 0 {
                return masked.get(open + 1..at);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(source: &str) -> ra_ap_syntax::SyntaxNode {
        ast::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT)
            .tree()
            .syntax()
            .clone()
    }

    #[test]
    fn a_file_that_may_rebind_the_root_refuses_it() {
        for source in [
            "use crate as tracing;",
            "use other::log;",
            "use dep::{x, y as tracing};",
            "mod tracing { macro_rules! info { () => {}; } }",
            "mod r#log;",
            "extern crate other as tracing;",
            "use dep::*;",
            "fn f() { use dep::*; }",
            "use super::*;",
            "use {super::*};",
            "generate_tracing!();",
            "mod inner { make_items!(); }",
        ] {
            let file = parsed(source);
            assert!(
                root_may_be_rebound(&file, "tracing") || root_may_be_rebound(&file, "log"),
                "{source}"
            );
        }
        for source in [
            "",
            "use tracing;",
            "use tracing::info;",
            "use log::{warn, error};",
            "extern crate tracing;",
            "fn f() { tracing::info!(\"x\"); }",
            "mod other { pub fn g() {} }",
            // A glob of this file's own module adds no binding the scan
            // does not read.
            "mod tests { use super::*; }",
            "use self::*;",
        ] {
            let file = parsed(source);
            assert!(!root_may_be_rebound(&file, "tracing"), "{source}");
            assert!(!root_may_be_rebound(&file, "log"), "{source}");
        }
    }

    #[test]
    fn only_an_exact_audited_leaf_under_a_verified_root_is_trusted() {
        let roots = ["tracing", "anyhow", "serde_json"];
        assert!(is_verified_audited_macro(&["tracing", "info"], &roots));
        assert!(is_verified_audited_macro(&["tracing", "error"], &roots));
        assert!(is_verified_audited_macro(&["anyhow", "anyhow"], &roots));
        assert!(is_verified_audited_macro(&["serde_json", "json"], &roots));
        for segments in [
            &["log", "info"][..],
            &["tracing", "event"],
            &["tracing", "info_span"],
            &["tracing", "r#info"],
            &["info"],
            &["dep", "tracing", "info"],
            // bail! and ensure! return; a leaf audited under another root
            // is not audited here.
            &["anyhow", "bail"],
            &["anyhow", "ensure"],
            &["anyhow", "json"],
            &["serde_json", "anyhow"],
            &["serde_json", "json_internal"],
        ] {
            assert!(!is_verified_audited_macro(segments, &roots), "{segments:?}");
        }
    }

    #[test]
    fn a_callee_in_the_arguments_is_a_mention() {
        let body = "tracing::info!(\"x\", is_bulk(qty)); tracing::info!(qty)";
        let first = body.find('(').unwrap_or_default();
        assert!(arguments_mention(body, first, "is_bulk"));
        let second = body.rfind('(').unwrap_or_default();
        assert!(!arguments_mention(body, second, "is_bulk"));
        // A longer identifier is not the callee; an unclosed list is.
        let longer = "tracing::info!(is_bulky)";
        assert!(!arguments_mention(
            longer,
            longer.find('(').unwrap_or_default(),
            "is_bulk"
        ));
        let open = "tracing::info!(qty";
        assert!(arguments_mention(
            open,
            open.find('(').unwrap_or_default(),
            "is_bulk"
        ));
    }
}
