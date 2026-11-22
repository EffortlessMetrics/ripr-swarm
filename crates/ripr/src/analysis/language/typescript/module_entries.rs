//! Same-module entry points that reach a changed top-level function.
//!
//! A test cannot call a module-private helper, and it often calls an exported
//! function only through a value a same-module factory built:
//!
//! ```ts
//! function _defu(base, defaults) { /* changed */ }
//! export function createDefu(merger) {
//!   return (...args) => args.reduce((p, c) => _defu(p, c, "", merger), {});
//! }
//! export const defu = createDefu();
//! ```
//!
//! A test calling `defu(...)` runs `_defu` through the factory's returned
//! closure. This module records, per top-level function owner, the exported
//! names whose code reaches it through a bounded same-module call graph, so the
//! relation layer can relate tests of those entries to the owner. The graph is
//! syntax-only and fail-closed: an edge exists only for a bare call to a
//! top-level declaration name whose every mention in the enclosing function is
//! a bare call, a function's own edges exclude the functions it returns, and a
//! factory product reaches only what the factory's directly returned function
//! calls. Reach through an entry is weaker than a direct owner call: it is
//! admitted only when no test calls the owner itself, and the classifier never
//! promotes it to `exposed`.
//!
//! Cross-module reach (#13592): the same bounded graph also records, per
//! exported entry, the imported bindings its code bare-calls
//! ([`OutboundImportCall`]). `ReExportIndex::cross_module_entries` resolves
//! those bindings through the existing import / re-export identity and adds
//! the importer's exports as entries of the imported module's owners, under
//! the same three-hop total bound and the same weaker-than-direct contract.

use super::module_class_nodes::{
    ModuleClass, constructed_method_calls, method_nodes, module_class, this_method_calls,
};
use super::module_value_reads::{
    byte_preserving_mask, never_mutated_in_module, past_type_arguments, reads_unshadowed_value,
};
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Maximum call edges followed from an exported entry to the owner. Matches
/// the Rust helper-owner graph bound (`HELPER_OWNER_CALL_GRAPH_MAX_HOPS`).
pub(crate) const MODULE_ENTRY_MAX_HOPS: usize = 3;

/// An exported name of the owner's module whose code reaches the owner.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypeScriptModuleEntry {
    /// Exported name a test imports (`default` for the default export).
    pub(crate) name: String,
    /// `true` when this entry is the module's default export, so a default
    /// import under any local name binds it.
    pub(crate) exported_as_default: bool,
    /// `Some(kind)` when the entry is a method of the owner's own class
    /// that reaches the owner through `this.method(...)` calls
    /// (`class_entries`), not an exported top-level name. `name` is then the
    /// method name (`constructor` for a constructor).
    pub(crate) class_member: Option<TypeScriptMethodKind>,
    /// `Some` when the entry is an instance method that a bare `@owner`
    /// decorator wraps (`decorated_entries`, RIPR-SPEC-0243 rule 21). `name`
    /// is the method name.
    pub(crate) decorated: Option<TypeScriptDecoratedMethod>,
    /// `true` when the entry is an exported factory that returns a fresh
    /// instance of the owner's own class (`instance_factories`, #10402). It is
    /// not a callable entry of the owner: a test reaches a getter owner only by
    /// reading the property on a receiver the factory built, so the entry call
    /// relation ignores it.
    #[serde(default)]
    pub(crate) instance_factory: bool,
    /// Call edges from this entry's code to the owner (same-module entries),
    /// or across the whole import chain (cross-module entries). `0` for
    /// entries that are not call-graph reach (class members, decorated
    /// methods, instance factories). Bounded by `MODULE_ENTRY_MAX_HOPS`.
    #[serde(default)]
    pub(crate) hops: u8,
    /// `Some` when the entry is an export of ANOTHER module that reaches the
    /// owner through an imported binding (#13592); `None` for an export of
    /// the owner's own module.
    #[serde(default)]
    pub(crate) cross_module: Option<TypeScriptCrossModuleLink>,
}

/// Where a cross-module entry lives and which modules and bindings its reach
/// to the owner passes through, so a test that mocks or fabricates any of them
/// is refused per test.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypeScriptCrossModuleLink {
    /// The file that exports the entry name.
    pub(crate) file: PathBuf,
    /// Canonical modules the chain imports through: every imported module and
    /// every re-export barrel resolved on the way (sorted, deduplicated).
    pub(crate) via_modules: Vec<String>,
    /// Binding names the chain calls (the owner and each intermediate export),
    /// for the spy-fabrication guard.
    pub(crate) chain_names: Vec<String>,
}

/// An imported binding that code reachable from exported entries of a module
/// bare-calls (#13592). Persisted in the module's export facts so the
/// workspace-level resolution never re-parses the importer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct OutboundImportCall {
    pub(crate) import: TypeScriptImport,
    /// Exports of the importing module whose code reaches the call, each with
    /// the call edges from the export to the calling function.
    pub(crate) entries: Vec<TypeScriptModuleEntry>,
}

/// The decorated method a pass-through decorator owner reaches, with the
/// lines of the wrapper's final return, the only line whose change the
/// method's call result carries.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypeScriptDecoratedMethod {
    pub(crate) class_name: String,
    /// The class is its module's `export default class`, so a test names it
    /// through a default import under any local name.
    pub(crate) default_export: bool,
    pub(crate) return_start_line: usize,
    pub(crate) return_end_line: usize,
}

/// One top-level declaration of the module.
#[derive(Default)]
struct ModuleNode<'s> {
    /// Function texts whose bare calls count as this node's calls.
    call_texts: Vec<CallText<'s>>,
    /// Factory whose returned function this node's value is (`const p = f()`).
    /// Borrowed from an AST atom — no per-node allocation (#10087).
    factory: Option<&'s str>,
    /// Function texts the declaration returns directly, when it is a factory.
    returned_texts: Vec<CallText<'s>>,
}

/// Code whose bare calls count, plus the enclosing function text that decides
/// whether a name is rebound there. `calls` is borrowed for a directly
/// returned function (nothing blanked) and owned only when the node's own
/// text had its returned functions blanked out — large modules otherwise copy
/// every returned closure body (#10087).
struct CallText<'s> {
    /// The calling code. For a function's own node, the directly returned
    /// functions are blanked out: they run only when the product is called.
    calls: std::borrow::Cow<'s, str>,
    /// The whole enclosing function (the factory, for a returned closure), so
    /// its parameters and locals shadow the top-level binding too.
    scope: &'s str,
}

/// Module-level non-function `const` bindings the graph may follow.
#[derive(Default)]
struct ModuleValues<'s> {
    names: BTreeSet<&'s str>,
    /// Parsed offset of each binding identifier.
    decl_starts: BTreeMap<&'s str, usize>,
    /// Values an immediately invoked function builds (#15928).
    built: BTreeMap<&'s str, BuiltValue<'s>>,
    /// Top-level class declarations whose methods may be nodes (#15928).
    classes: Vec<ModuleClass<'s>>,
}

/// `const X = (function () { ... })()`: the function that builds the value.
struct BuiltValue<'s> {
    /// The whole function text. Its bare calls are the value's calls: the
    /// function runs when the module loads, and what it declares (the methods
    /// of a class it returns) runs through the value.
    text: &'s str,
    /// The function's byte range when its body declares the binding's own
    /// name (`class X {}` then `return X`): every mention of the name inside
    /// is the inner binding, not the module-level one.
    shadow: Option<std::ops::Range<usize>>,
}

/// Exported entries per top-level owner name, plus the imported bindings the
/// entries' code bare-calls. `imports` are the file's ES/CommonJS import
/// bindings (never re-export hops); only a named, non-namespace binding whose
/// local name is unique and not a top-level declaration of the module can
/// become an outbound edge. Also returns the module-level non-function `const`
/// names that are followable value nodes (#13592 B2); only those may carry
/// entries for a `ModuleFunction` owner.
pub(crate) fn module_entries_and_outbound<'a>(
    statements: &'a [Statement<'a>],
    source: &'a str,
    imports: &[TypeScriptImport],
) -> (
    BTreeMap<String, Vec<TypeScriptModuleEntry>>,
    Vec<OutboundImportCall>,
    BTreeSet<String>,
) {
    // Node keys and factories are AST atoms or string literals, so the graph
    // borrows them instead of owning a String per node (#10087 follow-up).
    let mut nodes: BTreeMap<&'a str, ModuleNode<'a>> = BTreeMap::new();
    let mut exports: BTreeMap<String, BTreeSet<TypeScriptModuleEntry>> = BTreeMap::new();
    let mut module_values = ModuleValues::default();
    let mut exported_values: BTreeSet<&'a str> = BTreeSet::new();
    for statement in statements {
        collect_statement(
            statement,
            source,
            &mut nodes,
            &mut exports,
            &mut module_values,
            &mut exported_values,
        );
    }
    let ModuleValues {
        names: mut values,
        decl_starts,
        built,
        classes,
    } = module_values;
    // A value is followable only while nothing in the file writes it; a name
    // that is also a function/factory node keeps its bare-call edges only.
    // Only values some node's code actually mentions are scanned (the mutation
    // scan is whole-module), and an exported value is never followed: another
    // module may import and mutate it.
    let mentioned: std::collections::HashSet<&str> = nodes
        .values()
        .flat_map(|node| node.call_texts.iter().chain(node.returned_texts.iter()))
        .map(|text| &*text.calls)
        // A building function and a module-private class read values too.
        .chain(built.values().map(|value| value.text))
        .chain(classes.iter().map(|class| class.text))
        .flat_map(|text| {
            text.split(|ch: char| !is_javascript_identifier_char(ch))
                .filter(|token| !token.is_empty())
        })
        .collect();
    values.retain(|name| {
        !nodes.contains_key(name) && !exported_values.contains(name) && mentioned.contains(name)
    });
    // The whole-module mask is built only when a value or class needs it.
    let mut method_nodes_of_classes = Vec::new();
    if !values.is_empty() || !classes.is_empty() {
        let masked_source = byte_preserving_mask(source, &ts_mask_literals_and_comments(source));
        values.retain(|name| {
            never_mutated_in_module(
                source,
                &masked_source,
                name,
                decl_starts.get(name).copied(),
                built.get(name).and_then(|value| value.shadow.as_ref()),
            )
        });
        method_nodes_of_classes = method_nodes(&classes, source, &masked_source, |name| {
            nodes.contains_key(name)
                || decl_starts.contains_key(name)
                || imports.iter().any(|import| import.local == name)
        });
    }
    let method_nodes_of_classes = method_nodes_of_classes;
    // Imported bindings a node may bare-call: named, value, bound once, and
    // not also a top-level declaration of this module.
    let bindings: BTreeMap<&str, &TypeScriptImport> = imports
        .iter()
        .filter(|import| {
            !import.namespace
                && import.imported.is_some()
                && !nodes.contains_key(import.local.as_str())
                && imports
                    .iter()
                    .filter(|other| other.local == import.local)
                    .count()
                    == 1
        })
        .map(|import| (import.local.as_str(), import))
        .collect();
    let names: std::collections::HashSet<&str> = nodes
        .keys()
        .copied()
        .chain(bindings.keys().copied())
        .collect();
    let mut callees: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut import_callees: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (name, node) in &nodes {
        let (top_level, imported): (BTreeSet<&str>, BTreeSet<&str>) =
            node_callees(node, &nodes, &names, &values)
                .into_iter()
                .partition(|callee| nodes.contains_key(callee) || values.contains(callee));
        callees.insert(*name, top_level);
        if !imported.is_empty() {
            import_callees.insert(*name, imported);
        }
    }
    // A built value calls what its building function calls (#15928). A name
    // the function also declares as a member (`compare(a, b) { ... }`) reads
    // as a call in text, so it is no edge.
    for (name, value) in &built {
        if !values.contains(name) {
            continue;
        }
        let node = ModuleNode {
            call_texts: vec![CallText {
                calls: std::borrow::Cow::Borrowed(value.text),
                scope: value.text,
            }],
            ..ModuleNode::default()
        };
        let top_level: BTreeSet<&str> = node_callees(&node, &nodes, &names, &values)
            .into_iter()
            .filter(|callee| *callee != *name)
            .filter(|callee| nodes.contains_key(callee) || values.contains(callee))
            .filter(|callee| !declares_member_named(value.text, callee))
            .collect();
        callees.insert(*name, top_level);
    }
    // A method of a module-private class is a node `Class.method` (#15928):
    // it calls what its text bare-calls, the siblings it calls on `this`, and
    // any method it runs on a class it constructs. A function reaches it by
    // `new Class(...).method(`.
    for method in &method_nodes_of_classes {
        let node = ModuleNode {
            call_texts: vec![CallText {
                calls: std::borrow::Cow::Borrowed(method.text),
                scope: method.text,
            }],
            ..ModuleNode::default()
        };
        let mut edges: BTreeSet<&str> = node_callees(&node, &nodes, &names, &values)
            .into_iter()
            .filter(|callee| nodes.contains_key(callee) || values.contains(callee))
            .filter(|callee| !declares_member_named(method.text, callee))
            .collect();
        edges.extend(this_method_calls(method, &method_nodes_of_classes));
        edges.extend(constructed_method_calls(
            method.text,
            method.text,
            &method_nodes_of_classes,
        ));
        callees.insert(method.key.as_str(), edges);
    }
    if !method_nodes_of_classes.is_empty() {
        for (name, node) in &nodes {
            if node.factory.is_some() {
                continue;
            }
            for text in &node.call_texts {
                let constructed =
                    constructed_method_calls(&text.calls, text.scope, &method_nodes_of_classes);
                if !constructed.is_empty() {
                    callees.entry(*name).or_default().extend(constructed);
                }
            }
        }
    }
    let mut entries: BTreeMap<String, Vec<TypeScriptModuleEntry>> = BTreeMap::new();
    // (import local) -> exports of this module that reach a call of it.
    let mut outbound: BTreeMap<&str, BTreeMap<String, TypeScriptModuleEntry>> = BTreeMap::new();
    for (local, exported) in &exports {
        for (reached, hops) in reachable_within_hops(local, &callees) {
            if reached != local.as_str() {
                let owner_entries = entries.entry(reached.to_string()).or_default();
                for entry in exported {
                    let entry = TypeScriptModuleEntry {
                        hops: u8::try_from(hops).unwrap_or(u8::MAX),
                        ..entry.clone()
                    };
                    if !owner_entries.contains(&entry) {
                        owner_entries.push(entry);
                    }
                }
            }
            // The import edge itself is one more hop.
            if hops >= MODULE_ENTRY_MAX_HOPS {
                continue;
            }
            for &binding in import_callees.get(reached).into_iter().flatten() {
                let by_name = outbound.entry(binding).or_default();
                for entry in exported {
                    let entry = TypeScriptModuleEntry {
                        hops: u8::try_from(hops).unwrap_or(u8::MAX),
                        ..entry.clone()
                    };
                    by_name
                        .entry(entry.name.clone())
                        .and_modify(|seen| {
                            if entry.hops < seen.hops {
                                *seen = entry.clone();
                            }
                        })
                        .or_insert(entry);
                }
            }
        }
    }
    for owner_entries in entries.values_mut() {
        owner_entries.sort();
    }
    let outbound = outbound
        .into_iter()
        .filter_map(|(binding, by_name)| {
            Some(OutboundImportCall {
                import: (*bindings.get(binding)?).clone(),
                entries: by_name.into_values().collect(),
            })
        })
        .collect();
    (
        entries,
        outbound,
        values.into_iter().map(str::to_string).collect(),
    )
}

fn collect_statement<'a>(
    statement: &'a Statement<'a>,
    source: &'a str,
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
    exports: &mut BTreeMap<String, BTreeSet<TypeScriptModuleEntry>>,
    values: &mut ModuleValues<'a>,
    exported_values: &mut BTreeSet<&'a str>,
) {
    match statement {
        Statement::FunctionDeclaration(func) => {
            collect_function(func, source, nodes);
        }
        Statement::VariableDeclaration(decl) => {
            collect_variables(decl, source, nodes, values);
        }
        // An exported class is constructed by tests directly; only a
        // module-private one needs the entry graph.
        Statement::ClassDeclaration(class) => {
            values.classes.extend(module_class(class, source));
        }
        Statement::ExportNamedDeclaration(export) => {
            if export.export_kind == ImportOrExportKind::Type {
                return;
            }
            match export.declaration.as_ref() {
                Some(Declaration::FunctionDeclaration(func)) => {
                    if let Some(name) = collect_function(func, source, nodes) {
                        record_export(exports, name, name);
                    }
                }
                Some(Declaration::VariableDeclaration(decl)) => {
                    for name in collect_variables(decl, source, nodes, values) {
                        record_export(exports, name, name);
                    }
                    // An exported value may be imported and mutated by another
                    // module, which this file cannot see: never followed.
                    for declarator in &decl.declarations {
                        if let Some(name) = binding_identifier_name(&declarator.id) {
                            exported_values.insert(name);
                        }
                    }
                }
                Some(_) => {}
                // `export { local as exported }` without a source names local
                // bindings; a re-export from another module (`from "./x"`)
                // binds nothing of this module.
                None if export.source.is_none() => {
                    for specifier in &export.specifiers {
                        if specifier.export_kind == ImportOrExportKind::Type {
                            continue;
                        }
                        let (Some(local), Some(exported)) = (
                            module_export_name_text(&specifier.local),
                            module_export_name_text(&specifier.exported),
                        ) else {
                            continue;
                        };
                        record_export(exports, &local, &exported);
                    }
                }
                None => {}
            }
        }
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                // An anonymous default function is a node of its own; no
                // declaration can be named `default`, so the key is free.
                let name = func.id.as_ref().map_or("default", |id| id.name.as_str());
                if collect_function_as(func, name, source, nodes) {
                    record_export(exports, name, "default");
                }
            }
            ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                collect_arrow_as(arrow, "default", source, nodes);
                record_export(exports, "default", "default");
            }
            ExportDefaultDeclarationKind::Identifier(ident) => {
                record_export(exports, ident.name.as_str(), "default");
            }
            _ => {}
        },
        _ => {}
    }
}

fn record_export(
    exports: &mut BTreeMap<String, BTreeSet<TypeScriptModuleEntry>>,
    local: &str,
    exported: &str,
) {
    exports
        .entry(local.to_string())
        .or_default()
        .insert(TypeScriptModuleEntry {
            name: exported.to_string(),
            exported_as_default: exported == "default",
            class_member: None,
            decorated: None,
            instance_factory: false,
            hops: 0,
            cross_module: None,
        });
}

fn collect_function<'a>(
    func: &'a Function<'a>,
    source: &'a str,
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
) -> Option<&'a str> {
    let name = func.id.as_ref()?.name.as_str();
    collect_function_as(func, name, source, nodes).then_some(name)
}

/// Record `func` as node `name`; `false` for an overload signature, which
/// carries no body.
fn collect_function_as<'a>(
    func: &'a Function<'a>,
    name: &'a str,
    source: &'a str,
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
) -> bool {
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    let returned = returned_functions(&body.statements);
    add_function_node(name, func.span, &returned, source, nodes);
    true
}

fn collect_arrow_as<'a>(
    arrow: &'a ArrowFunctionExpression<'a>,
    name: &'a str,
    source: &'a str,
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
) {
    let returned = if arrow.expression {
        // `(m) => (...args) => ...`: the expression body is the returned value.
        arrow_expression_body(arrow)
            .and_then(function_span)
            .into_iter()
            .collect()
    } else {
        returned_functions(&arrow.body.statements)
    };
    add_function_node(name, arrow.span, &returned, source, nodes);
}

/// A function node: its own calls exclude the functions it returns, which
/// become the calls of any value it builds.
fn add_function_node<'a>(
    name: &'a str,
    span: oxc_span::Span,
    returned: &[oxc_span::Span],
    source: &'a str,
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
) {
    let scope = span_text(source, span);
    // Blank the returned functions by building the owned text once from
    // sorted span segments — clipping overlaps so the blanked region is
    // exactly the union of the segments, matching copy-then-`replace_range`
    // for any overlap shape — instead of copying the whole scope and
    // allocating a throwaway blank string per returned function (#10087).
    // Functions with no directly returned functions (the common case) borrow
    // the scope instead of copying it at all (#10096 review).
    let mut blanks: Vec<(usize, usize)> = returned
        .iter()
        .map(|inner| {
            (
                inner.start.saturating_sub(span.start) as usize,
                inner.end.saturating_sub(span.start) as usize,
            )
        })
        .filter(|(start, end)| {
            *start <= *end
                && *end <= scope.len()
                && scope.is_char_boundary(*start)
                && scope.is_char_boundary(*end)
        })
        .collect();
    blanks.sort_unstable();
    let calls = if blanks.is_empty() {
        std::borrow::Cow::Borrowed(scope)
    } else {
        let mut own = String::with_capacity(scope.len());
        let mut cursor = 0usize;
        for (start, end) in blanks {
            // Clip to the cursor: a segment overlapping the previous one only
            // contributes its tail, so the blanked region is the union — the
            // same result the old per-range `replace_range` produced.
            let start = start.max(cursor);
            if start >= end {
                continue;
            }
            own.push_str(&scope[cursor..start]);
            own.extend(std::iter::repeat_n(' ', end - start));
            cursor = end;
        }
        own.push_str(&scope[cursor..]);
        std::borrow::Cow::Owned(own)
    };
    let node = nodes.entry(name).or_default();
    node.call_texts.push(CallText { calls, scope });
    node.returned_texts
        .extend(returned.iter().map(|inner| CallText {
            calls: std::borrow::Cow::Borrowed(span_text(source, *inner)),
            scope,
        }));
}

/// Top-level `const`/`let`/`var` declarators this module can follow; returns
/// the bound names that became graph nodes.
fn collect_variables<'a>(
    decl: &'a VariableDeclaration<'a>,
    source: &'a str,
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
    values: &mut ModuleValues<'a>,
) -> Vec<&'a str> {
    let mut names = Vec::new();
    let is_const = decl.kind == oxc_ast::ast::VariableDeclarationKind::Const;
    for declarator in &decl.declarations {
        let Some(name) = binding_identifier_name(&declarator.id) else {
            continue;
        };
        let Some(init) = declarator.init.as_ref() else {
            continue;
        };
        // The parsed binding offset: the declaration is recognised from the
        // AST, not from the text before it (`const A = 0, TABLE = {}`).
        let start = match &declarator.id {
            BindingPattern::BindingIdentifier(identifier) => identifier.span.start as usize,
            _ => continue,
        };
        let add_value = |values: &mut ModuleValues<'a>| {
            if is_const {
                values.names.insert(name);
                values.decl_starts.insert(name, start);
            }
        };
        match unwrap_type_wrappers(init) {
            Expression::ArrowFunctionExpression(arrow) => {
                collect_arrow_as(arrow, name, source, nodes);
                names.push(name);
            }
            Expression::FunctionExpression(func) => {
                names.extend(collect_function_as(func, name, source, nodes).then_some(name));
            }
            Expression::CallExpression(call) => {
                let Expression::Identifier(callee) = unwrap_type_wrappers(&call.callee) else {
                    add_value(values);
                    if is_const && let Some(value) = built_value(call, name, source) {
                        values.built.insert(name, value);
                    }
                    continue;
                };
                // `Symbol("x")` builds a value, not a factory product.
                if callee.name == "Symbol" {
                    add_value(values);
                    continue;
                }
                nodes.entry(name).or_default().factory = Some(callee.name.as_str());
                names.push(name);
            }
            // A non-function `const` value (regex, string, object, array,
            // symbol, `new X()`): never an export entry itself, but a node a
            // reading function can reach (#13592 B2).
            _ => add_value(values),
        }
    }
    names
}

/// `(function () { ... })()` or `(() => { ... })()` with no arguments.
fn built_value<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
    name: &str,
    source: &'a str,
) -> Option<BuiltValue<'a>> {
    if !call.arguments.is_empty() || call.optional {
        return None;
    }
    let (span, statements) = match unwrap_type_wrappers(&call.callee) {
        Expression::FunctionExpression(func) if !func.generator && !func.r#async => {
            (func.span, &func.body.as_ref()?.statements)
        }
        Expression::ArrowFunctionExpression(arrow) if !arrow.expression && !arrow.r#async => {
            (arrow.span, &arrow.body.statements)
        }
        _ => return None,
    };
    let declares_name = statements.iter().any(|statement| match statement {
        Statement::ClassDeclaration(class) => {
            class.id.as_ref().is_some_and(|id| id.name.as_str() == name)
        }
        Statement::FunctionDeclaration(func) => {
            func.id.as_ref().is_some_and(|id| id.name.as_str() == name)
        }
        Statement::VariableDeclaration(decl) => decl
            .declarations
            .iter()
            .any(|declarator| binding_identifier_name(&declarator.id) == Some(name)),
        _ => false,
    });
    Some(BuiltValue {
        text: span_text(source, span),
        shadow: declares_name.then_some(span.start as usize..span.end as usize),
    })
}

/// `name(...) {` or `name(...): T {` in `text`: a method or shorthand member
/// declaration of that name. Fail-closed for the caller: a call followed by
/// `:` in a conditional expression also answers `true`.
fn declares_member_named(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(idx, _)| {
        let before = &text[..idx];
        // `name (a) {` and `name<T>(a: T) {` declare the same member.
        let after = past_type_arguments(&text[idx + name.len()..]);
        if before
            .chars()
            .next_back()
            .is_some_and(is_javascript_identifier_char)
            || before.trim_end().ends_with('.')
            || !after.starts_with('(')
        {
            return false;
        }
        let mut depth = 0usize;
        for (offset, ch) in after.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let tail = after[offset + 1..].trim_start();
                        return tail.starts_with('{') || tail.starts_with(':');
                    }
                }
                _ => {}
            }
        }
        true
    })
}

/// Spans of the function expressions the body returns from its own top-level
/// `return` statements. Nested returns (inside `if` or loops) are not followed.
fn returned_functions(statements: &[Statement<'_>]) -> Vec<oxc_span::Span> {
    statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::ReturnStatement(ret) => ret.argument.as_ref(),
            _ => None,
        })
        .filter_map(function_span)
        .collect()
}

fn function_span(expression: &Expression<'_>) -> Option<oxc_span::Span> {
    match unwrap_type_wrappers(expression) {
        Expression::ArrowFunctionExpression(arrow) => Some(arrow.span),
        Expression::FunctionExpression(func) => Some(func.span),
        _ => None,
    }
}

fn arrow_expression_body<'a>(arrow: &'a ArrowFunctionExpression<'a>) -> Option<&'a Expression<'a>> {
    match arrow.body.statements.first()? {
        Statement::ExpressionStatement(statement) => Some(&statement.expression),
        _ => None,
    }
}

/// Strip `as T`, `satisfies T`, `x!` and parentheses, which do not change the
/// runtime value.
pub(crate) fn unwrap_type_wrappers<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::TSAsExpression(inner) => unwrap_type_wrappers(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap_type_wrappers(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap_type_wrappers(&inner.expression),
        Expression::ParenthesizedExpression(inner) => unwrap_type_wrappers(&inner.expression),
        other => other,
    }
}

fn span_text(source: &str, span: oxc_span::Span) -> &str {
    source
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default()
}

/// Top-level names a node calls. A factory product calls what its factory's
/// returned function calls.
fn node_callees<'n, 'a>(
    node: &ModuleNode<'a>,
    nodes: &BTreeMap<&'a str, ModuleNode<'a>>,
    names: &std::collections::HashSet<&'n str>,
    values: &BTreeSet<&'n str>,
) -> BTreeSet<&'n str> {
    let texts: Vec<&CallText<'_>> = match node.factory {
        Some(factory) => nodes
            .get(factory)
            .map(|factory_node| factory_node.returned_texts.iter().collect())
            .unwrap_or_default(),
        None => node.call_texts.iter().collect(),
    };
    // Tokenize each text once and confirm only top-level names it mentions,
    // so a module with many declarations is not rescanned once per name.
    // Only names that are actually top-level bindings are deduped per text —
    // collecting every token (the old BTreeSet) allocated once per function
    // on large modules (#10087).
    let mut callees = BTreeSet::new();
    for text in texts {
        // Masked at most once per function text, not once per value.
        let mut masked: Option<String> = None;
        let mut checked: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for token in text
            .calls
            .split(|ch: char| !is_javascript_identifier_char(ch))
        {
            if let Some(&value) = values.get(token) {
                // Read-only use of a module-level `const` value (#13592 B2).
                // Comments and literals are masked only for the read-presence
                // check; the shadow check stays on the raw scope, so a
                // mis-masked declaration can only remove an edge.
                if checked.insert(value)
                    && reads_unshadowed_value(
                        masked.get_or_insert_with(|| ts_mask_literals_and_comments(&text.calls)),
                        text.scope,
                        value,
                    )
                {
                    callees.insert(value);
                }
                continue;
            }
            // Membership first: `checked` then only ever holds top-level
            // binding names, not every distinct keyword/identifier (#10096
            // review).
            let Some(&name) = names.get(token) else {
                continue;
            };
            if !checked.insert(name) {
                continue;
            }
            if text_calls_top_level(text, name, nodes.contains_key(name)) {
                callees.insert(name);
            }
        }
    }
    callees
}

/// A bare call to `name` that the enclosing function cannot have rebound, or,
/// for a declaration of this module (`declared_here`), `name` handed by
/// reference to a synchronous array callback method (`keys.sort(name)`,
/// #15928), which calls it before returning. An imported binding passed by
/// reference is no edge: cross-module reach stays bare calls only.
/// Fail-closed: every mention of `name` in the scope must be one of those two
/// forms. Any other mention (a parameter, including destructured and
/// nested-callback parameters, an assignment, a `function name` declaration, a
/// value stored or passed to anything else, such as `setTimeout(name)` or
/// `promise.then(name)`) may bind, alias or defer it, so no edge is recorded.
fn text_calls_top_level(text: &CallText<'_>, name: &str, declared_here: bool) -> bool {
    (contains_call_name(&text.calls, name)
        || (declared_here && runs_function_reference(&text.calls, name)))
        && every_mention_is_a_bare_call(text.scope, name)
}

/// The raw scan is the cheap filter; the reference counts only when it is
/// still there with comments and literals masked, so `// keys.sort(name)`
/// after code on the same line, or inside a string, is no call.
fn runs_function_reference(calls: &str, name: &str) -> bool {
    let referenced = |code: &str| {
        code.match_indices(name)
            .any(|(idx, _)| is_sync_callback_reference(&code[..idx], &code[idx + name.len()..]))
    };
    referenced(calls) && referenced(&ts_mask_literals_and_comments(calls))
}

/// `recv.sort(NAME)` / `recv.map(NAME, ...)`: the whole first argument of a
/// synchronous array callback method.
fn is_sync_callback_reference(before: &str, after: &str) -> bool {
    let head = before.trim_end();
    let rest = after.trim_start();
    head.ends_with('(')
        && (rest.starts_with(')') || rest.starts_with(','))
        && callback_runs_synchronously(head)
}

fn every_mention_is_a_bare_call(scope: &str, name: &str) -> bool {
    scope.match_indices(name).all(|(idx, _)| {
        let before = &scope[..idx];
        let after = &scope[idx + name.len()..];
        let bounded_before = before
            .chars()
            .next_back()
            .is_none_or(|ch| !is_javascript_identifier_char(ch));
        let bounded_after = after
            .chars()
            .next()
            .is_none_or(|ch| !is_javascript_identifier_char(ch));
        if !(bounded_before && bounded_after) {
            // Part of a longer identifier: not a mention of `name`.
            return true;
        }
        let member = before.trim_end().ends_with('.');
        let declared = before
            .trim_end()
            .strip_suffix("function")
            .is_some_and(|rest| {
                rest.chars()
                    .next_back()
                    .is_none_or(|ch| !is_javascript_identifier_char(ch))
            });
        let rest = after.trim_start();
        let called = rest.starts_with('(') || rest.starts_with("?.(");
        // `x.name(...)` is a member call, which is another binding entirely.
        member || (called && !declared) || is_sync_callback_reference(before, after)
    })
}

/// Names reachable from `start` within the hop bound, each with its shortest
/// call-edge distance (`start` itself at `0`).
fn reachable_within_hops<'n>(
    start: &'n str,
    callees: &BTreeMap<&'n str, BTreeSet<&'n str>>,
) -> BTreeMap<&'n str, usize> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::from([(start, 0)]);
    let mut queue: VecDeque<(&str, usize)> = VecDeque::from([(start, 0)]);
    while let Some((name, hops)) = queue.pop_front() {
        if hops == MODULE_ENTRY_MAX_HOPS {
            continue;
        }
        for &callee in callees.get(name).into_iter().flatten() {
            if !seen.contains_key(callee) {
                seen.insert(callee, hops + 1);
                queue.push_back((callee, hops + 1));
            }
        }
    }
    seen
}
