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

use ra_ap_syntax::ast::{HasAttrs, HasModuleItem, HasName, HasVisibility};
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
    /// Carries a plain `#[macro_export]`: the macro is also named by the
    /// path `crate::name`, wherever in the crate it is defined.
    pub exported: bool,
}

/// A top-level `use crate::name;` or `use crate::{a, b};` of plain names,
/// with no attribute, rename, nesting or glob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopedUse {
    pub names: Vec<String>,
    /// Byte range of the whole item.
    pub start: usize,
    pub end: usize,
    /// For a group, each name with the byte range of its identifier and the
    /// comma that follows it, so one name can be blanked and the rest kept.
    pub members: Vec<(String, usize, usize)>,
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
    pub uses: Vec<ScopedUse>,
    /// `macro_export` is spelled somewhere this scope does not list as a
    /// plain exported top-level macro (inside a module or function, behind a
    /// `cfg`, with arguments, in a macro body). That macro is named by a
    /// crate path from anywhere, so a resolver that imports by path refuses.
    pub export_opaque: bool,
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
                let (exported, plain) = macro_attributes(&rule);
                if exported && !plain {
                    scope.export_opaque = true;
                }
                match flatten(rule.syntax()) {
                    Some(text) => scope.macros.push(ScopedMacro {
                        name,
                        offset,
                        text,
                        exported: exported && plain,
                    }),
                    None => scope.opaque = true,
                }
            }
            ast::Item::Module(module)
                if module.item_list().is_some() && module_attributes(&module).0 =>
            {
                scope.opaque = true;
            }
            ast::Item::Module(module) if module.item_list().is_some() => {}
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
            ast::Item::Use(item) => {
                if let Some((names, members)) = crate_path_import(&item) {
                    let range = item.syntax().text_range();
                    scope.uses.push(ScopedUse {
                        names,
                        start: usize::from(range.start()),
                        end: usize::from(range.end()),
                        members,
                    });
                }
            }
            _ => {}
        }
    }
    let spelled = parse
        .syntax_node()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && token.text() == "macro_export")
        .count();
    let listed = scope.macros.iter().filter(|scoped| scoped.exported).count();
    scope.export_opaque |= spelled != listed;
    Some(scope)
}

/// How `source` calls one macro: bare (`name!`) or by the path
/// `crate::name!`, at the file's top level or nested.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GeneratorCalls {
    pub bare: bool,
    /// A bare call inside an inline module or function. A `use` is not
    /// visible in a child module, so a macro named by one reaches none.
    pub nested_bare: bool,
    /// Byte range of the `crate::` in front of each top-level `crate::name!`.
    pub crate_prefixes: Vec<(usize, usize)>,
}

/// The calls `source` makes to each of `names`, keyed by name. Only a bare
/// `name!` and a top-level `crate::name!` are recorded; any other path (a
/// different macro by that name) is not this expander's to resolve. `None`
/// when `source` does not parse cleanly.
pub(crate) fn generator_calls(
    source: &str,
    names: &std::collections::BTreeSet<String>,
) -> Option<std::collections::BTreeMap<String, GeneratorCalls>> {
    let parse = parse_clean_source_file(source)?;
    let mut calls = std::collections::BTreeMap::<String, GeneratorCalls>::new();
    for call in parse
        .syntax_node()
        .descendants()
        .filter_map(ast::MacroCall::cast)
    {
        let Some(path) = call.path() else {
            continue;
        };
        let Some(name) = path
            .segment()
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
            .filter(|name| names.contains(name))
        else {
            continue;
        };
        let top_level = call
            .syntax()
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::SOURCE_FILE);
        let entry = calls.entry(name).or_default();
        match path.qualifier() {
            None => {
                entry.bare = true;
                entry.nested_bare |= !top_level;
            }
            Some(qualifier) if top_level && qualifier.qualifier().is_none() => {
                let head: String = qualifier
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let segment_start = path
                    .segment()
                    .map(|s| usize::from(s.syntax().text_range().start()));
                if head == "crate"
                    && let Some(end) = segment_start
                {
                    entry
                        .crate_prefixes
                        .push((usize::from(path.syntax().text_range().start()), end));
                }
            }
            Some(_) => {}
        }
    }
    Some(calls)
}

/// The names of an attribute-free `use crate::name;` or `use crate::{a, b};`,
/// and for the group form each member's byte range.
fn crate_path_import(item: &ast::Use) -> Option<(Vec<String>, Vec<(String, usize, usize)>)> {
    if item.attrs().next().is_some() {
        return None;
    }
    let tree = item.use_tree()?;
    if tree.rename().is_some() || tree.star_token().is_some() {
        return None;
    }
    let plain = |name: &str| {
        !name.is_empty()
            && !matches!(name, "self" | "super" | "crate")
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    };
    let squeeze =
        |text: String| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let Some(list) = tree.use_tree_list() else {
        let text = squeeze(tree.path()?.syntax().text().to_string());
        let (head, name) = text.split_once("::")?;
        let identifier = plain(name) && !name.contains("::");
        return (head == "crate" && identifier).then(|| (vec![name.to_string()], Vec::new()));
    };
    // A group's rewrite would drop a visibility, so only private groups.
    if item.visibility().is_some() || squeeze(tree.path()?.syntax().text().to_string()) != "crate" {
        return None;
    }
    let mut members = Vec::new();
    let mut pending: Option<(String, usize)> = None;
    for element in list.syntax().children_with_tokens() {
        match element {
            ra_ap_syntax::NodeOrToken::Node(node) => {
                let member = ast::UseTree::cast(node)?;
                if member.rename().is_some()
                    || member.star_token().is_some()
                    || member.use_tree_list().is_some()
                    || pending.is_some()
                {
                    return None;
                }
                let name = squeeze(member.path()?.syntax().text().to_string());
                if !plain(&name) {
                    return None;
                }
                pending = Some((name, usize::from(member.syntax().text_range().start())));
            }
            ra_ap_syntax::NodeOrToken::Token(token) if token.kind() == SyntaxKind::COMMA => {
                let (name, start) = pending.take()?;
                members.push((name, start, usize::from(token.text_range().end())));
            }
            ra_ap_syntax::NodeOrToken::Token(_) => {}
        }
    }
    if let Some((name, start)) = pending {
        // The last member has no comma after it; a preceding comma stays as
        // a trailing one when it is blanked, so its range ends at the name.
        let end = list
            .syntax()
            .children()
            .filter_map(ast::UseTree::cast)
            .last()
            .map(|last| usize::from(last.syntax().text_range().end()))?;
        members.push((name, start, end));
    }
    let names: Vec<String> = members.iter().map(|(name, ..)| name.clone()).collect();
    let distinct: std::collections::BTreeSet<&String> = names.iter().collect();
    (!names.is_empty() && distinct.len() == names.len()).then_some((names, members))
}

/// Whether the macro is `#[macro_export]`, and whether every attribute it
/// carries is one that cannot change whether or where it exists.
fn macro_attributes(rule: &ast::MacroRules) -> (bool, bool) {
    let mut exported = false;
    let mut plain = true;
    for attr in rule.attrs() {
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
            "macro_export" => exported = true,
            other
                if ["allow(", "warn(", "deny(", "doc"]
                    .iter()
                    .any(|p| other.starts_with(p)) => {}
            _ => plain = false,
        }
    }
    (exported, plain)
}

/// Byte offset of the first `name!` token pair in the code (not a comment or
/// a literal, and not the tail of a longer identifier) for any `names`.
pub(crate) fn first_macro_call(
    source: &str,
    names: &std::collections::BTreeSet<String>,
) -> Option<usize> {
    let parse = parse_clean_source_file(source)?;
    let mut previous: Option<ra_ap_syntax::SyntaxToken> = None;
    for token in parse
        .syntax_node()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        // A definition's body is a template, not an invocation.
        .filter(|token| {
            !token
                .parent_ancestors()
                .any(|node| matches!(node.kind(), SyntaxKind::MACRO_RULES | SyntaxKind::MACRO_DEF))
        })
    {
        if token.kind() == SyntaxKind::BANG
            && let Some(name) = previous.as_ref().filter(|p| p.kind() == SyntaxKind::IDENT)
            && names.contains(name.text())
        {
            return Some(usize::from(name.text_range().start()));
        }
        previous = Some(token);
    }
    None
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
pub(crate) fn splice_imported_macros(source: &str, imported: &ImportedMacros) -> Option<String> {
    if imported.definitions.contains('\n') {
        return None;
    }
    let parse = parse_clean_source_file(source)?;
    let first = parse.tree().items().next()?;
    let offset = usize::from(first.syntax().text_range().start());
    let mut body = source.to_string();
    // `use crate::name;` named the macro by path; the definition now precedes
    // the file, so the import is blanked (newlines kept) instead of left to
    // read as a second, ambiguous source of the same name.
    let blank = |body: &mut String, start: usize, end: usize| -> Option<()> {
        let blanked: String = source
            .get(start..end)?
            .chars()
            .flat_map(|c| {
                // One space per byte keeps every later byte range valid.
                let fill = if c == '\n' { '\n' } else { ' ' };
                std::iter::repeat_n(fill, c.len_utf8())
            })
            .collect();
        body.replace_range(start..end, &blanked);
        Some(())
    };
    if !imported.path_imports.is_empty()
        && let Some(scope) = macro_scope(source)
    {
        for import in &scope.uses {
            let hoisted = |name: &String| imported.path_imports.contains(name);
            if import.names.iter().all(hoisted) {
                blank(&mut body, import.start, import.end)?;
            } else {
                // The rest of a group stays imported; a comma left behind by
                // the last member is a valid trailing comma.
                for (name, start, end) in &import.members {
                    if hoisted(name) {
                        blank(&mut body, *start, *end)?;
                    }
                }
            }
        }
    }
    for &(start, end) in &imported.blanked_prefixes {
        blank(&mut body, start, end)?;
    }
    let mut spliced = String::with_capacity(body.len() + imported.definitions.len() + 1);
    spliced.push_str(body.get(..offset)?);
    spliced.push_str(&imported.definitions);
    spliced.push('\n');
    spliced.push_str(body.get(offset..)?);
    Some(spliced)
}

/// What a file sees from other files (#6925): one line of `macro_rules!`
/// items, and the names among them that the file also spells as a top-level
/// `use crate::name;` (a `#[macro_export]` macro named by path).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportedMacros {
    pub definitions: String,
    pub path_imports: Vec<String>,
    /// Byte ranges of `crate::` in front of top-level `crate::name!` calls
    /// the definitions answer; blanked so the call reads as the textual one.
    pub blanked_prefixes: Vec<(usize, usize)>,
}

impl ImportedMacros {
    pub(crate) fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
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
    fn a_call_is_a_name_and_a_bang_in_code() {
        let names: std::collections::BTreeSet<String> = ["t".to_string()].into();
        assert_eq!(first_macro_call("assert!(x); t!(a);", &names), Some(12));
        assert_eq!(first_macro_call("fn f() { /* t!(x) */ }", &names), None);
        let inert =
            "macro_rules! unused { () => { t!(earlier); } }\n#[macro_use] mod macros;\nt!(x);\n";
        assert_eq!(first_macro_call(inert, &names), inert.rfind("t!(x)"));
        assert_eq!(first_macro_call("const S: &str = \"t!(x)\";", &names), None);
    }

    fn calls_of(source: &str, name: &str) -> GeneratorCalls {
        let names: std::collections::BTreeSet<String> = [name.to_string()].into();
        generator_calls(source, &names)
            .and_then(|mut calls| calls.remove(name))
            .unwrap_or_default()
    }

    #[test]
    fn a_path_import_reaches_only_top_level_calls() {
        let top = calls_of("use crate::t;\nt!(a);\nt!(b);\n", "t");
        assert!(top.bare && !top.nested_bare);
        assert!(calls_of("use crate::t;\nmod tests { t!(a); }\n", "t").nested_bare);
        assert!(calls_of("use crate::t;\nfn f() { t!(a); }\n", "t").nested_bare);
        assert_eq!(
            calls_of("use crate::t;\nu!(a);\n", "t"),
            GeneratorCalls::default()
        );
    }

    #[test]
    fn crate_path_calls_record_the_prefix_only_at_top_level() {
        let source = "crate::t!(a);\ncrate :: t!(b);\nother::t!(c);\nself::t!(d);\nfn f() { crate::t!(e); }\ncrate::x::t!(f);\n";
        let calls = calls_of(source, "t");
        assert!(!calls.bare, "no bare call: {calls:?}");
        let prefixes: Vec<&str> = calls
            .crate_prefixes
            .iter()
            .filter_map(|&(start, end)| source.get(start..end))
            .collect();
        assert_eq!(prefixes, vec!["crate::", "crate :: "]);
    }

    fn uses_of(source: &str) -> Vec<Vec<String>> {
        macro_scope(source)
            .map(|scope| scope.uses.into_iter().map(|import| import.names).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_group_of_plain_crate_names_is_a_path_import() {
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(uses_of("use crate::{a, b};\n"), vec![names(&["a", "b"])]);
        assert_eq!(uses_of("use crate::{a, b,};\n"), vec![names(&["a", "b"])]);
        assert_eq!(uses_of("use crate::a;\n"), vec![names(&["a"])]);
        for refused in [
            "use crate::{a, b as c};\n",
            "use crate::{a, nested::b};\n",
            "use crate::{a, *};\n",
            "use crate::{a, self};\n",
            "use crate::{a, a};\n",
            "use crate::{};\n",
            "pub use crate::{a, b};\n",
            "#[cfg(test)]\nuse crate::{a, b};\n",
            "use other::{a, b};\n",
            "use crate::m::{a, b};\n",
        ] {
            assert!(uses_of(refused).is_empty(), "{refused}");
        }
    }

    #[test]
    fn blanking_one_group_member_keeps_the_rest_valid() {
        let source = "use crate::{t, helper, u};\nt!(a);\nu!(b);\n";
        let imported = |names: &[&str]| ImportedMacros {
            definitions: "macro_rules! t { () => {} } macro_rules! u { () => {} }".to_string(),
            path_imports: names.iter().map(|n| n.to_string()).collect(),
            ..ImportedMacros::default()
        };
        let both = splice_imported_macros(source, &imported(&["t", "u"])).unwrap_or_default();
        assert!(both.contains("use crate::{   helper,  "), "{both}");
        assert!(parse_clean_source_file(&both).is_some());
        let first = splice_imported_macros(source, &imported(&["t"])).unwrap_or_default();
        assert!(first.contains("use crate::{   helper, u};"), "{first}");
        let all = "use crate::{t, u,};\nt!(a);\n";
        let blanked = splice_imported_macros(all, &imported(&["t", "u"])).unwrap_or_default();
        assert!(!blanked.contains("use crate"), "{blanked}");
        assert_eq!(
            blanked.len(),
            all.len() + imported(&[]).definitions.len() + 1
        );
    }

    #[test]
    fn blanking_a_crate_prefix_leaves_a_bare_call() {
        let source = "crate::t!(a);\nfn f() {}\n";
        let calls = calls_of(source, "t");
        let imported = ImportedMacros {
            definitions: "macro_rules! t { () => {} }".to_string(),
            blanked_prefixes: calls.crate_prefixes,
            ..ImportedMacros::default()
        };
        let spliced = splice_imported_macros(source, &imported).unwrap_or_default();
        assert_eq!(
            spliced,
            "macro_rules! t { () => {} }\n       t!(a);\nfn f() {}\n"
        );
    }

    #[test]
    fn blanking_a_path_import_keeps_byte_ranges_with_multibyte_text() {
        let source = "use crate::t; // é\nuse crate::u; // ü€\nt!(a);\nu!(b);\n";
        let imported = ImportedMacros {
            definitions: "macro_rules! t { () => {} } macro_rules! u { () => {} }".to_string(),
            path_imports: vec!["t".to_string(), "u".to_string()],
            ..ImportedMacros::default()
        };
        let spliced = splice_imported_macros(source, &imported).unwrap_or_default();
        assert!(
            !spliced.is_empty(),
            "premise: the source parses and splices"
        );
        assert_eq!(
            spliced.matches('\n').count(),
            source.matches('\n').count() + 1
        );
        assert!(!spliced.contains("use crate"), "{spliced}");
        assert!(spliced.ends_with("t!(a);\nu!(b);\n"), "{spliced}");
    }

    #[test]
    fn splicing_lands_after_inner_attributes_and_adds_one_line() {
        let source = "#![no_std]\n\nfn first() {}\nfn second() {}\n";
        let imported = ImportedMacros {
            definitions: "macro_rules! m { () => {} }".to_string(),
            path_imports: Vec::new(),
            ..ImportedMacros::default()
        };
        let spliced = splice_imported_macros(source, &imported);
        assert_eq!(
            spliced.as_deref(),
            Some("#![no_std]\n\nmacro_rules! m { () => {} }\nfn first() {}\nfn second() {}\n")
        );
        let single = |definitions: &str| ImportedMacros {
            definitions: definitions.to_string(),
            path_imports: Vec::new(),
            ..ImportedMacros::default()
        };
        assert_eq!(
            splice_imported_macros("", &single("x")),
            None,
            "no item to precede"
        );
        assert_eq!(splice_imported_macros(source, &single("a\nb")), None);
    }
}
