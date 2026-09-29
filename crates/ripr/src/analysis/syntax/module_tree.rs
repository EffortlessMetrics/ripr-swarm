//! Module-tree edges one Rust file declares (#4435).
//!
//! The diff seeding authority needs to know whether a changed file is part of
//! any Cargo target's module tree. This producer reports, for one file, the
//! out-of-line edges rustc would follow from it: `mod name;` (default
//! resolution, including declarations nested in inline modules), top-level
//! `#[path = "..."] mod name;`, and literal `include!("...")`.
//!
//! Anything that could add an edge this scan cannot resolve makes the scan
//! incomplete instead of silently dropping it:
//!
//! - a parse error;
//! - a `#[path]` that is not one plain string literal, or is introduced
//!   through `cfg_attr`;
//! - a `#[path]` on an inline module, or on an out-of-line declaration
//!   nested in one (the directory rules there differ and are not modeled);
//! - an out-of-line `mod name;` outside the item tree (inside a function);
//! - a `mod` keyword or an `include` inside a macro token tree (`cfg_if!`,
//!   `macro_rules!`), where edges only exist after expansion;
//! - any other item-position macro call (a dependency's macro can expand to
//!   `mod name;`), except std's `thread_local!` and `compile_error!`;
//! - a non-literal `include!`, except the generated-code shape that names
//!   `OUT_DIR`, which includes build output rather than a source file.
//!
//! An incomplete scan never proves a file unreachable; the consumer keeps its
//! layout rule for that package.

use ra_ap_syntax::{
    AstNode, Edition, SourceFile, SyntaxKind,
    ast::{self, HasAttrs, HasName},
};
use std::path::PathBuf;

use super::ra::{include_literal_path, path_target_from_attributes};
use crate::analysis::facts::ModulePathTarget;

/// Std item-position macros whose expansion cannot declare a module.
/// Compared after any `std::`/`core::` prefix is dropped.
const ITEM_MACROS_WITHOUT_MODULES: [&str; 2] = ["thread_local", "compile_error"];

/// One out-of-line module-tree edge declared by a Rust file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RustModuleTreeEdge {
    /// `mod name;` with default resolution. `inline` names the enclosing
    /// inline modules, outermost first (`mod a { mod b; }` → `["a"]`).
    Default { inline: Vec<String>, name: String },
    /// Top-level `#[path = "..."] mod name;`, relative to the declaring
    /// file's directory.
    Path(PathBuf),
    /// `include!("...")`, relative to the including file's directory.
    Include(PathBuf),
}

/// The module-tree edges of one file and whether the scan saw every edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RustModuleTreeScan {
    pub(crate) edges: Vec<RustModuleTreeEdge>,
    pub(crate) complete: bool,
}

/// Scans one Rust source text for its module-tree edges.
pub(crate) fn rust_module_tree_scan(text: &str) -> RustModuleTreeScan {
    let parse = SourceFile::parse(text, Edition::CURRENT);
    if !parse.errors().is_empty() {
        return RustModuleTreeScan::default();
    }
    let tree = parse.tree();
    let mut scan = RustModuleTreeScan {
        edges: Vec::new(),
        complete: true,
    };
    for module in tree.syntax().descendants().filter_map(ast::Module::cast) {
        let attributes = module
            .attrs()
            .map(|attr| attr.syntax().text().to_string())
            .collect::<Vec<_>>();
        let path_target = path_target_from_attributes(&attributes);
        if module.item_list().is_some() {
            if path_target != ModulePathTarget::Default {
                scan.complete = false;
            }
            continue;
        }
        let Some(name) = module.name() else {
            scan.complete = false;
            continue;
        };
        let Some(inline) = enclosing_inline_modules(&module) else {
            scan.complete = false;
            continue;
        };
        match path_target {
            ModulePathTarget::Default => scan.edges.push(RustModuleTreeEdge::Default {
                inline,
                // `mod r#type;` resolves to `type.rs`.
                name: name.text().trim_start_matches("r#").to_string(),
            }),
            ModulePathTarget::Literal(path) if inline.is_empty() => {
                scan.edges
                    .push(RustModuleTreeEdge::Path(PathBuf::from(path)));
            }
            ModulePathTarget::Literal(_) | ModulePathTarget::Unknown => scan.complete = false,
        }
    }
    for macro_call in tree.syntax().descendants().filter_map(ast::MacroCall::cast) {
        let Some(path) = macro_call.path() else {
            continue;
        };
        let callee = path.syntax().text().to_string();
        let callee = callee.trim_start_matches("::");
        let callee = callee
            .strip_prefix("std::")
            .or_else(|| callee.strip_prefix("core::"))
            .unwrap_or(callee);
        if callee != "include" {
            // An item-position macro from any crate can expand to `mod x;`
            // without spelling `mod` here, so its expansion is unknown.
            // Std's item macros that cannot declare modules are the only
            // exception.
            let item_position = macro_call.syntax().parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    SyntaxKind::SOURCE_FILE | SyntaxKind::ITEM_LIST
                )
            });
            if item_position && !ITEM_MACROS_WITHOUT_MODULES.contains(&callee) {
                scan.complete = false;
            }
            continue;
        }
        // The node spans the call's attributes too; the expression starts at
        // the macro path.
        let start: usize = path.syntax().text_range().start().into();
        let end: usize = macro_call.syntax().text_range().end().into();
        let expression = text.get(start..end).unwrap_or_default();
        match include_literal_path(expression) {
            Some(target) => scan.edges.push(RustModuleTreeEdge::Include(target)),
            None if expression.contains("OUT_DIR") => {}
            None => scan.complete = false,
        }
    }
    // Declarations and includes inside a token tree only exist after macro
    // expansion (`cfg_if! { ... include!("unix.rs"); }`).
    if tree.syntax().descendants_with_tokens().any(|element| {
        let in_token_tree = element
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::TOKEN_TREE);
        in_token_tree
            && (element.kind() == SyntaxKind::MOD_KW
                || element.as_token().is_some_and(|token| {
                    token.kind() == SyntaxKind::IDENT && token.text() == "include"
                }))
    }) {
        scan.complete = false;
    }
    scan
}

/// The inline modules enclosing an out-of-line declaration, outermost first,
/// or `None` when the declaration sits outside the item tree (a function body
/// or other block).
fn enclosing_inline_modules(module: &ast::Module) -> Option<Vec<String>> {
    let mut names = Vec::new();
    for ancestor in module.syntax().ancestors().skip(1) {
        match ancestor.kind() {
            SyntaxKind::SOURCE_FILE => {
                names.reverse();
                return Some(names);
            }
            SyntaxKind::ITEM_LIST => {}
            SyntaxKind::MODULE => {
                let inline = ast::Module::cast(ancestor)?;
                names.push(inline.name()?.text().trim_start_matches("r#").to_string());
            }
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default(inline: &[&str], name: &str) -> RustModuleTreeEdge {
        RustModuleTreeEdge::Default {
            inline: inline.iter().map(|segment| segment.to_string()).collect(),
            name: name.to_string(),
        }
    }

    #[test]
    fn scan_reports_default_path_include_and_nested_inline_edges() {
        let scan = rust_module_tree_scan(
            "mod plain;\n\
             #[cfg(test)]\nmod tests;\n\
             #[path = \"odd/place.rs\"]\nmod placed;\n\
             pub mod outer { pub mod inner { mod leaf; } }\n\
             mod inline_only { fn f() {} }\n\
             include!(\"fragment.rs\");\n\
             include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\n",
        );
        assert!(scan.complete, "{scan:?}");
        assert_eq!(
            scan.edges,
            vec![
                default(&[], "plain"),
                default(&[], "tests"),
                RustModuleTreeEdge::Path(PathBuf::from("odd/place.rs")),
                default(&["outer", "inner"], "leaf"),
                RustModuleTreeEdge::Include(PathBuf::from("fragment.rs")),
            ]
        );
    }

    #[test]
    fn scan_is_incomplete_for_edges_it_cannot_resolve() {
        for source in [
            "#[path = concat!(\"a\", \".rs\")]\nmod dynamic;\n",
            "#[cfg_attr(unix, path = \"unix.rs\")]\nmod platform;\n",
            "#[path = \"inline\"]\nmod inline { mod child; }\n",
            "mod outer { #[path = \"x.rs\"] mod child; }\n",
            "fn f() { mod hidden; }\n",
            "cfg_if::cfg_if! { if #[cfg(unix)] { mod unix; } }\n",
            "macro_rules! declare { ($name:ident) => { mod $name; }; }\n",
            "include!(concat!(\"frag\", \".rs\"));\n",
            "decl::declare_mod!(generated);\n",
            "cfg_if::cfg_if! { if #[cfg(unix)] { include!(\"unix.rs\"); } }\n",
            "mod outer { lazy_static::lazy_static! { static ref X: u8 = 1; } }\n",
            "mod broken\n",
        ] {
            assert!(
                !rust_module_tree_scan(source).complete,
                "`{source}` must not scan as complete"
            );
        }
    }

    #[test]
    fn scan_ignores_non_item_text_and_resolves_raw_identifiers() {
        let scan = rust_module_tree_scan(
            "// mod commented;\n\
             const TEXT: &str = \"mod quoted;\";\n\
             fn f() { let _ = preinclude!(\"x.rs\"); println!(\"in a body\"); }\n\
             thread_local! { static COUNT: u8 = 0; }\n\
             mod r#type;\n\
             mod r#async { mod r#match; }\n\
             std::include!(\"frag.rs\");\n",
        );
        assert!(scan.complete, "{scan:?}");
        assert_eq!(
            scan.edges,
            vec![
                default(&[], "type"),
                default(&["async"], "match"),
                RustModuleTreeEdge::Include(PathBuf::from("frag.rs")),
            ]
        );
    }
}
