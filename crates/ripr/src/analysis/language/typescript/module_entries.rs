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
//! top-level declaration name that the calling body does not rebind, and a
//! factory product reaches only what the factory's directly returned function
//! calls. Reach through an entry is weaker than a direct owner call: it is
//! admitted only when no test calls the owner itself, and the classifier never
//! promotes it to `exposed`.

use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Maximum call edges followed from an exported entry to the owner. Matches
/// the Rust helper-owner graph bound (`HELPER_OWNER_CALL_GRAPH_MAX_HOPS`).
const MODULE_ENTRY_MAX_HOPS: usize = 3;

/// An exported name of the owner's module whose code reaches the owner.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TypeScriptModuleEntry {
    /// Exported name a test imports (`default` for the default export).
    pub(crate) name: String,
    /// `true` when this entry is the module's default export, so a default
    /// import under any local name binds it.
    pub(crate) exported_as_default: bool,
}

/// One top-level declaration of the module.
#[derive(Default)]
struct ModuleNode<'s> {
    /// Function texts whose bare calls count as this node's calls.
    call_texts: Vec<CallText<'s>>,
    /// Factory whose returned function this node's value is (`const p = f()`).
    factory: Option<String>,
    /// Function texts the declaration returns directly, when it is a factory.
    returned_texts: Vec<CallText<'s>>,
}

/// A function's source text plus the parameter names it binds.
struct CallText<'s> {
    text: &'s str,
    params: Vec<String>,
}

/// Exported entries per top-level owner name.
pub(crate) fn module_entries_by_owner(
    statements: &[Statement<'_>],
    source: &str,
) -> BTreeMap<String, Vec<TypeScriptModuleEntry>> {
    let mut nodes: BTreeMap<String, ModuleNode<'_>> = BTreeMap::new();
    let mut exports: BTreeMap<String, BTreeSet<TypeScriptModuleEntry>> = BTreeMap::new();
    for statement in statements {
        collect_statement(statement, source, &mut nodes, &mut exports);
    }
    let names: BTreeSet<&str> = nodes.keys().map(String::as_str).collect();
    let callees: BTreeMap<&str, BTreeSet<&str>> = nodes
        .iter()
        .map(|(name, node)| (name.as_str(), node_callees(node, &nodes, &names)))
        .collect();
    let mut entries: BTreeMap<String, Vec<TypeScriptModuleEntry>> = BTreeMap::new();
    for (local, exported) in &exports {
        for reached in reachable_within_hops(local, &callees) {
            if reached == local.as_str() {
                continue;
            }
            let owner_entries = entries.entry(reached.to_string()).or_default();
            for entry in exported {
                if !owner_entries.contains(entry) {
                    owner_entries.push(entry.clone());
                }
            }
        }
    }
    for owner_entries in entries.values_mut() {
        owner_entries.sort();
    }
    entries
}

fn collect_statement<'s>(
    statement: &Statement<'_>,
    source: &'s str,
    nodes: &mut BTreeMap<String, ModuleNode<'s>>,
    exports: &mut BTreeMap<String, BTreeSet<TypeScriptModuleEntry>>,
) {
    match statement {
        Statement::FunctionDeclaration(func) => {
            collect_function(func, source, nodes);
        }
        Statement::VariableDeclaration(decl) => {
            collect_variables(decl, source, nodes);
        }
        Statement::ExportNamedDeclaration(export) => {
            if export.export_kind == ImportOrExportKind::Type {
                return;
            }
            match export.declaration.as_ref() {
                Some(Declaration::FunctionDeclaration(func)) => {
                    if let Some(name) = collect_function(func, source, nodes) {
                        record_export(exports, &name, &name);
                    }
                }
                Some(Declaration::VariableDeclaration(decl)) => {
                    for name in collect_variables(decl, source, nodes) {
                        record_export(exports, &name, &name);
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
                if let Some(name) = collect_function(func, source, nodes) {
                    record_export(exports, &name, "default");
                }
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
        });
}

fn collect_function<'s>(
    func: &Function<'_>,
    source: &'s str,
    nodes: &mut BTreeMap<String, ModuleNode<'s>>,
) -> Option<String> {
    let name = func.id.as_ref()?.name.to_string();
    // Overload signatures carry no body; only the implementation counts.
    let body = func.body.as_ref()?;
    let node = nodes.entry(name.clone()).or_default();
    node.call_texts.push(CallText {
        text: span_text(source, func.span),
        params: parameter_names(&func.params),
    });
    node.returned_texts
        .extend(returned_function_texts(&body.statements, source));
    Some(name)
}

/// Top-level `const`/`let`/`var` declarators this module can follow; returns
/// the bound names that became graph nodes.
fn collect_variables<'s>(
    decl: &VariableDeclaration<'_>,
    source: &'s str,
    nodes: &mut BTreeMap<String, ModuleNode<'s>>,
) -> Vec<String> {
    let mut names = Vec::new();
    for declarator in &decl.declarations {
        let Some(name) = binding_identifier_name(&declarator.id) else {
            continue;
        };
        let Some(init) = declarator.init.as_ref() else {
            continue;
        };
        match unwrap_type_wrappers(init) {
            Expression::ArrowFunctionExpression(arrow) => {
                let node = nodes.entry(name.to_string()).or_default();
                node.call_texts.push(CallText {
                    text: span_text(source, arrow.span),
                    params: parameter_names(&arrow.params),
                });
                if arrow.expression {
                    // `(m) => (...args) => ...`: the expression body is the
                    // returned value.
                    node.returned_texts.extend(
                        arrow_expression_body(arrow).and_then(|body| function_text(body, source)),
                    );
                } else {
                    node.returned_texts
                        .extend(returned_function_texts(&arrow.body.statements, source));
                }
                names.push(name.to_string());
            }
            Expression::FunctionExpression(func) => {
                let node = nodes.entry(name.to_string()).or_default();
                node.call_texts.push(CallText {
                    text: span_text(source, func.span),
                    params: parameter_names(&func.params),
                });
                if let Some(body) = func.body.as_ref() {
                    node.returned_texts
                        .extend(returned_function_texts(&body.statements, source));
                }
                names.push(name.to_string());
            }
            Expression::CallExpression(call) => {
                let Expression::Identifier(callee) = unwrap_type_wrappers(&call.callee) else {
                    continue;
                };
                nodes.entry(name.to_string()).or_default().factory = Some(callee.name.to_string());
                names.push(name.to_string());
            }
            _ => {}
        }
    }
    names
}

/// Function expressions the body returns from its own top-level `return`
/// statements. Nested returns (inside `if` or loops) are not followed.
fn returned_function_texts<'s>(statements: &[Statement<'_>], source: &'s str) -> Vec<CallText<'s>> {
    statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::ReturnStatement(ret) => ret.argument.as_ref(),
            _ => None,
        })
        .filter_map(|argument| function_text(argument, source))
        .collect()
}

fn function_text<'s>(expression: &Expression<'_>, source: &'s str) -> Option<CallText<'s>> {
    match unwrap_type_wrappers(expression) {
        Expression::ArrowFunctionExpression(arrow) => Some(CallText {
            text: span_text(source, arrow.span),
            params: parameter_names(&arrow.params),
        }),
        Expression::FunctionExpression(func) => Some(CallText {
            text: span_text(source, func.span),
            params: parameter_names(&func.params),
        }),
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
fn unwrap_type_wrappers<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
    match expression {
        Expression::TSAsExpression(inner) => unwrap_type_wrappers(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap_type_wrappers(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap_type_wrappers(&inner.expression),
        Expression::ParenthesizedExpression(inner) => unwrap_type_wrappers(&inner.expression),
        other => other,
    }
}

fn parameter_names(params: &FormalParameters<'_>) -> Vec<String> {
    let mut names: Vec<String> = params
        .items
        .iter()
        .filter_map(|item| binding_identifier_name(&item.pattern))
        .map(str::to_string)
        .collect();
    if let Some(rest) = params.rest.as_ref()
        && let Some(name) = binding_identifier_name(&rest.rest.argument)
    {
        names.push(name.to_string());
    }
    names
}

fn span_text(source: &str, span: oxc_span::Span) -> &str {
    source
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default()
}

/// Top-level names a node calls. A factory product calls what its factory's
/// returned function calls.
fn node_callees<'n>(
    node: &ModuleNode<'_>,
    nodes: &BTreeMap<String, ModuleNode<'_>>,
    names: &BTreeSet<&'n str>,
) -> BTreeSet<&'n str> {
    let texts: Vec<&CallText<'_>> = match node.factory.as_deref() {
        Some(factory) => nodes
            .get(factory)
            .map(|factory_node| factory_node.returned_texts.iter().collect())
            .unwrap_or_default(),
        None => node.call_texts.iter().collect(),
    };
    names
        .iter()
        .copied()
        .filter(|name| texts.iter().any(|text| text_calls_top_level(text, name)))
        .collect()
}

/// A bare call to `name` the function text does not rebind: a parameter or a
/// body-local declaration of the same name shadows the top-level binding.
fn text_calls_top_level(text: &CallText<'_>, name: &str) -> bool {
    !text.params.iter().any(|param| param == name)
        && contains_call_name(text.text, name)
        && !local_identifier_declared_in_test_body(function_body_text(text.text), name)
}

/// The text after the function's parameter list, so the scan for body-local
/// declarations does not read the parameter list or the function's own name.
fn function_body_text(text: &str) -> &str {
    text.find('{').map_or(text, |open| &text[open..])
}

fn reachable_within_hops<'n>(
    start: &'n str,
    callees: &BTreeMap<&'n str, BTreeSet<&'n str>>,
) -> BTreeSet<&'n str> {
    let mut seen: BTreeSet<&str> = BTreeSet::from([start]);
    let mut queue: VecDeque<(&str, usize)> = VecDeque::from([(start, 0)]);
    while let Some((name, hops)) = queue.pop_front() {
        if hops == MODULE_ENTRY_MAX_HOPS {
            continue;
        }
        for &callee in callees.get(name).into_iter().flatten() {
            if seen.insert(callee) {
                queue.push_back((callee, hops + 1));
            }
        }
    }
    seen
}
