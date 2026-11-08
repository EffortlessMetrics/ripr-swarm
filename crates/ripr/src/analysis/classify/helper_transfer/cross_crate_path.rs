//! The path by which another crate names a unique free `pub fn` (#11401).
//!
//! `wscore::discount` names a function at the root of `wscore`'s library;
//! `wscore::pricing::discount` names one in a public module. Both are read
//! here from the declarations themselves, never from a spelled call: the
//! function must be `pub`, and every module between the library root and it
//! must be an unconditional, attribute-free `pub mod` declared exactly once,
//! out of line in the file layout it names or inline in its parent.
//!
//! Fail-closed: a `pub(crate)` or restricted item, a `#[cfg]`, `#[path]` or
//! any other attribute on a link of the chain, a second module of the same
//! name, a missing or doubled module file, an inline module in the middle of
//! a file chain, or any file of the chain that may splice items in (a macro
//! definition, an item-position macro, `include!`) names no path.

use crate::analysis::classify::{call_identity, related_tests};
use crate::analysis::facts::{FunctionSummary, RustIndex};
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, SyntaxNode};
use std::path::{Path, PathBuf};

/// Where a library exports a function: the library root file and the module
/// segments between the crate name and the function.
pub(super) struct PublicPath {
    pub(super) root: PathBuf,
    pub(super) modules: Vec<String>,
}

pub(super) fn public_path(index: &RustIndex, callee: &FunctionSummary) -> Option<PublicPath> {
    let normalized = related_tests::normalize_path(&callee.file);
    let prefix = related_tests::package_prefix(&callee.file).unwrap_or_default();
    let relative = normalized.strip_prefix(&prefix)?.strip_prefix("src/")?;
    let file_modules = file_module_segments(relative)?;
    let root = PathBuf::from(format!("{prefix}src/lib.rs"));

    let mut current = root.clone();
    let mut container = parsed_items(index, &current, !file_modules.is_empty(), false)?;
    for segment in &file_modules {
        let declaration = sole_public_module(&container, segment)?;
        if declaration.item_list().is_some() {
            return None;
        }
        let normalized_current = related_tests::normalize_path(&current);
        let directory = match normalized_current.rsplit_once('/') {
            Some((directory, name)) if name == "lib.rs" || name == "mod.rs" => {
                directory.to_string()
            }
            Some((directory, name)) => format!("{directory}/{}", name.trim_end_matches(".rs")),
            None => return None,
        };
        let flat = PathBuf::from(format!("{directory}/{segment}.rs"));
        let nested = PathBuf::from(format!("{directory}/{segment}/mod.rs"));
        current = match (
            index.files().get(&flat).is_some(),
            index.files().get(&nested).is_some(),
        ) {
            (true, false) => flat,
            (false, true) => nested,
            _ => return None,
        };
        container = parsed_items(index, &current, true, true)?;
    }
    if related_tests::normalize_path(&current) != normalized {
        return None;
    }

    let source = &index.files().get(&current)?.source;
    let parse = crate::analysis::syntax::parse_clean_source_file(source)?;
    let mut named = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            function
                .name()
                .is_some_and(|name| is_named(&name, &callee.name))
        });
    let function = named.next()?;
    if named.next().is_some()
        || !is_plain_pub(function.visibility())
        || presence_may_change(&function)
    {
        return None;
    }
    let mut inline = Vec::new();
    let mut node = function.syntax().parent()?;
    loop {
        if ast::SourceFile::can_cast(node.kind()) {
            break;
        }
        let list = ast::ItemList::cast(node.clone())?;
        let module = ast::Module::cast(list.syntax().parent()?)?;
        let siblings = module.syntax().parent()?;
        let name = module.name()?.text().trim_start_matches("r#").to_string();
        if !is_plain_pub(module.visibility())
            || module.attrs().next().is_some()
            || list.attrs().next().is_some()
            || !declared_once(&siblings, &name)
            || call_identity::source_may_splice_items(source, true)
        {
            return None;
        }
        inline.push(name);
        node = siblings;
    }
    inline.reverse();
    let mut modules = file_modules;
    modules.extend(inline);
    if !modules.iter().all(|segment| super::is_identifier(segment)) {
        return None;
    }
    Some(PublicPath { root, modules })
}

/// The module segments a file under `src/` stands for: none for `lib.rs`,
/// `a/b.rs` and `a/b/mod.rs` are `[a, b]`. `None` for a binary target.
fn file_module_segments(relative: &str) -> Option<Vec<String>> {
    if relative == "lib.rs" {
        return Some(Vec::new());
    }
    let stem = relative.strip_suffix(".rs")?;
    let mut parts: Vec<&str> = stem.split('/').collect();
    if parts.last() == Some(&"mod") {
        parts.pop();
    }
    if parts.is_empty()
        || parts
            .first()
            .is_some_and(|first| matches!(*first, "main" | "bin"))
    {
        return None;
    }
    Some(parts.into_iter().map(str::to_string).collect())
}

/// The top-level items of `path`, refused when the file may splice items in
/// (checked when `check_splice`, which is every file of a module chain) or,
/// for a module file, when it carries any inner attribute (`#![cfg(..)]`
/// gates the whole module). The library root keeps its crate-level inner
/// attributes (`#![deny(..)]`, `#![cfg_attr(docsrs, ..)]`).
fn parsed_items(
    index: &RustIndex,
    path: &Path,
    check_splice: bool,
    module_file: bool,
) -> Option<Vec<ast::Item>> {
    let source = &index.files().get(path)?.source;
    if check_splice && call_identity::source_may_splice_items(source, true) {
        return None;
    }
    let parse = crate::analysis::syntax::parse_clean_source_file(source)?;
    if module_file && parse.tree().attrs().next().is_some() {
        return None;
    }
    Some(parse.tree().items().collect())
}

/// The one module called `segment` among `items`, which must be a plain
/// `pub mod` with no attribute.
fn sole_public_module(items: &[ast::Item], segment: &str) -> Option<ast::Module> {
    let mut same_named = items.iter().filter_map(|item| match item {
        ast::Item::Module(module) if module.name().is_some_and(|name| is_named(&name, segment)) => {
            Some(module.clone())
        }
        _ => None,
    });
    let module = same_named.next()?;
    (same_named.next().is_none()
        && is_plain_pub(module.visibility())
        && module.attrs().next().is_none())
    .then_some(module)
}

/// Whether exactly one module called `name` sits directly among the items of
/// `container` (a source file or an item list).
fn declared_once(container: &SyntaxNode, name: &str) -> bool {
    let items: Vec<ast::Item> = if let Some(file) = ast::SourceFile::cast(container.clone()) {
        file.items().collect()
    } else if let Some(list) = ast::ItemList::cast(container.clone()) {
        list.items().collect()
    } else {
        return false;
    };
    items
        .iter()
        .filter(|item| {
            matches!(item, ast::Item::Module(module)
                if module.name().is_some_and(|n| is_named(&n, name)))
        })
        .count()
        == 1
}

/// Whether an attribute on the function can make it absent: a `#[cfg(..)]`, or
/// a `#[cfg_attr(.., cfg(..))]` that adds one. A `cfg_attr` that only adds
/// documentation or lint attributes (`#[cfg_attr(docsrs, doc(..))]`) does not.
fn presence_may_change(function: &ast::Fn) -> bool {
    function.attrs().any(|attr| {
        // The attribute path, with a raw-identifier spelling (`r#cfg`) read
        // as the name it stands for.
        let path = attr
            .path()
            .map(|path| path.syntax().text().to_string())
            .unwrap_or_default();
        let name = path.trim_start_matches("r#");
        let adds_cfg = attr
            .syntax()
            .text()
            .to_string()
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .any(|word| word == "cfg");
        match name {
            "cfg" => true,
            "cfg_attr" => adds_cfg,
            _ => {
                matches!(attr.meta(), Some(ast::Meta::CfgMeta(_)))
                    || (matches!(attr.meta(), Some(ast::Meta::CfgAttrMeta(_))) && adds_cfg)
            }
        }
    })
}

fn is_named(name: &ast::Name, expected: &str) -> bool {
    name.text().trim_start_matches("r#") == expected
}

/// Exactly `pub`: no `pub(crate)`, `pub(super)` or `pub(in ..)`.
fn is_plain_pub(visibility: Option<ast::Visibility>) -> bool {
    visibility.is_some_and(|visibility| visibility.syntax().text() == "pub")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::summarize_file;

    fn path_of(files: &[(&str, &str)]) -> Option<Vec<String>> {
        let mut index = RustIndex::default();
        for (path, text) in files {
            let facts = summarize_file(PathBuf::from(path), (*text).to_string());
            index.extend_functions(facts.functions.iter().cloned());
            index.insert_file_only(PathBuf::from(path), facts);
        }
        let callee = index
            .functions()
            .iter()
            .find(|function| function.name == "discount")?
            .clone();
        public_path(&index, &callee).map(|found| found.modules)
    }

    const FN: &str = "pub fn discount(p: u32) -> u32 {\n    p\n}\n";

    fn segments(names: &[&str]) -> Option<Vec<String>> {
        Some(names.iter().map(|name| name.to_string()).collect())
    }

    #[test]
    fn a_function_is_named_by_its_chain_of_public_modules() {
        assert_eq!(path_of(&[("wscore/src/lib.rs", FN)]), segments(&[]));
        assert_eq!(
            path_of(&[("wscore/src/lib.rs", &format!("#[inline]\n{FN}"))]),
            segments(&[])
        );
        // Documentation and lint attributes do not change presence, and the
        // library root keeps its crate-level inner attributes.
        assert_eq!(
            path_of(&[(
                "wscore/src/lib.rs",
                &format!("#![deny(missing_docs)]\n#[cfg_attr(docsrs, doc(hidden))]\n{FN}")
            )]),
            segments(&[])
        );
        assert_eq!(
            path_of(&[(
                "wscore/src/lib.rs",
                &format!("pub mod pricing {{\n    {FN}}}\n")
            )]),
            segments(&["pricing"])
        );
        assert_eq!(
            path_of(&[
                ("wscore/src/lib.rs", "pub mod pricing;\n"),
                ("wscore/src/pricing.rs", FN),
            ]),
            segments(&["pricing"])
        );
        assert_eq!(
            path_of(&[
                ("wscore/src/lib.rs", "pub mod pricing;\n"),
                ("wscore/src/pricing/mod.rs", "pub mod rules;\n"),
                ("wscore/src/pricing/rules.rs", FN),
            ]),
            segments(&["pricing", "rules"])
        );
        assert_eq!(
            path_of(&[
                ("wscore/src/lib.rs", "pub mod pricing;\n"),
                (
                    "wscore/src/pricing.rs",
                    &format!("pub mod rules {{\n    {FN}}}\n")
                ),
            ]),
            segments(&["pricing", "rules"])
        );
    }

    #[test]
    fn any_link_that_is_not_plain_public_and_unique_names_no_path() {
        let private_fn = "fn discount(p: u32) -> u32 {\n    p\n}\n";
        for (label, files) in [
            (
                "private function",
                vec![("wscore/src/lib.rs", private_fn.to_string())],
            ),
            (
                "pub(crate) function",
                vec![(
                    "wscore/src/lib.rs",
                    "pub(crate) fn discount(p: u32) -> u32 {\n    p\n}\n".to_string(),
                )],
            ),
            (
                "cfg function",
                vec![("wscore/src/lib.rs", format!("#[cfg(test)]\n{FN}"))],
            ),
            (
                "cfg_attr that adds a cfg",
                vec![(
                    "wscore/src/lib.rs",
                    format!("#[cfg_attr(unix, cfg(test))]\n{FN}"),
                )],
            ),
            (
                "raw-identifier cfg on the function",
                vec![("wscore/src/lib.rs", format!("#[r#cfg(test)]\n{FN}"))],
            ),
            (
                "nested cfg_attr that adds a cfg",
                vec![(
                    "wscore/src/lib.rs",
                    format!("#[cfg_attr(unix, cfg_attr(test, cfg(test)))]\n{FN}"),
                )],
            ),
            (
                "inner cfg on an inline module",
                vec![(
                    "wscore/src/lib.rs",
                    format!("pub mod pricing {{\n    #![cfg(test)]\n    {FN}}}\n"),
                )],
            ),
            (
                "cfg_attr on a parent module",
                vec![(
                    "wscore/src/lib.rs",
                    format!("#[cfg_attr(unix, path = \"x.rs\")]\npub mod pricing {{\n    {FN}}}\n"),
                )],
            ),
            (
                "restricted-in module",
                vec![(
                    "wscore/src/lib.rs",
                    format!("pub(in crate) mod pricing {{\n    {FN}}}\n"),
                )],
            ),
            (
                "inner attribute on a module file",
                vec![
                    ("wscore/src/lib.rs", "pub mod pricing;\n".to_string()),
                    (
                        "wscore/src/pricing.rs",
                        format!("#![cfg(feature = \"x\")]\n{FN}"),
                    ),
                ],
            ),
            (
                "re-export of a private module's function",
                vec![
                    (
                        "wscore/src/lib.rs",
                        "mod pricing;\npub use pricing::discount;\n".to_string(),
                    ),
                    ("wscore/src/pricing.rs", FN.to_string()),
                ],
            ),
            (
                "private module",
                vec![("wscore/src/lib.rs", format!("mod pricing {{\n    {FN}}}\n"))],
            ),
            (
                "pub(crate) module",
                vec![(
                    "wscore/src/lib.rs",
                    format!("pub(crate) mod pricing {{\n    {FN}}}\n"),
                )],
            ),
            (
                "attributed module",
                vec![(
                    "wscore/src/lib.rs",
                    format!("#[cfg(feature = \"x\")]\npub mod pricing {{\n    {FN}}}\n"),
                )],
            ),
            (
                "private file module",
                vec![
                    ("wscore/src/lib.rs", "mod pricing;\n".to_string()),
                    ("wscore/src/pricing.rs", FN.to_string()),
                ],
            ),
            (
                "path-remapped file module",
                vec![
                    (
                        "wscore/src/lib.rs",
                        "#[path = \"other.rs\"]\npub mod pricing;\n".to_string(),
                    ),
                    ("wscore/src/pricing.rs", FN.to_string()),
                ],
            ),
            (
                "undeclared file",
                vec![
                    ("wscore/src/lib.rs", "pub fn other() {}\n".to_string()),
                    ("wscore/src/pricing.rs", FN.to_string()),
                ],
            ),
            (
                "both module files",
                vec![
                    ("wscore/src/lib.rs", "pub mod pricing;\n".to_string()),
                    ("wscore/src/pricing.rs", FN.to_string()),
                    ("wscore/src/pricing/mod.rs", FN.to_string()),
                ],
            ),
            (
                "module declared twice",
                vec![(
                    "wscore/src/lib.rs",
                    format!("pub mod pricing {{\n    {FN}}}\npub mod pricing {{}}\n"),
                )],
            ),
            (
                "macro in the chain",
                vec![(
                    "wscore/src/lib.rs",
                    format!("macro_rules! m {{ () => {{}} }}\npub mod pricing {{\n    {FN}}}\n"),
                )],
            ),
            (
                "binary target",
                vec![("wscore/src/main.rs", FN.to_string())],
            ),
            (
                "method of a type",
                vec![(
                    "wscore/src/lib.rs",
                    "pub struct S;\nimpl S {\n    pub fn discount(&self) -> u32 {\n        1\n    }\n}\n"
                        .to_string(),
                )],
            ),
        ] {
            let borrowed: Vec<(&str, &str)> = files
                .iter()
                .map(|(path, text)| (*path, text.as_str()))
                .collect();
            assert_eq!(path_of(&borrowed), None, "{label}");
        }
    }
}
