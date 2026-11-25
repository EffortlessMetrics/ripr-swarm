//! The files of the modules that enclose an out-of-line module file (#16277).
//!
//! A test module file such as `src/tests.rs` usually starts with
//! `use super::*;`, which reaches the items of the file that declares it. A
//! reading that must know what such a glob can bring in reads those files
//! too. Only default module resolution is followed: a declaration with a
//! `#[path]` (or a `cfg_attr` that may add one), one nested in an inline
//! module, or a file declared from more than one candidate ends the chain,
//! so a glob that climbs past it stays unsettled.

use crate::analysis::workspace::module_parent_candidates;
use ra_ap_syntax::AstNode;
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use std::path::{Path, PathBuf};

/// A module tree deeper than this is not followed further.
const MAX_ENCLOSING_FILES: usize = 8;

/// The files that enclose `file`, nearest first, each declaring the next
/// one's module at its own top level with a plain `mod name;`. `tree_of`
/// returns the parsed tree of an indexed file, `None` for any other path.
pub(crate) fn enclosing_module_files(
    file: &Path,
    mut tree_of: impl FnMut(&Path) -> Option<ast::SourceFile>,
) -> Vec<ast::SourceFile> {
    let mut enclosing = Vec::new();
    let mut current = file.to_path_buf();
    while enclosing.len() < MAX_ENCLOSING_FILES {
        let Some(child) = out_of_line_module_name(&current) else {
            break;
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
    enclosing
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
            && !module
                .attrs()
                .any(|attr| attr.syntax().text().to_string().contains("path"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn chain(files: &[(&str, &str)], start: &str) -> Vec<String> {
        let trees: BTreeMap<PathBuf, ast::SourceFile> = files
            .iter()
            .map(|(path, text)| {
                let parse = ast::SourceFile::parse(text, ra_ap_syntax::Edition::CURRENT);
                (PathBuf::from(path), parse.tree())
            })
            .collect();
        enclosing_module_files(Path::new(start), |path| trees.get(path).cloned())
            .iter()
            .map(|tree| tree.syntax().text().to_string())
            .collect()
    }

    #[test]
    fn a_test_module_file_reads_the_crate_root_that_declares_it() {
        let root = "mod tests;\npub fn f() {}\n";
        assert_eq!(
            chain(
                &[("src/lib.rs", root), ("src/tests.rs", "")],
                "src/tests.rs"
            ),
            vec![root.to_string()]
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
            vec![a.to_string(), root.to_string()]
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
            vec![a.to_string(), root.to_string()]
        );
    }

    #[test]
    fn a_path_attribute_an_inline_declaration_or_two_declarers_end_the_chain() {
        let none: Vec<String> = Vec::new();
        assert_eq!(
            chain(
                &[("src/lib.rs", "#[path = \"tests.rs\"]\nmod tests;\n")],
                "src/tests.rs"
            ),
            none
        );
        assert_eq!(
            chain(
                &[("src/lib.rs", "mod outer { mod tests; }\n")],
                "src/tests.rs"
            ),
            none
        );
        assert_eq!(
            chain(
                &[
                    ("src/lib.rs", "mod tests;\n"),
                    ("src/main.rs", "mod tests;\n")
                ],
                "src/tests.rs"
            ),
            none
        );
        assert_eq!(
            chain(&[("src/lib.rs", "mod other;\n")], "src/tests.rs"),
            none
        );
        assert_eq!(chain(&[], "src/lib.rs"), none);
    }
}
