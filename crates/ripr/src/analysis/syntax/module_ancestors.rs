//! The files of the modules that enclose an out-of-line module file (#16277).
//!
//! A test module file such as `src/tests.rs` usually starts with
//! `use super::*;`, which reaches the items of the file that declares it. A
//! reading that must know what such a glob can bring in reads those files
//! too. Only default module resolution is followed: a declaration with a
//! `#[path]` (or a `cfg_attr` that may add one), one nested in an inline
//! module, or a file declared from more than one candidate breaks the
//! chain, and a chain that does not reach a crate root is not used at all,
//! so a glob that leaves the file stays unsettled.

use crate::analysis::workspace::module_parent_candidates;
use ra_ap_syntax::AstNode;
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use std::path::{Path, PathBuf};

/// A module tree deeper than this is not followed further.
const MAX_ENCLOSING_FILES: usize = 8;

/// The files that enclose `file`, nearest first, each declaring the next
/// one's module at its own top level with a plain `mod name;`, up to a crate
/// root (`lib.rs` or `main.rs`); empty for a crate root itself, and `None`
/// when the chain does not reach one. `tree_of` returns the parsed tree of an
/// indexed file, `None` for any other path.
pub(crate) fn enclosing_module_files(
    file: &Path,
    mut tree_of: impl FnMut(&Path) -> Option<ast::SourceFile>,
) -> Option<Vec<ast::SourceFile>> {
    let mut enclosing = Vec::new();
    let mut current = file.to_path_buf();
    while enclosing.len() < MAX_ENCLOSING_FILES {
        let Some(child) = out_of_line_module_name(&current) else {
            // A crate root: every step below it is confirmed.
            return Some(enclosing);
        };
        let mut declaring: Vec<(PathBuf, ast::SourceFile)> = module_parent_candidates(&current)
            .into_iter()
            .filter_map(|candidate| {
                let tree = tree_of(&candidate)?;
                declares_at_top_level(&tree, &child).then_some((candidate, tree))
            })
            .collect();
        if declaring.len() != 1 {
            break;
        }
        let Some((parent, tree)) = declaring.pop() else {
            break;
        };
        enclosing.push(tree);
        current = parent;
    }
    // The chain stopped short of a crate root, so even its first file may be
    // an unused file beside an inline module that really declares the child
    // (`mod a { mod tests; }` in `lib.rs` beside a stale `a.rs`), and an
    // unread enclosing module may define macros the child sees.
    None
}

/// The module name a file holds when another file declares it: the stem, or
/// the directory name of a `mod.rs`. `None` for a crate root.
fn out_of_line_module_name(file: &Path) -> Option<String> {
    let file_name = file.file_name()?.to_str()?;
    if file_name == "lib.rs" || file_name == "main.rs" {
        return None;
    }
    let stem = if file_name == "mod.rs" {
        file.parent()?.file_name()?
    } else {
        file.file_stem()?
    };
    stem.to_str().map(str::to_string)
}

/// Whether the file's own top level declares `mod name;` with no attribute
/// that may move it to another path.
fn declares_at_top_level(tree: &ast::SourceFile, name: &str) -> bool {
    tree.items().any(|item| {
        let ast::Item::Module(module) = item else {
            return false;
        };
        module.item_list().is_none()
            && module
                .name()
                .is_some_and(|ident| ident.text().trim_start_matches("r#") == name)
            && !module.attrs().any(|attr| may_move_module(&attr))
    })
}

/// A `#[path]` attribute, or a `cfg_attr` that spells `path` among the
/// attributes it may add. Other attributes (`cfg`, `doc`, lints) leave
/// default resolution in place.
fn may_move_module(attr: &ast::Attr) -> bool {
    match attribute_name(attr).as_str() {
        "" | "path" => true,
        "cfg_attr" => attr.syntax().text().to_string().contains("path"),
        _ => false,
    }
}

/// The leading path of an attribute (`cfg` for `#[cfg(test)]`), read from
/// its text: `ast::Attr::path` has no path for the dedicated `cfg` and
/// `cfg_attr` meta nodes. Empty when the attribute spells none.
pub(super) fn attribute_name(attr: &ast::Attr) -> String {
    let text = attr
        .meta()
        .map(|meta| meta.syntax().text().to_string())
        .unwrap_or_default();
    text.trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn chain(files: &[(&str, &str)], start: &str) -> Option<Vec<String>> {
        let trees: BTreeMap<PathBuf, ast::SourceFile> = files
            .iter()
            .map(|(path, text)| {
                let parse = ast::SourceFile::parse(text, ra_ap_syntax::Edition::CURRENT);
                (PathBuf::from(path), parse.tree())
            })
            .collect();
        enclosing_module_files(Path::new(start), |path| trees.get(path).cloned()).map(|files| {
            files
                .iter()
                .map(|tree| tree.syntax().text().to_string())
                .collect()
        })
    }

    #[test]
    fn a_test_module_file_reads_the_crate_root_that_declares_it() {
        let root = "mod tests;\npub fn f() {}\n";
        assert_eq!(
            chain(
                &[("src/lib.rs", root), ("src/tests.rs", "")],
                "src/tests.rs"
            ),
            Some(vec![root.to_string()])
        );
    }

    #[test]
    fn cfg_and_doc_attributes_keep_default_resolution() {
        let root = "#[cfg(test)]\n#[doc = \"happy path tests\"]\nmod tests;\n";
        assert_eq!(
            chain(
                &[("src/lib.rs", root), ("src/tests.rs", "")],
                "src/tests.rs"
            ),
            Some(vec![root.to_string()])
        );
    }

    #[test]
    fn a_nested_module_file_reads_every_enclosing_file_up_to_the_root() {
        let root = "mod a;\n";
        let a = "mod tests;\n";
        assert_eq!(
            chain(
                &[
                    ("src/lib.rs", root),
                    ("src/a.rs", a),
                    ("src/a/tests.rs", "")
                ],
                "src/a/tests.rs"
            ),
            Some(vec![a.to_string(), root.to_string()])
        );
        assert_eq!(
            chain(
                &[
                    ("src/lib.rs", root),
                    ("src/a/mod.rs", a),
                    ("src/a/tests/mod.rs", "")
                ],
                "src/a/tests/mod.rs"
            ),
            Some(vec![a.to_string(), root.to_string()])
        );
    }

    #[test]
    fn a_path_attribute_an_inline_declaration_or_two_declarers_end_the_chain() {
        assert_eq!(
            chain(
                &[("src/lib.rs", "#[path = \"tests.rs\"]\nmod tests;\n")],
                "src/tests.rs"
            ),
            None
        );
        assert_eq!(
            chain(
                &[("src/lib.rs", "mod outer { mod tests; }\n")],
                "src/tests.rs"
            ),
            None
        );
        assert_eq!(
            chain(
                &[
                    ("src/lib.rs", "mod tests;\n"),
                    ("src/main.rs", "mod tests;\n")
                ],
                "src/tests.rs"
            ),
            None
        );
        assert_eq!(
            chain(
                &[(
                    "src/lib.rs",
                    "#[cfg_attr(unix, path = \"t.rs\")]\nmod tests;\n"
                )],
                "src/tests.rs"
            ),
            None
        );
        assert_eq!(
            chain(&[("src/lib.rs", "mod other;\n")], "src/tests.rs"),
            None
        );
        // A stale `a.rs` beside the inline `mod a { mod tests; }` that
        // really declares the test file is not taken as its parent.
        assert_eq!(
            chain(
                &[
                    ("src/lib.rs", "mod a { mod tests; }\n"),
                    ("src/a.rs", "mod tests;\n"),
                    ("src/a/tests.rs", ""),
                ],
                "src/a/tests.rs"
            ),
            None
        );
        // A crate root has no enclosing files, and its chain is known.
        assert_eq!(chain(&[], "src/lib.rs"), Some(Vec::new()));
    }
}
