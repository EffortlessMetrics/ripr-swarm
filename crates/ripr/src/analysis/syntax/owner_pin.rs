//! Bounded execution context for owner-return pins, not a Rust resolver.
//!
//! Oracle extraction intentionally retains deferred assertions. This query
//! admits only uniquely identified assertions on ordinary statement paths,
//! optionally through one syntactically bound, directly invoked closure.

use super::parse_clean_source_file;
use super::ra::{LineIndex, slice_macro_call_text, slice_text};
use crate::analysis::facts::cfg_predicates::attribute_test_build_availability;
use ra_ap_syntax::{
    AstNode, SyntaxNode, TextSize,
    ast::{self, HasArgList, HasAttrs, HasName},
};
use std::collections::{BTreeMap, BTreeSet};

type AssertionKey = (usize, String);
type FunctionKey = (usize, usize, String);

#[derive(Clone, Debug, Default)]
pub(crate) struct OwnerPinAssertions {
    functions: BTreeMap<FunctionKey, FunctionAssertions>,
    module_declarations: BTreeMap<(usize, String), bool>,
}

#[derive(Clone, Debug, Default)]
struct FunctionAssertions {
    body: String,
    assertions: BTreeSet<AssertionKey>,
    macros: BTreeSet<String>,
}

impl OwnerPinAssertions {
    pub(crate) fn admits_module_declaration(&self, line: usize, declaration: &str) -> bool {
        self.module_declarations
            .get(&(line, declaration.to_string()))
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn admits(
        &self,
        function: (usize, usize, &str),
        body: &str,
        assertion: (usize, &str),
        ambiguous_macros: &BTreeSet<String>,
    ) -> bool {
        self.functions
            .get(&(function.0, function.1, function.2.to_string()))
            .is_some_and(|facts| {
                facts.body == body
                    && facts.macros.is_disjoint(ambiguous_macros)
                    && facts
                        .assertions
                        .contains(&(assertion.0, assertion.1.to_string()))
            })
    }
}

/// Visible bindings that may shadow trusted macros. Visibility and namespace
/// are intentionally not resolved; each test consults only names it uses.
/// Unknown macro imports affect all trusted names, including cross-file scope.
pub(crate) fn trusted_macro_binding_ambiguities(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
) -> BTreeSet<String> {
    macro_binding_ambiguities(source, packages, trusted, &BTreeSet::new())
}

/// Apply the same binding/import/opaque-expansion authority to candidate empty
/// macros. Only the declaring file may exempt its exact local declaration.
pub(crate) fn empty_macro_binding_ambiguities(
    source: &str,
    packages: &BTreeSet<String>,
    names: &BTreeSet<String>,
    declaring_file: bool,
) -> BTreeSet<String> {
    let trusted: Vec<_> = names.iter().map(String::as_str).collect();
    let allowed = if declaring_file {
        names.clone()
    } else {
        BTreeSet::new()
    };
    macro_binding_ambiguities(source, packages, &trusted, &allowed)
}

fn macro_binding_ambiguities(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    allowed_empty: &BTreeSet<String>,
) -> BTreeSet<String> {
    macro_binding_scan(source, packages, trusted, allowed_empty)
        .unwrap_or_else(|| trusted.iter().map(|name| (*name).to_string()).collect())
}

/// One file's macro-binding scan, kept apart from the run-wide union: the
/// names it may shadow, or `None` when it may shadow any name (an unclean
/// parse, `#[macro_use]`, `no_implicit_prelude`, a foreign glob import).
/// The diff scope (#5320) reads the `None` case for files it withholds.
pub(crate) fn macro_binding_scan(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    allowed_empty: &BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    let mut ambiguous = BTreeSet::new();
    if !source.contains("macro")
        && !source.contains("use")
        && !source.contains("no_implicit_prelude")
        && !source.contains('!')
    {
        return Some(ambiguous);
    }
    let parse = parse_clean_source_file(source)?;
    for node in parse.tree().syntax().descendants() {
        let definition = ast::MacroRules::cast(node.clone())
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()));
        if let Some(name) = definition {
            let name = name.text().to_string();
            let name = name.trim_start_matches("r#");
            let admitted_declaration = allowed_empty.contains(name)
                && ast::MacroRules::cast(node.clone()).is_some_and(|item| empty_catch_all(&item));
            if trusted.contains(&name) && !admitted_declaration {
                ambiguous.insert(name.to_string());
            }
        }
        if let Some(attr) = ast::Attr::cast(node.clone())
            && attr
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| matches!(token.text(), "macro_use" | "no_implicit_prelude"))
        {
            return None;
        }
        if let Some(call) = ast::MacroCall::cast(node.clone())
            && call
                .path()
                .is_some_and(|path| !is_trusted_macro(&path.syntax().text().to_string(), trusted))
            && let Some(tree) = call.token_tree()
        {
            for token in tree
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
            {
                let name = token.text().trim_start_matches("r#");
                if trusted.contains(&name) {
                    ambiguous.insert(name.to_string());
                }
            }
        }
        if let Some(import) = ast::Use::cast(node) {
            let tree = import.use_tree()?;
            let root = tree
                .path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            let root = root
                .trim_start_matches("::")
                .split("::")
                .next()
                .unwrap_or("")
                .trim();
            let own = matches!(root, "crate" | "self" | "super")
                || packages
                    .iter()
                    .any(|package| package.replace('-', "_") == root);
            for item in tree.syntax().descendants().filter_map(ast::UseTree::cast) {
                if item.star_token().is_some() && !own {
                    return None;
                }
                let name = if let Some(rename) = item.rename() {
                    rename.name().map(|name| name.text().to_string())
                } else if item.use_tree_list().is_none() {
                    item.path()
                        .and_then(|path| path.segment())
                        .and_then(|segment| segment.name_ref())
                        .map(|name| name.text().to_string())
                } else {
                    None
                };
                if let Some(name) = name {
                    let name = name.trim_start_matches("r#");
                    if trusted.contains(&name) {
                        ambiguous.insert(name.to_string());
                    }
                }
            }
        }
    }
    Some(ambiguous)
}

/// This recognizes one bounded syntax form, not a macro evaluator: a sole
/// `($($name:tt)*) => {}` rule consumes any invocation and emits no tokens.
/// Other matchers, arms, attributes and nonempty transcribers stay opaque.
fn empty_catch_all(item: &ast::MacroRules) -> bool {
    if item.attrs().next().is_some() {
        return false;
    }
    let Some(tree) = item.token_tree() else {
        return false;
    };
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let mut text: Vec<_> = tokens.iter().map(|token| token.text()).collect();
    if text.len() == 17 && text.get(15) == Some(&";") {
        text.remove(15);
    }
    // Token-tree parsing retains `=` and `>` as separate punctuation tokens;
    // unlike expression grammar it does not combine them into FAT_ARROW.
    text.len() == 16
        && tokens
            .get(5)
            .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::IDENT)
        && text[..5] == ["{", "(", "$", "(", "$"]
        && text[6..] == [":", "tt", ")", "*", ")", "=", ">", "{", "}", "}"]
}

fn local_empty_macros(root: &SyntaxNode) -> BTreeMap<String, ast::MacroRules> {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut candidates = BTreeMap::new();
    for node in root.descendants() {
        let rule = ast::MacroRules::cast(node.clone());
        let name = rule
            .as_ref()
            .and_then(|item| item.name())
            .or_else(|| ast::MacroDef::cast(node.clone()).and_then(|item| item.name()));
        let Some(name) = name else {
            continue;
        };
        let name = name.text().to_string();
        *counts.entry(name.clone()).or_default() += 1;
        if !name.starts_with("r#")
            && let Some(rule) = rule
            && empty_catch_all(&rule)
            && rule.syntax().parent().is_some_and(|parent| {
                ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
            })
        {
            candidates.insert(name, rule);
        }
    }
    candidates.retain(|name, _| counts.get(name) == Some(&1));
    candidates
}

pub(crate) fn local_empty_macro_names(source: &str) -> BTreeSet<String> {
    parse_clean_source_file(source)
        .map(|parse| {
            local_empty_macros(parse.tree().syntax())
                .into_keys()
                .collect()
        })
        .unwrap_or_default()
}

fn resolves_empty_local(call: &ast::MacroCall, empty: &BTreeMap<String, ast::MacroRules>) -> bool {
    let Some(path) = call.path() else {
        return false;
    };
    let Some(definition) = empty.get(&path.syntax().text().to_string()) else {
        return false;
    };
    let Some(scope) = definition.syntax().parent() else {
        return false;
    };
    definition.syntax().text_range().end() <= call.syntax().text_range().start()
        && call.syntax().ancestors().any(|ancestor| ancestor == scope)
}

/// Discarded call-argument spans from the same bounded local resolver used by
/// execution admission. This only removes evidence; workspace macro ambiguity
/// still independently refuses positive assertion admission.
pub(super) fn empty_local_macro_invocation_ranges(
    root: &SyntaxNode,
) -> Vec<std::ops::Range<usize>> {
    let empty = local_empty_macros(root);
    if empty.is_empty() {
        return Vec::new();
    }
    root.descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| resolves_empty_local(call, &empty))
        .map(|call| {
            let range = call.syntax().text_range();
            u32::from(range.start()) as usize..u32::from(range.end()) as usize
        })
        .collect()
}

pub(crate) fn owner_pin_assertions(source: &str, trusted: &[&str]) -> OwnerPinAssertions {
    let mut result = OwnerPinAssertions::default();
    let Some(parse) = parse_clean_source_file(source) else {
        return result;
    };
    let lines = LineIndex::new(source);
    let empty_macros = local_empty_macros(parse.tree().syntax());
    for module in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
    {
        if module.item_list().is_some() {
            continue;
        }
        let (Some(token), Some(name)) = (module.mod_token(), module.name()) else {
            continue;
        };
        let key = (
            lines.line(token.text_range().start()),
            format!("mod {};", name.text()),
        );
        let admitted = supported_item_context(module.syntax());
        result
            .module_declarations
            .entry(key)
            .and_modify(|previous| *previous = false)
            .or_insert(admitted);
    }
    let mut identities = BTreeMap::<FunctionKey, usize>::new();
    for function in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
    {
        let (Some(name), Some(token), Some(body)) =
            (function.name(), function.fn_token(), function.body())
        else {
            continue;
        };
        let key = (
            lines.line(token.text_range().start()),
            lines.line_for_range_end(function.syntax().text_range().end()),
            name.text().to_string(),
        );
        *identities.entry(key.clone()).or_default() += 1;
        if function.async_token().is_some()
            || has_escape(body.syntax(), trusted, &empty_macros)
            || !supported_item_context(function.syntax())
            || function
                .attrs()
                .any(|attr| !runs_body_unchanged(&attr, function.syntax()))
        {
            continue;
        }
        let body_source = slice_text(
            source,
            token.text_range().start(),
            function.syntax().text_range().end(),
        );
        let mut candidates = BTreeMap::<AssertionKey, Vec<ast::MacroCall>>::new();
        for call in function
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            if call
                .path()
                .is_none_or(|path| path.syntax().text() != "assert_eq")
            {
                continue;
            }
            let range = call.syntax().text_range();
            let assertion = (
                lines.line(range.start()),
                slice_macro_call_text(source, range.start(), range.end()),
            );
            candidates.entry(assertion).or_default().push(call);
        }
        let macros = function
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
            .filter_map(|call| call.path())
            .map(|path| path.syntax().text().to_string())
            .collect();
        // Compute the conservative prefix boundary once per function, rather
        // than rescanning its body for every candidate assertion/invocation.
        // A nested helper or async block has its own return context.
        // The earlier escape gate still refuses returns in any closure.
        let first_return = body
            .syntax()
            .descendants()
            .filter_map(ast::ReturnExpr::cast)
            .filter(|expression| {
                expression
                    .syntax()
                    .ancestors()
                    .find(|node| {
                        ast::Fn::can_cast(node.kind())
                            || ast::BlockExpr::cast(node.clone())
                                .is_some_and(|block| block.async_token().is_some())
                    })
                    .is_some_and(|owner| owner == *function.syntax())
            })
            .map(|expression| expression.syntax().text_range().start())
            .min();
        let assertions = candidates
            .into_iter()
            .filter_map(|(key, calls)| {
                // OracleFact has line/text, not an offset. No identical spelling
                // on the same line may borrow another invocation's context.
                (calls.len() == 1
                    && eager_path(calls[0].syntax().clone(), &function, false, first_return))
                .then_some(key)
            })
            .collect();
        // Duplicate function identities are ambiguous too.
        if let std::collections::btree_map::Entry::Vacant(entry) =
            result.functions.entry(key.clone())
        {
            entry.insert(FunctionAssertions {
                body: body_source,
                assertions,
                macros,
            });
        } else {
            result.functions.insert(key, FunctionAssertions::default());
        }
    }
    result
        .functions
        .retain(|key, _| identities.get(key) == Some(&1));
    result
}

/// A libtest item cannot be nested in an executable body. Module/source
/// attributes include inner attributes on ItemList, not only outer attrs.
fn supported_item_context(item: &SyntaxNode) -> bool {
    let mut source_file = false;
    for (depth, node) in item.ancestors().enumerate() {
        if depth > 0
            && !ast::ItemList::can_cast(node.kind())
            && !ast::Module::can_cast(node.kind())
            && !ast::SourceFile::can_cast(node.kind())
        {
            return false;
        }
        if node.children().filter_map(ast::Attr::cast).any(|attr| {
            attribute_test_build_availability(&attr.syntax().text().to_string()) != Some(true)
        }) {
            return false;
        }
        source_file |= ast::SourceFile::can_cast(node.kind());
    }
    source_file
}

/// Attributes a pinned test may carry: `#[test]` itself, and the
/// `serial_test` locks, which run the unchanged body while holding a mutex or
/// file lock. Lock arguments are admitted only as bare key names
/// (`#[serial(env, db)]`): `inner_attrs = [..]` hands the body to other
/// attribute macros and `crate = ..` swaps the runtime that runs it, so any
/// other argument refuses. Every other attribute (`ignore`, `should_panic`,
/// an async runtime or a parameterizing macro) may skip, invert or rewrite
/// the body, so it keeps the test out of the pin.
fn runs_body_unchanged(attr: &ast::Attr, test: &SyntaxNode) -> bool {
    if attr.simple_name().as_deref() == Some("test") {
        return true;
    }
    if attr.excl_token().is_some() {
        return false;
    }
    let Some(path) = attr.path() else {
        return false;
    };
    let path = path.syntax().text().to_string().replace(' ', "");
    let Some(module) = test.parent() else {
        return false;
    };
    let admitted = match path.split_once("::") {
        Some(("serial_test", leaf)) => {
            SERIAL_TEST_LOCKS.contains(&leaf) && serial_test_path_is_the_crate(&module)
        }
        Some(_) => false,
        None => bare_lock_bound_in_module(&path, &module) && serial_test_path_is_the_crate(&module),
    };
    admitted && lock_arguments_are_bare_keys(attr)
}

const SERIAL_TEST_LOCKS: &[&str] = &["serial", "parallel", "file_serial", "file_parallel"];

/// Whether a bare lock attribute `name` resolves to a serial_test lock in the
/// test's own module (`module` is its item list or source file). Only an
/// explicit `use serial_test::<lock>` (optionally renamed) that is a direct
/// item of that module counts: an explicit import shadows globs and the
/// `#[macro_use]` prelude, and a second explicit binding of the name there
/// would not compile, so another one refuses. Imports in function bodies,
/// sibling modules or macro token trees bind nothing for this attribute. The
/// caller still checks that `serial_test` in that path is the crate.
fn bare_lock_bound_in_module(name: &str, module: &SyntaxNode) -> bool {
    let name = unraw(name);
    let mut bound = false;
    for leaf in module_use_leaves(module) {
        let UseBinding::Name(binding) = &leaf.binding else {
            continue;
        };
        if unraw(binding) != name {
            continue;
        }
        let is_lock = leaf.path.len() == 2
            && leaf.path[0] == "serial_test"
            && SERIAL_TEST_LOCKS.contains(&leaf.path[1].as_str());
        if !is_lock || bound {
            return false;
        }
        bound = true;
    }
    bound
}

/// Whether `serial_test::..` in the test's module resolves to the extern
/// crate: the module binds no item named `serial_test` (`mod`, `use .. as`,
/// `extern crate .. as`), has no item-position macro call or other
/// attribute or derive macro on a direct item that could emit one, and
/// imports no glob except `super::*` into an enclosing module of this file
/// that satisfies the same rule. Any other glob may bring a `serial_test`
/// that shadows the extern prelude, so it refuses.
fn serial_test_path_is_the_crate(module: &SyntaxNode) -> bool {
    for item in module.children().filter_map(ast::Item::cast) {
        if item
            .attrs()
            .any(|attr| !attribute_emits_no_items(&attr, module))
        {
            return false;
        }
        match &item {
            ast::Item::MacroCall(_) => return false,
            ast::Item::Module(inner)
                if inner
                    .name()
                    .is_some_and(|name| unraw(name.text()) == "serial_test") =>
            {
                return false;
            }
            ast::Item::ExternCrate(krate) => {
                let binding = match krate.rename() {
                    Some(rename) => rename.name().map(|name| name.text().to_string()),
                    None => krate.name_ref().map(|name| name.text().to_string()),
                };
                let Some(binding) = binding else {
                    return false;
                };
                let real = krate
                    .name_ref()
                    .is_some_and(|name| unraw(name.text()) == "serial_test");
                if (unraw(&binding) == "serial_test" && !real)
                    || PRELUDE_MACROS.contains(&unraw(&binding))
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    for leaf in module_use_leaves(module) {
        match &leaf.binding {
            UseBinding::Name(binding) => {
                let real = leaf.path.len() == 1 && leaf.path[0] == "serial_test";
                if unraw(binding) == "serial_test" && !real {
                    return false;
                }
                // `derive`, the std derives and `test` are prelude macros an
                // import can rebind, which voids the allowlist below.
                if PRELUDE_MACROS.contains(&unraw(binding)) {
                    return false;
                }
            }
            UseBinding::Glob => {
                let into_parent = leaf.path.len() == 1 && leaf.path[0] == "super";
                let parent = module
                    .parent()
                    .filter(|owner| ast::Module::can_cast(owner.kind()))
                    .and_then(|owner| owner.parent());
                match parent {
                    Some(parent) if into_parent => {
                        if !serial_test_path_is_the_crate(&parent) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
    }
    true
}

/// Built-in attributes and std derives expand to no new items, provided the
/// caller has refused any import rebinding a `PRELUDE_MACROS` name. A
/// `serial_test` lock is trusted here because it is the crate's only when the
/// surrounding check passes; a bare name must be bound to a lock (renames
/// included).
/// Anything else (`cfg_attr` included) may emit a `serial_test` module.
fn attribute_emits_no_items(attr: &ast::Attr, module: &SyntaxNode) -> bool {
    const BUILTIN: &[&str] = &[
        "test",
        "cfg",
        "allow",
        "warn",
        "deny",
        "forbid",
        "expect",
        "doc",
        "inline",
        "cold",
        "must_use",
        "non_exhaustive",
        "repr",
        "path",
        "ignore",
        "should_panic",
        "track_caller",
        "deprecated",
        "macro_use",
        "macro_export",
    ];
    let Some(path) = attr.path() else {
        // `cfg(..)` parses as its own meta kind with no path.
        return attr.simple_name().as_deref() == Some("cfg");
    };
    let path = path.syntax().text().to_string().replace(' ', "");
    if let Some(("serial_test", leaf)) = path.split_once("::") {
        return SERIAL_TEST_LOCKS.contains(&leaf);
    }
    if path == "derive" {
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            return false;
        };
        return meta.token_tree().is_some_and(|tree| {
            let text = tree.syntax().text().to_string();
            text.trim_start_matches('(')
                .trim_end_matches(')')
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .all(|name| STD_DERIVES.contains(&name))
        });
    }
    BUILTIN.contains(&path.as_str())
        || (!path.contains("::") && bare_lock_bound_in_module(&path, module))
}

const STD_DERIVES: &[&str] = &[
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
];

/// Prelude macro names `attribute_emits_no_items` trusts that an import can
/// rebind. Built-in attributes (`cfg`, `inline`, ..) cannot be rebound: an
/// import of one is an ambiguity error. A `#[macro_use]` extern crate can
/// also rebind these names with no import; `macro_binding_scan` refuses any
/// `macro_use` in the scanned files, which this check relies on.
const PRELUDE_MACROS: &[&str] = &[
    "derive",
    "test",
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
];

fn unraw(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// The use leaves of the module's own direct `use` items.
fn module_use_leaves(module: &SyntaxNode) -> Vec<UseLeaf> {
    let mut leaves = Vec::new();
    for item in module.children().filter_map(ast::Use::cast) {
        if let Some(tree) = item.use_tree() {
            flatten_use_tree(&tree, &[], &mut leaves);
        }
    }
    leaves
}

/// No argument list, or one holding only identifiers and commas.
fn lock_arguments_are_bare_keys(attr: &ast::Attr) -> bool {
    let tree = match attr.meta() {
        Some(ast::Meta::PathMeta(_)) => return true,
        Some(ast::Meta::TokenTreeMeta(meta)) => match meta.token_tree() {
            Some(tree) => tree,
            None => return false,
        },
        _ => return false,
    };
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let Some((first, rest)) = tokens.split_first() else {
        return false;
    };
    let Some((last, inner)) = rest.split_last() else {
        return false;
    };
    first.text() == "("
        && last.text() == ")"
        && inner.iter().all(|token| {
            token.text() == ","
                || (token.kind() == ra_ap_syntax::SyntaxKind::IDENT
                    && !matches!(token.text(), "inner_attrs" | "crate" | "path"))
        })
}

struct UseLeaf {
    path: Vec<String>,
    binding: UseBinding,
}

enum UseBinding {
    Glob,
    Name(String),
}

/// Expand one use tree into its leaves with their full paths and the name
/// each binds (`as` rename, or the last segment). `as _` binds nothing.
fn flatten_use_tree(tree: &ast::UseTree, prefix: &[String], out: &mut Vec<UseLeaf>) {
    let mut path = prefix.to_vec();
    if let Some(own) = tree.path() {
        path.extend(
            own.syntax()
                .text()
                .to_string()
                .split("::")
                .map(|segment| unraw(segment.trim()).to_string())
                .filter(|segment| !segment.is_empty()),
        );
    }
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            flatten_use_tree(&child, &path, out);
        }
        return;
    }
    if tree.star_token().is_some() {
        out.push(UseLeaf {
            path,
            binding: UseBinding::Glob,
        });
        return;
    }
    let name = match tree.rename() {
        Some(rename) => match rename.name() {
            Some(name) => name.text().to_string(),
            None => return,
        },
        None => match path.last() {
            Some(last) if last != "self" => last.clone(),
            Some(_) => match path.iter().rev().nth(1) {
                Some(parent) => parent.clone(),
                None => return,
            },
            None => return,
        },
    };
    out.push(UseLeaf {
        path,
        binding: UseBinding::Name(name),
    });
}

fn has_escape(
    body: &SyntaxNode,
    trusted: &[&str],
    empty: &BTreeMap<String, ast::MacroRules>,
) -> bool {
    body.descendants().any(|node| {
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            // Discarded arguments are not executed. Cross-file/import/shadow
            // ambiguity is checked by the shared binding authority at admission.
            if resolves_empty_local(&call, empty) {
                return false;
            }
            if call
                .path()
                .is_none_or(|path| !is_trusted_macro(&path.syntax().text().to_string(), trusted))
            {
                return true;
            }
            // Macro operands are opaque to AST descendant walks. Refuse
            // hidden exits and nested expansion, but not boolean negation.
            if call
                .token_tree()
                .is_some_and(|tree| opaque_macro_operand(&tree))
            {
                return true;
            }
        }

        // Root returns are checked against the actual invocation's statement
        // prefix below. A later return cannot undo an earlier assertion.
        // Closure returns retain the existing conservative refusal, including
        // returns in closures other than the selected one.
        (ast::ReturnExpr::can_cast(node.kind())
            && node
                .ancestors()
                .any(|parent| ast::ClosureExpr::can_cast(parent.kind())))
            || (ast::TryExpr::can_cast(node.kind())
                && node
                    .ancestors()
                    .any(|parent| ast::ClosureExpr::can_cast(parent.kind())))
            || ast::BreakExpr::can_cast(node.kind())
            || ast::ContinueExpr::can_cast(node.kind())
            || ast::YieldExpr::can_cast(node.kind())
    })
}

fn opaque_macro_operand(tree: &ast::TokenTree) -> bool {
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.iter().any(|token| {
        matches!(
            token.text(),
            "return" | "break" | "continue" | "yield" | "?"
        )
    }) || tokens.windows(3).any(|tokens| {
        tokens[0].kind() == ra_ap_syntax::SyntaxKind::IDENT
            && tokens[1].text() == "!"
            && matches!(tokens[2].text(), "(" | "[" | "{")
    })
}

fn is_trusted_macro(path: &str, trusted: &[&str]) -> bool {
    // Qualified roots can themselves be rebound. Without name resolution,
    // only bare names with the workspace binding check are established.
    trusted.contains(&path)
}

fn eager_path(
    mut node: SyntaxNode,
    function: &ast::Fn,
    through_closure: bool,
    first_return: Option<TextSize>,
) -> bool {
    // When this query follows a bound closure, recursion below resets this
    // coordinate to the real invocation, not the earlier closure definition.
    let execution_start = node.text_range().start();
    loop {
        if node
            .children()
            .any(|child| ast::Attr::can_cast(child.kind()))
        {
            return false;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        if parent == *function.syntax() {
            return function.body().is_some_and(|body| {
                body.syntax() == &node
                    && first_return.is_none_or(|position| position >= execution_start)
            });
        }
        if let Some(closure) = ast::ClosureExpr::cast(parent.clone()) {
            if through_closure {
                return false;
            }
            let Some(call) = closure_invocation(&closure, function, first_return) else {
                return false;
            };
            return eager_path(call.syntax().clone(), function, true, first_return);
        }
        if let Some(block) = ast::BlockExpr::cast(parent.clone()) {
            if block.async_token().is_some()
                || block.const_token().is_some()
                || block.gen_token().is_some()
                || block.try_block_modifier().is_some()
                || block.label().is_some()
            {
                return false;
            }
        } else if let Some(binding) = ast::LetStmt::cast(parent.clone()) {
            if binding.let_else().is_some()
                || binding
                    .initializer()
                    .is_none_or(|expr| expr.syntax() != &node)
            {
                return false;
            }
        } else if !(ast::MacroExpr::can_cast(parent.kind())
            || ast::ExprStmt::can_cast(parent.kind())
            || ast::StmtList::can_cast(parent.kind())
            || ast::ParenExpr::can_cast(parent.kind()))
        {
            return false;
        }
        node = parent;
    }
}

fn closure_invocation(
    closure: &ast::ClosureExpr,
    function: &ast::Fn,
    first_return: Option<TextSize>,
) -> Option<ast::CallExpr> {
    if closure.async_token().is_some()
        || closure.const_token().is_some()
        || closure.gen_token().is_some()
        || closure
            .param_list()
            .is_none_or(|params| params.params().next().is_some())
        || closure
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let mut expression = closure.syntax().clone();
    while let Some(parent) = expression
        .parent()
        .filter(|node| ast::ParenExpr::can_cast(node.kind()))
    {
        expression = parent;
    }
    if let Some(call) = expression.parent().and_then(ast::CallExpr::cast) {
        return (call.expr().is_some_and(|expr| expr.syntax() == &expression)
            && no_arguments(&call))
        .then_some(call);
    }
    let binding = expression.parent().and_then(ast::LetStmt::cast)?;
    if binding
        .initializer()
        .is_none_or(|expr| expr.syntax() != &expression)
        || binding.let_else().is_some()
        || binding
            .syntax()
            .children()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    if !eager_path(binding.syntax().clone(), function, true, first_return) {
        return None;
    }
    let scope = binding.syntax().parent()?;
    if !ast::StmtList::can_cast(scope.kind()) {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some()
        || pattern.ref_token().is_some()
        || pattern.at_token().is_some()
    {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    if name.starts_with("r#") {
        return None;
    }
    // Count tokens, including token trees, not only call expressions: aliases,
    // shadowing, macro arguments, mutation and capture are all unestablished.
    if function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text().trim_start_matches("r#") == name)
        .count()
        != 2
    {
        return None;
    }
    function
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .find(|call| {
            call.syntax().text_range().start() >= binding.syntax().text_range().end()
                && call
                    .syntax()
                    .ancestors()
                    .skip(1)
                    .find(|node| ast::StmtList::can_cast(node.kind()))
                    .as_ref()
                    == Some(&scope)
                && no_arguments(call)
                && call.expr().is_some_and(|expr| {
                    matches!(expr, ast::Expr::PathExpr(_))
                        && expr.syntax().text().to_string() == name
                })
        })
}

fn no_arguments(call: &ast::CallExpr) -> bool {
    call.arg_list()
        .is_some_and(|args| args.args().next().is_none())
}
