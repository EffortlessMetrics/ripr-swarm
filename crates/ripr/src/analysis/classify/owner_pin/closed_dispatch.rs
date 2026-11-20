//! #15277 review: the workspace-visibility gates of a receiver reading that
//! credits a trait method called through a wrapper of the value
//! (`Box<dyn Trait>`, and later a `Deref` wrapper).
//!
//! Method lookup on `wrapper.method()` tries the wrapper itself, and its
//! autoref, before it reaches the value inside. Any trait in scope with a
//! method of that name and an impl covering the wrapper (`impl Ext for
//! Box<dyn Shape>`, a blanket `impl<T: ?Sized> Ext for T`) wins at that
//! step, so the wrapped value's impl never runs. Such an impl may live
//! where no indexed source shows it:
//!
//! - a proc-macro expansion (`#[derive(Ext)]` on an unrelated item);
//! - a `#[path = ".."]` module outside the indexed root;
//! - a workspace member or dependency the current mode does not index;
//! - a `use` from a crate outside the workspace.
//!
//! [`sources_closed`] is the whole-workspace verdict that none of those
//! can apply, computed once per run. [`method_defined_only_by_trait`] then
//! asks that the indexed sources define the method name only in the
//! trait's own declaration and at most one impl of that trait, so no other
//! trait or inherent impl can answer the call first.

use super::{OwnerPinSyntax, RustIndex, is_ident_byte, whole_word_offsets};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::parse_clean_source_file_memoized;
use ra_ap_syntax::{AstNode, ast};

/// Derives the compiler itself provides. Any other derive is a proc macro
/// that may write an impl for the wrapper.
const BUILTIN_DERIVES: &[&str] = &[
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

/// Single-segment attributes the compiler itself provides. `path` is left
/// out on purpose: it may load a module file the index does not read.
const BUILTIN_ATTRIBUTES: &[&str] = &[
    "allow",
    "automatically_derived",
    "cold",
    "deny",
    "deprecated",
    "doc",
    "expect",
    "forbid",
    "ignore",
    "inline",
    "must_use",
    "non_exhaustive",
    "repr",
    "should_panic",
    "test",
    "track_caller",
    "warn",
];

/// Tool roots of multi-segment attributes (`#[rustfmt::skip]`).
const TOOL_ATTRIBUTE_ROOTS: &[&str] = &["clippy", "diagnostic", "rustdoc", "rustfmt"];

/// Whether the indexed sources are the whole program a test's wrapper call
/// can resolve against, memoized once per run in `syntax`.
pub(in crate::analysis::classify) fn sources_closed(
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    syntax.receiver_dispatch_closed(|| compute_sources_closed(index))
}

fn compute_sources_closed(index: &RustIndex) -> bool {
    every_member_indexed(index)
        && !any_member_is_proc_macro(index)
        // Dependency closure, every package file indexed, `use` roots inside
        // the workspace, no `macro_rules!`, `extern crate`, `include!` or
        // `#[path]`.
        && super::super::element_field_pin::field_workspace_is_closed(index)
        && index
            .files()
            .values()
            .all(|facts| source_attributes_are_builtin(&facts.source))
}

/// Whether every workspace member has an indexed file in this mode. Draft
/// and fast modes index the changed packages and their dependents only, so
/// a member they skip may hold a blanket impl.
fn every_member_indexed(index: &RustIndex) -> bool {
    let indexed = index.macro_scope_crates();
    index
        .workspace_member_crates()
        .iter()
        .all(|name| indexed.contains(name))
}

/// Whether a workspace member's manifest declares a proc-macro library, or
/// cannot be read. A proc macro's output is never indexed.
fn any_member_is_proc_macro(index: &RustIndex) -> bool {
    index.workspace_member_dirs.iter().any(|directory| {
        let Ok(text) = crate::bounded_input::read_metadata_toml(directory.join("Cargo.toml"))
        else {
            return true;
        };
        let Ok(manifest) = text.parse::<toml::Table>() else {
            return true;
        };
        manifest_is_proc_macro(&manifest)
    })
}

/// `[lib] proc-macro` (or `proc_macro`) at any value: a `false` written out
/// is unusual enough to refuse with the rest.
fn manifest_is_proc_macro(manifest: &toml::Table) -> bool {
    manifest
        .get("lib")
        .is_some_and(|lib| lib.get("proc-macro").is_some() || lib.get("proc_macro").is_some())
}

/// Whether `source` parses cleanly and every attribute in it is a compiler
/// built-in: no custom derive, attribute macro or `#[path]`.
fn source_attributes_are_builtin(source: &str) -> bool {
    if !source.contains('#') {
        return true;
    }
    parse_clean_source_file_memoized(source).is_some_and(|parse| {
        parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Attr::cast)
            .all(|attribute| {
                let metas = attribute.skip_cfg_attrs();
                !metas.is_empty() && metas.iter().all(meta_is_builtin)
            })
    })
}

fn meta_is_builtin(meta: &ast::Meta) -> bool {
    match meta {
        ast::Meta::CfgMeta(_) => true,
        // `skip_cfg_attrs` already opened every `cfg_attr`; `unsafe(..)`
        // attributes are not read.
        ast::Meta::CfgAttrMeta(_) | ast::Meta::UnsafeMeta(_) => false,
        // `doc = ".."` only: `path = ".."` must refuse.
        ast::Meta::KeyValueMeta(meta) => meta.path().is_some_and(|path| path_text(&path) == "doc"),
        ast::Meta::PathMeta(meta) => meta
            .path()
            .is_some_and(|path| attribute_path_is_builtin(&path_text(&path))),
        ast::Meta::TokenTreeMeta(meta) => {
            let Some(path) = meta.path() else {
                return false;
            };
            let path = path_text(&path);
            if path == "derive" {
                return meta
                    .token_tree()
                    .is_some_and(|tree| derive_list_is_builtin(&tree.syntax().text().to_string()));
            }
            attribute_path_is_builtin(&path)
        }
    }
}

fn path_text(path: &ast::Path) -> String {
    path.syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect()
}

/// Whether a (whitespace-free) attribute path is a built-in or a tool
/// attribute.
fn attribute_path_is_builtin(path: &str) -> bool {
    match path.split_once("::") {
        None => BUILTIN_ATTRIBUTES.contains(&path),
        Some((root, rest)) => TOOL_ATTRIBUTE_ROOTS.contains(&root) && !rest.is_empty(),
    }
}

/// Whether a `derive(..)` token tree (`(Debug, Clone)`) names only the
/// built-in derives, each by its bare name.
fn derive_list_is_builtin(tree: &str) -> bool {
    let Some(inner) = tree
        .trim()
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    inner
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .all(|name| BUILTIN_DERIVES.contains(&name))
}

/// Whether the indexed sources define `method` only in the declaration of
/// `trait trait_name` and in at most one `impl trait_name for ..` block:
/// any other `fn method` (another trait's method or default, a blanket
/// impl's, an inherent one on the wrapper, a nested one) may answer the
/// wrapper call before the wrapped value's vtable is reached. The index's
/// facts and a masked textual count of `fn method` must agree, so a
/// definition the parser left out also refuses.
pub(in crate::analysis::classify) fn method_defined_only_by_trait(
    trait_name: &str,
    method: &str,
    index: &RustIndex,
) -> bool {
    let mut declarations = 0usize;
    let mut impls = 0usize;
    let mut facts = 0usize;
    for function in index.functions().iter() {
        if function.name != method {
            continue;
        }
        facts += 1;
        match &function.item.container {
            FunctionContainer::Trait { trait_name: name } if name == trait_name => {
                declarations += 1;
            }
            FunctionContainer::TraitImpl { trait_path, .. }
                if trait_path.rsplit("::").next() == Some(trait_name) =>
            {
                impls += 1;
            }
            _ => return false,
        }
    }
    if declarations != 1 || impls > 1 || facts != declarations + impls {
        return false;
    }
    let written: usize = index
        .files()
        .values()
        .map(|file| {
            if file.source.contains(method) {
                fn_definitions(&mask_comments_and_strings(&file.source), method)
            } else {
                0
            }
        })
        .sum();
    written == facts
}

/// How many times masked `source` writes `fn method` (raw `fn r#method`
/// too). Every offset is at an ASCII keyword, so each slice starts on a
/// character boundary.
fn fn_definitions(masked: &str, method: &str) -> usize {
    whole_word_offsets(masked, "fn")
        .into_iter()
        .filter(|&offset| {
            let Some(rest) = masked.get(offset + "fn".len()..) else {
                return false;
            };
            let name = rest.trim_start();
            let name = name.strip_prefix("r#").unwrap_or(name);
            name.strip_prefix(method)
                .is_some_and(|after| !after.bytes().next().is_some_and(is_ident_byte))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::{
        attribute_path_is_builtin, derive_list_is_builtin, fn_definitions, manifest_is_proc_macro,
        source_attributes_are_builtin,
    };

    #[test]
    fn counts_every_written_definition_of_the_method() {
        let source = "trait Shape { fn area(&self) -> u32; }\n\
                      impl Shape for Square { fn  area(&self) -> u32 { 1 } }\n\
                      trait Ext { fn r#area(&self) -> u32 { 7 } }\n\
                      fn areas() {}\n\
                      fn größe() {}\n";
        assert_eq!(fn_definitions(source, "area"), 3);
        assert_eq!(fn_definitions(source, "areas"), 1);
        assert_eq!(fn_definitions(source, "größe"), 1);
        assert_eq!(fn_definitions("let fnx = area;", "area"), 0);
    }

    #[test]
    fn only_compiler_derives_and_attributes_are_builtin() {
        for source in [
            "#[derive(Debug, Clone, PartialEq)]\nstruct S;",
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}",
            "#[cfg_attr(test, derive(Debug))]\nstruct S;",
            "#![allow(dead_code)]\n#[rustfmt::skip]\nfn f() {}",
            "/// docs\n#[doc = \"x\"]\nfn f() {}",
            "fn f() {}",
        ] {
            assert!(source_attributes_are_builtin(source), "{source}");
        }
        for source in [
            "#[derive(Ext)]\nstruct Marker;",
            "#[derive(Debug, serde::Serialize)]\nstruct S;",
            "#[path = \"../../shared/ext.rs\"]\nmod ext;",
            "#[cfg_attr(test, path = \"x.rs\")]\nmod ext;",
            "#[tokio::test]\nasync fn t() {}",
            "#[my_attribute]\nfn f() {}",
            "#[derive(Debug)\nstruct S;",
        ] {
            assert!(!source_attributes_are_builtin(source), "{source}");
        }
        assert!(attribute_path_is_builtin("clippy::too_many_arguments"));
        assert!(!attribute_path_is_builtin("path"));
        assert!(derive_list_is_builtin("(Debug, Clone,)"));
        assert!(!derive_list_is_builtin("(Debug, std::hash::Hash)"));
    }

    #[test]
    fn a_proc_macro_manifest_is_detected() -> Result<(), toml::de::Error> {
        let proc_macro: toml::Table =
            "[package]\nname = \"mac\"\n[lib]\nproc-macro = true\n".parse()?;
        let underscore: toml::Table = "[lib]\nproc_macro = true\n".parse()?;
        let plain: toml::Table =
            "[package]\nname = \"probe\"\n[lib]\npath = \"src/lib.rs\"\n".parse()?;
        assert!(manifest_is_proc_macro(&proc_macro));
        assert!(manifest_is_proc_macro(&underscore));
        assert!(!manifest_is_proc_macro(&plain));
        Ok(())
    }
}
