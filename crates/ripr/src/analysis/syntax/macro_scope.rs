//! The textual `macro_rules!` scope one file contributes to others (#6925).
//!
//! A `macro_rules!` is visible after its definition in the same module and
//! in every child module declared after it; `#[macro_use] mod name;` also
//! lets the items after it see the macros the module file defines. The
//! test-generator expander reads one file at a time, so a table whose
//! generator lives in `tests/macros.rs` was never expanded.
//!
//! This module reads one file's top-level definitions and out-of-line `mod`
//! declarations, and splices macro definitions that another file makes
//! visible in front of the file's first item. It resolves nothing across
//! files; the facts pass owns which files that is and refuses what it
//! cannot resolve.

use ra_ap_syntax::ast::{HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, ast};

use super::nesting::parse_clean_source_file;

/// A top-level `macro_rules!` item, flattened to one line so splicing it in
/// front of a file shifts every later line by the same, known amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopedMacro {
    pub name: String,
    /// Byte offset of the item in its file; textual scope is ordered by it.
    pub offset: usize,
    pub text: String,
}

/// A top-level out-of-line `mod name;`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopedModule {
    pub name: String,
    pub offset: usize,
    /// `#[macro_use]`: the macros the module file defines stay in scope
    /// after the declaration.
    pub macro_use: bool,
    /// Carries no attribute but `macro_use`, `cfg(test)`, `allow`, `warn`,
    /// `deny` or `doc`. A `#[path]`, another `cfg` or a `cfg_attr` can
    /// change which file the module is or whether it exists at all.
    pub plain: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MacroScope {
    pub macros: Vec<ScopedMacro>,
    pub modules: Vec<ScopedModule>,
    /// A definition this scope could not list (a token spanning lines) or a
    /// `#[macro_use]` inline module whose macros it does not list. Either can
    /// shadow a name the listed definitions carry, so a resolver that reads
    /// this file refuses to import.
    pub opaque: bool,
}

/// The top-level definitions and module declarations of `source`. `None`
/// when it does not parse cleanly.
pub(crate) fn macro_scope(source: &str) -> Option<MacroScope> {
    let parse = parse_clean_source_file(source)?;
    let mut scope = MacroScope::default();
    for item in parse.tree().items() {
        let offset = usize::from(item.syntax().text_range().start());
        match item {
            ast::Item::MacroRules(rule) => {
                let Some(name) = rule.name().map(|name| name.text().to_string()) else {
                    continue;
                };
                match flatten(rule.syntax()) {
                    Some(text) => scope.macros.push(ScopedMacro { name, offset, text }),
                    None => scope.opaque = true,
                }
            }
            ast::Item::Module(module) if module.item_list().is_some() => {
                if module_attributes(&module).0 {
                    scope.opaque = true;
                }
            }
            ast::Item::Module(module) => {
                let Some(name) = module.name().map(|name| name.text().to_string()) else {
                    continue;
                };
                let (macro_use, plain) = module_attributes(&module);
                scope.modules.push(ScopedModule {
                    name,
                    offset,
                    macro_use,
                    plain,
                });
            }
            _ => {}
        }
    }
    Some(scope)
}

fn module_attributes(module: &ast::Module) -> (bool, bool) {
    let mut macro_use = false;
    let mut plain = true;
    for attr in module.attrs() {
        let text: String = attr
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let inner = text
            .strip_prefix("#[")
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or("?");
        match inner {
            "macro_use" => macro_use = true,
            "cfg(test)" => {}
            other
                if ["allow(", "warn(", "deny(", "doc"]
                    .iter()
                    .any(|p| other.starts_with(p)) => {}
            _ => plain = false,
        }
    }
    (macro_use, plain)
}

/// The node's code on one line: comments dropped, each run of whitespace a
/// single space. `None` when a token itself spans lines (a string literal),
/// which would move every later line.
fn flatten(node: &ra_ap_syntax::SyntaxNode) -> Option<String> {
    let mut text = String::new();
    for token in node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        match token.kind() {
            SyntaxKind::COMMENT => {
                if !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            SyntaxKind::WHITESPACE => {
                if !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            _ => {
                if token.text().contains('\n') {
                    return None;
                }
                text.push_str(token.text());
            }
        }
    }
    Some(text.trim().to_string())
}

/// `source` with `imported` (one line of definitions) inserted in front of
/// its first item, so the definitions precede every invocation and follow
/// any inner attribute. The result has exactly one more line break than the
/// text it replaces, before the first item. `None` when `source` does not
/// parse cleanly or has no item.
pub(crate) fn splice_imported_macros(source: &str, imported: &str) -> Option<String> {
    if imported.contains('\n') {
        return None;
    }
    let parse = parse_clean_source_file(source)?;
    let first = parse.tree().items().next()?;
    let offset = usize::from(first.syntax().text_range().start());
    let mut spliced = String::with_capacity(source.len() + imported.len() + 1);
    spliced.push_str(source.get(..offset)?);
    spliced.push_str(imported);
    spliced.push('\n');
    spliced.push_str(source.get(offset..)?);
    Some(spliced)
}

/// Lines `splice_imported_macros` adds before any item.
pub(crate) const SPLICED_LINES: usize = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_lists_top_level_definitions_and_plain_module_declarations() {
        let scope = macro_scope(
            "// header\n#[macro_use]\nmod macros;\n#[path = \"x.rs\"]\nmod odd;\nmod cases;\nmacro_rules! t {\n    // a comment\n    ($n:ident) => { #[test] fn $n() {} };\n}\nmod inline { macro_rules! hidden { () => {} } }\n",
        );
        assert!(scope.is_some(), "premise: the file parses");
        let scope = scope.unwrap_or_default();
        assert_eq!(
            scope
                .modules
                .iter()
                .map(|module| (module.name.as_str(), module.macro_use, module.plain))
                .collect::<Vec<_>>(),
            vec![
                ("macros", true, true),
                ("odd", false, false),
                ("cases", false, true)
            ]
        );
        assert_eq!(
            scope.macros.len(),
            1,
            "an inline module's macro is not top level"
        );
        assert_eq!(
            scope.macros[0].text,
            "macro_rules! t { ($n:ident) => { #[test] fn $n() {} }; }"
        );
    }

    #[test]
    fn a_definition_with_a_multiline_token_is_not_flattened() {
        let scope = macro_scope("macro_rules! t {\n    () => { \"a\nb\" };\n}\n");
        assert_eq!(
            scope.map(|scope| (scope.macros.len(), scope.opaque)),
            Some((0, true))
        );
    }

    #[test]
    fn an_inline_macro_use_module_makes_the_scope_opaque() {
        let scope = macro_scope("#[macro_use]\nmod inner { macro_rules! t { () => {} } }\n");
        assert_eq!(scope.map(|scope| scope.opaque), Some(true));
        let plain = macro_scope("mod inner { macro_rules! t { () => {} } }\n");
        assert_eq!(plain.map(|scope| scope.opaque), Some(false));
    }

    #[test]
    fn splicing_lands_after_inner_attributes_and_adds_one_line() {
        let source = "#![no_std]\n\nfn first() {}\nfn second() {}\n";
        let spliced = splice_imported_macros(source, "macro_rules! m { () => {} }");
        assert_eq!(
            spliced.as_deref(),
            Some("#![no_std]\n\nmacro_rules! m { () => {} }\nfn first() {}\nfn second() {}\n")
        );
        assert_eq!(splice_imported_macros("", "x"), None, "no item to precede");
        assert_eq!(splice_imported_macros(source, "a\nb"), None);
    }
}
