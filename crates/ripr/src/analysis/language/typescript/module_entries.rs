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

use super::module_value_reads::{
    byte_preserving_mask, never_mutated_in_module, reads_unshadowed_value,
};
use super::*;
use oxc_ast::ast::{
    AssignmentTargetPropertyIdentifier, BindingIdentifier, IdentifierReference,
    SimpleAssignmentTarget, TSType,
};
use oxc_ast_visit::{Visit, walk};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Maximum call edges followed from an exported entry to the owner. Matches
/// the Rust helper-owner graph bound (`HELPER_OWNER_CALL_GRAPH_MAX_HOPS`).
pub(crate) const MODULE_ENTRY_MAX_HOPS: usize = 3;

/// Maximum edges followed from an exported entry to the owner when the path
/// passes at least one closure edge (#13592). Combinator-built code is deep:
/// unjs/pathe `matchGlob` reaches `int2alpha` only through `compileGlob`,
/// the `Grammar` constant, `Range`, `RangeValue`, `RangeAlphaUpper` and
/// `makeRangeAlpha`. Reach along such a path is never more than unknown.
pub(crate) const MODULE_CLOSURE_MAX_HOPS: usize = 8;

/// Closure entries kept per owner; an owner past it keeps none (#13592).
pub(crate) const MAX_CLOSURE_ENTRIES_PER_OWNER: usize = 32;

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
    /// `true` when every path from the entry to the owner passes a closure
    /// edge (#13592): the owner is passed as a value (`JSON.parse(text,
    /// reviver)`, `keys.sort(compare)`), referenced by a value that is
    /// invoked later (`parse(input, Grammar)`), or is the factory or a
    /// callback argument of a call-built constant (`const Rule =
    /// match(re, () => regex(...))`). Whether and when the value runs is not
    /// modeled, so such reach is never more than unknown.
    #[serde(default)]
    pub(crate) closure: bool,
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
    /// Names the declaration's code references (#13592): every identifier
    /// read anywhere in the function, its returned closures and nested
    /// callbacks, or the whole initializer of a call-built constant (its
    /// callee included), minus every name the code binds itself. A reference
    /// to another top-level declaration is a closure edge: the value may run
    /// whenever this code runs, but when (or whether) is not modeled. A
    /// function's directly returned functions are left out, as for its calls.
    references: Vec<&'s str>,
    /// References of the functions this declaration returns directly: closure
    /// edges of a value it builds (`const p = f()`), never its own.
    returned_references: Vec<&'s str>,
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
    let mut values: BTreeSet<&'a str> = BTreeSet::new();
    let mut exported_values: BTreeSet<&'a str> = BTreeSet::new();
    let mut decl_starts: BTreeMap<&'a str, usize> = BTreeMap::new();
    for statement in statements {
        collect_statement(
            statement,
            source,
            &mut nodes,
            &mut exports,
            &mut values,
            &mut exported_values,
            &mut decl_starts,
        );
    }
    // A value is followable only while nothing in the file writes it; a name
    // that is also a function/factory node keeps its bare-call edges only.
    // Only values some node's code actually mentions are scanned (the mutation
    // scan is whole-module), and an exported value is never followed: another
    // module may import and mutate it.
    let mentioned: std::collections::HashSet<&str> = nodes
        .values()
        .flat_map(|node| node.call_texts.iter().chain(node.returned_texts.iter()))
        .flat_map(|text| {
            text.calls
                .split(|ch: char| !is_javascript_identifier_char(ch))
                .filter(|token| !token.is_empty())
        })
        .collect();
    values.retain(|name| {
        !nodes.contains_key(name) && !exported_values.contains(name) && mentioned.contains(name)
    });
    if !values.is_empty() {
        let masked_source = byte_preserving_mask(source, &ts_mask_literals_and_comments(source));
        values.retain(|name| {
            never_mutated_in_module(source, &masked_source, name, decl_starts.get(name).copied())
        });
    }
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
    // Closure edges (#13592): a reference to another top-level declaration
    // that the bare-call graph does not already record. A name the module
    // ever reassigns may hold some other value, so it is never a target.
    let reassigned = reassigned_names(statements);
    let closure_callees: BTreeMap<&str, BTreeSet<&str>> = nodes
        .iter()
        .map(|(name, node)| {
            let calls = callees.get(name);
            // A built value also reaches what its factory's returned
            // functions reference.
            // A factory the module reassigns may no longer be the declared
            // one, so its returned functions' references are not inherited.
            let built = node
                .factory
                .filter(|factory| !reassigned.contains(factory))
                .and_then(|factory| nodes.get(factory))
                .map(|factory| factory.returned_references.as_slice())
                .unwrap_or_default();
            let targets: BTreeSet<&str> = node
                .references
                .iter()
                .chain(built)
                .copied()
                .filter(|target| {
                    target != name
                        && nodes.contains_key(target)
                        && !reassigned.contains(target)
                        && !calls.is_some_and(|calls| calls.contains(target))
                })
                .collect();
            (*name, targets)
        })
        .collect();
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
    // Owners reached from an export only along a path with a closure edge:
    // an entry of their own, marked so it reads unknown and never chains
    // like a call (#13592).
    for (local, exported) in &exports {
        for (reached, hops) in reachable_through_closures(local, &callees, &closure_callees) {
            if reached == local.as_str() {
                continue;
            }
            // `reachable_through_closures` leaves out every owner this
            // export already reaches by calls, and each export is visited
            // once, so no entry here duplicates another.
            let owner_entries = entries.entry(reached.to_string()).or_default();
            owner_entries.extend(exported.iter().map(|entry| TypeScriptModuleEntry {
                hops: u8::try_from(hops).unwrap_or(u8::MAX),
                closure: true,
                ..entry.clone()
            }));
        }
    }
    for owner_entries in entries.values_mut() {
        // A densely connected module (a combinator grammar with many
        // exports) could give an owner a closure entry per export, and every
        // related test rechecks them all. Past the cap the owner keeps only
        // its call entries: the reading before closure reach, which may be
        // `no_static_path` though the analysis saw the reach.
        if owner_entries.iter().filter(|entry| entry.closure).count()
            > MAX_CLOSURE_ENTRIES_PER_OWNER
        {
            owner_entries.retain(|entry| !entry.closure);
        }
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
    values: &mut BTreeSet<&'a str>,
    exported_values: &mut BTreeSet<&'a str>,
    decl_starts: &mut BTreeMap<&'a str, usize>,
) {
    match statement {
        Statement::FunctionDeclaration(func) => {
            collect_function(func, source, nodes);
        }
        Statement::VariableDeclaration(decl) => {
            collect_variables(decl, source, nodes, values, decl_starts);
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
                    for name in collect_variables(decl, source, nodes, values, decl_starts) {
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
            closure: false,
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
    add_function_references(name, &func.params, body, &returned, nodes);
    // A named function expression binds its own name inside its body
    // (`const label = function helper() { return helper.name; }`): a
    // reference to that name is the function itself, never a top-level
    // declaration of the same name.
    if let Some(own) = func.id.as_ref().map(|id| id.name.as_str())
        && own != name
        && let Some(node) = nodes.get_mut(name)
    {
        node.references.retain(|target| *target != own);
        node.returned_references.retain(|target| *target != own);
    }
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
    add_function_references(name, &arrow.params, &arrow.body, &returned, nodes);
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
    values: &mut BTreeSet<&'a str>,
    decl_starts: &mut BTreeMap<&'a str, usize>,
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
        let mut add_value = |values: &mut BTreeSet<&'a str>| {
            if is_const {
                values.insert(name);
                decl_starts.insert(name, start);
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
                    continue;
                };
                // `Symbol("x")` builds a value, not a factory product.
                if callee.name == "Symbol" {
                    add_value(values);
                    continue;
                }
                nodes.entry(name).or_default().factory = Some(callee.name.as_str());
                // The callee and every argument (callbacks included) are
                // closure references of the built value (#13592).
                let mut references = References::default();
                references.visit_expression(init);
                let References {
                    referenced, bound, ..
                } = references;
                let node = nodes.entry(name).or_default();
                push_unbound(&mut node.references, referenced, &bound);
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

/// Identifier references and bindings of one declaration's code (#13592).
/// Type positions are skipped: a `typeof f` annotation runs nothing.
#[derive(Default)]
struct References<'a> {
    referenced: Vec<&'a str>,
    bound: std::collections::HashSet<&'a str>,
}

impl<'a> Visit<'a> for References<'a> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.referenced.push(it.name.as_str());
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.bound.insert(it.name.as_str());
    }

    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}
}

/// Record a function node's references: its own code minus the functions it
/// returns directly, and, apart, those returned functions' references, which
/// a value the function builds reaches (like `returned_texts`). Fail-closed:
/// a name the function binds anywhere, returned closures included (a
/// parameter, a local, a nested callback's parameter, a catch binding), may
/// shadow the top-level declaration at any of its references, so none of
/// its references counts.
fn add_function_references<'a>(
    name: &'a str,
    params: &'a FormalParameters<'a>,
    body: &'a oxc_ast::ast::FunctionBody<'a>,
    returned: &[oxc_span::Span],
    nodes: &mut BTreeMap<&'a str, ModuleNode<'a>>,
) {
    let mut all = References::default();
    all.visit_formal_parameters(params);
    all.visit_function_body(body);
    if returned.is_empty() {
        let node = nodes.entry(name).or_default();
        push_unbound(&mut node.references, all.referenced, &all.bound);
        return;
    }
    // The same statements, minus each top-level `return` of a function the
    // node returns directly (an arrow's expression body is such a return).
    let mut own = References::default();
    own.visit_formal_parameters(params);
    for statement in &body.statements {
        let returns_function = match statement {
            Statement::ReturnStatement(ret) => ret.argument.as_ref(),
            Statement::ExpressionStatement(expression) => Some(&expression.expression),
            _ => None,
        }
        .and_then(function_span)
        .is_some_and(|span| returned.contains(&span));
        if !returns_function {
            own.visit_statement(statement);
        }
    }
    let own_names: std::collections::HashSet<&str> = own.referenced.iter().copied().collect();
    let returned_references: Vec<&'a str> = all
        .referenced
        .iter()
        .copied()
        .filter(|target| !own_names.contains(target))
        .collect();
    let node = nodes.entry(name).or_default();
    push_unbound(&mut node.references, own.referenced, &all.bound);
    push_unbound(
        &mut node.returned_references,
        returned_references,
        &all.bound,
    );
}

fn push_unbound<'a>(
    into: &mut Vec<&'a str>,
    referenced: Vec<&'a str>,
    bound: &std::collections::HashSet<&'a str>,
) {
    let mut seen: std::collections::HashSet<&'a str> = into.iter().copied().collect();
    for target in referenced {
        if !bound.contains(target) && seen.insert(target) {
            into.push(target);
        }
    }
}

/// Names the module assigns or updates anywhere (`x = ...`, `x++`, a
/// destructuring or `for (x of ...)` target), so a closure edge never
/// follows a binding that may hold some other value.
fn reassigned_names<'a>(statements: &'a [Statement<'a>]) -> std::collections::HashSet<&'a str> {
    #[derive(Default)]
    struct Reassigned<'a> {
        names: std::collections::HashSet<&'a str>,
    }
    // Only a bare identifier target rebinds a name: `obj.prop = 1` and
    // `obj[key]++` mutate `obj` and read `key`, they rebind neither.
    impl<'a> Visit<'a> for Reassigned<'a> {
        fn visit_simple_assignment_target(&mut self, it: &SimpleAssignmentTarget<'a>) {
            if let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = it {
                self.names.insert(id.name.as_str());
            }
            walk::walk_simple_assignment_target(self, it);
        }

        fn visit_assignment_target_property_identifier(
            &mut self,
            it: &AssignmentTargetPropertyIdentifier<'a>,
        ) {
            // `({ name } = value)` rebinds `name`.
            self.names.insert(it.binding.name.as_str());
            walk::walk_assignment_target_property_identifier(self, it);
        }
    }
    let mut visitor = Reassigned::default();
    for statement in statements {
        visitor.visit_statement(statement);
    }
    visitor.names
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
            if text_calls_top_level(text, name) {
                callees.insert(name);
            }
        }
    }
    callees
}

/// A bare call to `name` that the enclosing function cannot have rebound.
/// Fail-closed: every mention of `name` in the scope must be a bare call. Any
/// other mention (a parameter, including destructured and nested-callback
/// parameters, an assignment, a `function name` declaration, a value passed
/// along) may bind or alias it, so no edge is recorded.
fn text_calls_top_level(text: &CallText<'_>, name: &str) -> bool {
    contains_call_name(&text.calls, name) && every_mention_is_a_bare_call(text.scope, name)
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
        member || (called && !declared)
    })
}

/// Names reachable from `start` only along paths that take at least one
/// closure edge, within [`MODULE_CLOSURE_MAX_HOPS`], each with its shortest
/// such distance. A path may begin with bare calls (at most
/// [`MODULE_ENTRY_MAX_HOPS`] of them before its first closure edge) and mix
/// both kinds after it. Names the bare-call graph alone reaches within its
/// own bound are left out: their call entry already stands.
fn reachable_through_closures<'n>(
    start: &'n str,
    callees: &BTreeMap<&'n str, BTreeSet<&'n str>>,
    closure_callees: &BTreeMap<&'n str, BTreeSet<&'n str>>,
) -> BTreeMap<&'n str, usize> {
    let by_call = reachable_within_hops(start, callees);
    // States are (name, took a closure edge yet).
    let mut seen: std::collections::HashSet<(&str, bool)> =
        std::collections::HashSet::from([(start, false)]);
    let mut queue: VecDeque<(&str, bool, usize)> = VecDeque::from([(start, false, 0)]);
    let mut reached: BTreeMap<&str, usize> = BTreeMap::new();
    while let Some((name, closure, hops)) = queue.pop_front() {
        if closure && !by_call.contains_key(name) {
            reached.entry(name).or_insert(hops);
        }
        if hops == MODULE_CLOSURE_MAX_HOPS {
            continue;
        }
        let call_allowed = closure || hops < MODULE_ENTRY_MAX_HOPS;
        let next_calls = callees
            .get(name)
            .into_iter()
            .flatten()
            .filter(|_| call_allowed)
            .map(|callee| (*callee, closure));
        let next_closures = closure_callees
            .get(name)
            .into_iter()
            .flatten()
            .map(|callee| (*callee, true));
        for state in next_calls.chain(next_closures).collect::<Vec<_>>() {
            if seen.insert(state) {
                queue.push_back((state.0, state.1, hops + 1));
            }
        }
    }
    reached
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
