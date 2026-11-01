use super::PythonFramework;
use super::admission::{PythonAdmissionContext, PythonTestFunction, assertion_admission};
use super::conditional_defs::live_conditional_functions;
use super::module_constants::{
    PythonModuleConstant, PythonModuleNames, constants_visible_in_function, python_test_rebinding,
};
use super::parametrize::{ParametrizeScope, parametrize_cases};
#[cfg(test)]
use super::source_facts::extract_source_facts;
use super::source_utils::{
    SourceText, line_for_range_end, line_for_range_start, normalized_path, text_for_range,
};
use super::static_limits::{collect_static_cli_receiver_names, is_static_route_decorator};
use super::{
    PythonImport, PythonOwner, PythonParameter, PythonTest, collect_assertions_from_statements,
    expr_full_name, first_parenthesized_string_argument,
};
use crate::domain::OwnerKind;
use rustpython_parser::{
    ast::{self, Constant, Expr, Ranged, Stmt},
    text_size::TextRange,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[cfg(test)]
pub(super) fn extract_owners(file: &Path, source: &str) -> Vec<PythonOwner> {
    extract_source_facts(file, source).owners
}

#[allow(
    clippy::too_many_arguments,
    reason = "owner collection mirrors the per-file extraction inputs; the function-context input names the enclosing def for nested owners"
)]
pub(super) fn collect_owners_from_statements(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
    class_context: Option<&str>,
    function_context: Option<&str>,
    imports: &[PythonImport],
    module_constants: &[PythonModuleConstant],
    out: &mut Vec<PythonOwner>,
) {
    let owner_context = PythonOwnerContext {
        file,
        source,
        class_context,
        imports,
        module_constants,
    };
    // A definition under a statically decided module-level `if` runs on every
    // interpreter, so it is an owner like a top-level one (#9423).
    if class_context.is_none() && function_context.is_none() {
        for stmt in live_conditional_functions(source, statements) {
            let first = out.len();
            push_function_owner(stmt, owner_context, out);
            for owner in &mut out[first..] {
                owner.version_conditional = true;
            }
        }
    }
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => {
                push_function_owner(stmt, owner_context, out);
                if let Some(parent) = function_context {
                    let called = body_calls_nested_def(source, statements, &function.name);
                    if let Some(owner) = out.last_mut() {
                        owner.qualified_name = format!("{parent}.{}", function.name);
                        owner.owner_kind = Some(OwnerKind::Function);
                        owner.nested_called_by_parent = called;
                    }
                }
                let parent_qualified = out
                    .last()
                    .map(|owner| owner.qualified_name.clone())
                    .unwrap_or_default();
                collect_owners_from_statements(
                    file,
                    source,
                    &function.body,
                    class_context,
                    Some(parent_qualified.as_str()),
                    imports,
                    module_constants,
                    out,
                );
            }
            Stmt::AsyncFunctionDef(function) => {
                push_function_owner(stmt, owner_context, out);
                if let Some(parent) = function_context {
                    let called = body_calls_nested_def(source, statements, &function.name);
                    if let Some(owner) = out.last_mut() {
                        owner.qualified_name = format!("{parent}.{}", function.name);
                        owner.owner_kind = Some(OwnerKind::Function);
                        owner.nested_called_by_parent = called;
                    }
                }
                let parent_qualified = out
                    .last()
                    .map(|owner| owner.qualified_name.clone())
                    .unwrap_or_default();
                collect_owners_from_statements(
                    file,
                    source,
                    &function.body,
                    class_context,
                    Some(parent_qualified.as_str()),
                    imports,
                    module_constants,
                    out,
                );
            }
            Stmt::ClassDef(class) => {
                let class_path = class_context.map_or_else(
                    || class.name.to_string(),
                    |outer| format!("{outer}.{}", class.name),
                );
                let first = out.len();
                collect_owners_from_statements(
                    file,
                    source,
                    &class.body,
                    Some(class_path.as_str()),
                    None,
                    imports,
                    module_constants,
                    out,
                );
                // Class-level facts a dunder binding relies on (#9421): a class
                // with decorators, a metaclass, odd body statements or a second
                // definition of the same path may run another method than the
                // one the owner names.
                let duplicate = out[..first]
                    .iter()
                    .any(|owner| owner.class_path == class_path && owner.file == file);
                let plain = !duplicate
                    && class.decorator_list.is_empty()
                    && class.keywords.is_empty()
                    && class_body_is_plain(&class.body);
                let no_bases = class.bases.is_empty();
                for owner in out
                    .iter_mut()
                    .filter(|owner| owner.class_path == class_path && owner.file == file)
                {
                    owner.class_facts.plain = plain;
                    owner.class_facts.no_bases = no_bases;
                }
                out.push(owner_from_class(
                    PythonOwnerContext {
                        file,
                        source,
                        class_context,
                        imports,
                        module_constants,
                    },
                    class.name.as_str(),
                    class.range,
                    &class.decorator_list,
                ));
            }
            _ => {}
        }
    }
}

fn push_function_owner(stmt: &Stmt, context: PythonOwnerContext<'_>, out: &mut Vec<PythonOwner>) {
    match stmt {
        Stmt::FunctionDef(function) => out.push(owner_from_function(
            context,
            function.name.as_str(),
            function.range,
            &function.decorator_list,
            &function.args,
            &function.body,
            false,
        )),
        Stmt::AsyncFunctionDef(function) => out.push(owner_from_function(
            context,
            function.name.as_str(),
            function.range,
            &function.decorator_list,
            &function.args,
            &function.body,
            true,
        )),
        _ => {}
    }
}

/// The innermost class of a dotted class path (`Inner` for `Outer.Inner`).
fn innermost_class(class_path: &str) -> &str {
    class_path.rsplit('.').next().unwrap_or(class_path)
}

#[derive(Clone, Copy)]
struct PythonOwnerContext<'a> {
    file: &'a Path,
    source: &'a SourceText<'a>,
    class_context: Option<&'a str>,
    imports: &'a [PythonImport],
    module_constants: &'a [PythonModuleConstant],
}

/// Whether the statements that run when the enclosing function runs call the
/// nested def `name` by name (#12141). `statements` is the enclosing body. The
/// bodies of nested defs and classes, at any depth through `if`/`with`/`for`/
/// `try`/`match` blocks, and every `lambda` expression are excluded, so a call
/// from an unused sibling def, from the nested body itself, from a class body
/// or from a lambda is not an edge from the enclosing function (#12197).
/// What a nested declaration evaluates when it is defined (decorators, default
/// values, annotations, class bases) still runs and still counts. A spaced
/// call (`inner (n)`) counts. A body that rebinds the name (assignment,
/// walrus, `for`, `as`, `import`) or defines it more than once is fail-closed:
/// the call may reach a different binding. Returning or passing the def
/// without calling it is not a call.
fn body_calls_nested_def(source: &str, statements: &[Stmt], name: &str) -> bool {
    let mut scan = DeferredRanges::default();
    collect_deferred_ranges(statements, name, &mut scan);
    if scan.definitions > 1 {
        return false;
    }
    super::module_constants::visit_exprs_in_statements(statements, &mut |expr| {
        if let Expr::Lambda(lambda) = expr {
            scan.lambdas.push(span(lambda.range));
        }
    });
    let executed = statements
        .iter()
        .map(|stmt| match stmt {
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => {
                // Only what the declaration evaluates runs with the body.
                let mut eager = DeferredRanges::default();
                declaration_eager_ranges(stmt, &mut eager.eager);
                eager.lambdas.clone_from(&scan.lambdas);
                eager
                    .eager
                    .iter()
                    .map(|&(from, to)| {
                        text_without_ranges(source, from, to, &[], &[], &scan.lambdas)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => {
                let range = stmt.range();
                text_without_ranges(
                    source,
                    usize::from(range.start()),
                    usize::from(range.end()),
                    &scan.deferred,
                    &scan.eager,
                    &scan.lambdas,
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let ident = |ch: char| ch == '_' || ch.is_alphanumeric();
    let mut calls = false;
    let mut rebinds = false;
    for (idx, _) in executed.match_indices(name) {
        let end = idx + name.len();
        let bounded = super::static_limits::python_callee_start_has_boundary(&executed, idx)
            && executed[end..].chars().next().is_none_or(|ch| !ident(ch));
        if !bounded {
            continue;
        }
        let after = executed[end..].trim_start_matches([' ', '\t']);
        let before = executed[..idx].trim_end_matches([' ', '\t']);
        if after.starts_with('(') && super::related_tests::is_bare_call_site(&executed, idx) {
            calls = true;
        } else if (after.starts_with('=') && !after.starts_with("=="))
            || after.starts_with(":=")
            || matches!(
                before.rsplit(char::is_whitespace).next(),
                Some("as" | "for" | "import")
            )
        {
            rebinds = true;
        }
    }
    calls && !rebinds
}

/// Byte ranges found while reading an enclosing body for a nested-def call.
#[derive(Default)]
struct DeferredRanges {
    /// Nested defs and classes: their text does not run with the body.
    deferred: Vec<(usize, usize)>,
    /// Inside a deferred declaration, what it evaluates when defined.
    eager: Vec<(usize, usize)>,
    /// `lambda` expressions: their bodies run only if something calls them.
    lambdas: Vec<(usize, usize)>,
    /// Defs named like the target, anywhere in the enclosing body's blocks.
    definitions: usize,
}

fn span(range: TextRange) -> (usize, usize) {
    (usize::from(range.start()), usize::from(range.end()))
}

fn collect_deferred_ranges(statements: &[Stmt], name: &str, scan: &mut DeferredRanges) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => {
                scan.deferred.push(span(stmt.range()));
                declaration_eager_ranges(stmt, &mut scan.eager);
                if function.name.as_str() == name {
                    scan.definitions += 1;
                }
            }
            Stmt::AsyncFunctionDef(function) => {
                scan.deferred.push(span(stmt.range()));
                declaration_eager_ranges(stmt, &mut scan.eager);
                if function.name.as_str() == name {
                    scan.definitions += 1;
                }
            }
            Stmt::ClassDef(_) => {
                scan.deferred.push(span(stmt.range()));
                declaration_eager_ranges(stmt, &mut scan.eager);
            }
            _ => {
                for block in super::admission::nested_blocks(stmt) {
                    collect_deferred_ranges(block, name, scan);
                }
            }
        }
    }
}

/// The expressions a def or class evaluates when it is defined: decorators,
/// default values, return and argument annotations, class bases and keywords.
fn declaration_eager_ranges(stmt: &Stmt, out: &mut Vec<(usize, usize)>) {
    let mut push = |expr: &Expr| out.push(span(expr.range()));
    let arguments = |args: &ast::Arguments, push: &mut dyn FnMut(&Expr)| {
        for arg in args
            .posonlyargs
            .iter()
            .chain(&args.args)
            .chain(&args.kwonlyargs)
        {
            if let Some(default) = arg.default.as_deref() {
                push(default);
            }
            if let Some(annotation) = arg.def.annotation.as_deref() {
                push(annotation);
            }
        }
    };
    match stmt {
        Stmt::FunctionDef(function) => {
            function.decorator_list.iter().for_each(&mut push);
            arguments(&function.args, &mut push);
            function.returns.as_deref().into_iter().for_each(&mut push);
        }
        Stmt::AsyncFunctionDef(function) => {
            function.decorator_list.iter().for_each(&mut push);
            arguments(&function.args, &mut push);
            function.returns.as_deref().into_iter().for_each(&mut push);
        }
        Stmt::ClassDef(class) => {
            class.decorator_list.iter().for_each(&mut push);
            class.bases.iter().for_each(&mut push);
            class
                .keywords
                .iter()
                .for_each(|keyword| push(&keyword.value));
        }
        _ => {}
    }
}

/// The text of `[from, to)` with every byte inside a deferred or lambda range
/// blanked to a space (newlines kept so offsets and lines are stable), except
/// bytes inside an eager range, which a deferred declaration still evaluates.
/// Lambda ranges are blanked last, so a lambda in a default stays deferred.
fn text_without_ranges(
    source: &str,
    from: usize,
    to: usize,
    deferred: &[(usize, usize)],
    eager: &[(usize, usize)],
    lambdas: &[(usize, usize)],
) -> String {
    let from = from.min(source.len());
    let to = to.min(source.len());
    let original = source.get(from..to).unwrap_or_default().as_bytes();
    let mut bytes = original.to_vec();
    let blank = |bytes: &mut Vec<u8>, ranges: &[(usize, usize)]| {
        for &(start, end) in ranges {
            let start = start.saturating_sub(from).min(bytes.len());
            let end = end.saturating_sub(from).min(bytes.len());
            for byte in bytes.iter_mut().take(end).skip(start) {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
    };
    blank(&mut bytes, deferred);
    for &(start, end) in eager {
        let start = start.saturating_sub(from).min(bytes.len());
        let end = end.saturating_sub(from).min(bytes.len());
        if let (Some(dst), Some(src)) = (bytes.get_mut(start..end), original.get(start..end)) {
            dst.copy_from_slice(src);
        }
    }
    blank(&mut bytes, lambdas);
    String::from_utf8(bytes).unwrap_or_default()
}

fn owner_from_function(
    context: PythonOwnerContext<'_>,
    name: &str,
    range: TextRange,
    decorators: &[Expr],
    args: &ast::Arguments,
    body: &[Stmt],
    is_async: bool,
) -> PythonOwner {
    let decorator_names = decorator_names(decorators);
    // Only bare `classmethod` / `staticmethod` / `property` names are known
    // not to wrap the call; any other decorator (unreadable, call form,
    // attribute) may not call the function, so it yields no helper edges.
    let transparent_decorators = decorators.iter().all(|decorator| {
        matches!(decorator, Expr::Name(_))
            && expr_full_name(decorator).is_some_and(|name| {
                matches!(name.as_str(), "classmethod" | "staticmethod" | "property")
            })
    });
    // A click command's decorators build a command that runs this body when
    // invoked, so a helper it calls is reached through the command (#9226).
    let click_command = !is_async
        && decorator_names.len() == decorators.len()
        && decorator_names
            .iter()
            .any(|decorator| matches!(decorator.as_str(), "click.command" | "click.group"))
        && decorator_names.iter().all(|decorator| {
            matches!(
                decorator.as_str(),
                "click.command" | "click.group" | "click.option" | "click.argument"
            )
        })
        && context
            .imports
            .iter()
            .any(super::cli_runner::is_plain_click_import);
    // A decorator whose name cannot be read still wraps the call.
    let binding_only = !is_async
        && decorator_names.len() == decorators.len()
        && decorator_names
            .iter()
            .all(|decorator| matches!(decorator.as_str(), "classmethod" | "staticmethod"));
    let owner_kind = if context.class_context.is_some()
        && decorator_names.iter().any(|decorator| {
            decorator.ends_with("classmethod") || decorator.ends_with("staticmethod")
        }) {
        OwnerKind::ClassMethod
    } else if context.class_context.is_some() {
        OwnerKind::Method
    } else {
        OwnerKind::Function
    };
    let qualified_name = context
        .class_context
        .map(|class| format!("{}.{name}", innermost_class(class)))
        .unwrap_or_else(|| name.to_string());
    let route_paths = collect_static_route_paths(context.source, decorators);
    let route_methods = collect_static_route_methods(context.source, decorators);
    let route_receivers = collect_static_route_receivers(context.source, decorators);
    let dynamic_route_decorators = collect_dynamic_route_decorators(context.source, decorators);
    let unplain_decorators = called_decorator_names(decorators);
    let decorator_lines = decorators
        .first()
        .zip(decorators.last())
        .map(|(first, last)| {
            (
                line_for_range_start(context.source, first.range()),
                line_for_range_end(context.source, last.range()),
            )
        });
    let mut decorators = decorator_names;
    if is_async {
        decorators.push("async_def".to_string());
    }
    let parameters = function_parameters(context.source, args);
    let term_facts = super::term_activation::term_facts(
        context.source,
        body,
        &parameters,
        &text_for_range(context.source, range),
        !decorators.is_empty() || is_async,
        binding_only,
    )
    .with_calls_own_name(super::module_callees::calls_own_name(
        &text_for_range(context.source, range),
        name,
    ));
    let same_class_callees = super::same_class_callees::same_class_callees_from_body(
        body,
        super::same_class_callees::method_receiver_name(&parameters, &decorators),
    );
    let module_callees = if transparent_decorators || click_command {
        super::module_callees::bare_callees(&text_for_range(context.source, range), &parameters)
    } else {
        Vec::new()
    };
    PythonOwner {
        name: name.to_string(),
        qualified_name,
        file: context.file.to_path_buf(),
        start_line: line_for_range_start(context.source, range),
        end_line: line_for_range_end(context.source, range),
        owner_kind: Some(owner_kind),
        decorators,
        imports: context.imports.to_vec(),
        cli_receiver_names: collect_static_cli_receiver_names(context.source, context.imports),
        route_paths,
        route_methods,
        route_receivers,
        dynamic_route_decorators,
        unplain_decorators,
        decorator_lines,
        parameters,
        reexport_modules: Vec::new(),
        star_exported: false,
        ambiguous_src_modules: Vec::new(),
        rival_definition_modules: Vec::new(),
        module_constants: constants_visible_in_function(
            context.module_constants,
            args,
            body,
            &text_for_range(context.source, range),
        ),
        same_class_callees,
        module_callees,
        class_path: context.class_context.unwrap_or_default().to_string(),
        term_facts,
        class_facts: super::related_tests::PythonClassFacts {
            direct_statements: direct_expression_statements(context.source, body),
            bound_names: super::module_constants::function_bound_names(
                args,
                body,
                &text_for_range(context.source, range),
            ),
            ..Default::default()
        },
        foreign_writes: Default::default(),
        version_conditional: false,
        cli_subcommand: None,
        nested_called_by_parent: false,
    }
}

/// The trimmed text of each single-line expression statement written directly
/// in a function body (not under an `if`, `try`, loop or `with`), so a changed
/// line can be told apart from a guarded one (#9421).
fn direct_expression_statements(source: &SourceText<'_>, body: &[Stmt]) -> Vec<String> {
    body.iter()
        .filter_map(|stmt| match stmt {
            Stmt::Expr(expr) => Some(text_for_range(source, expr.range)),
            Stmt::Assign(assign) => Some(text_for_range(source, assign.range)),
            Stmt::AugAssign(assign) => Some(text_for_range(source, assign.range)),
            _ => None,
        })
        .map(|text| text.trim().to_string())
        .filter(|text| !text.contains('\n'))
        .collect()
}

/// Whether the class body holds only method definitions, a docstring, `pass`
/// and plain non-dunder attributes, so no dunder is assigned, regenerated or
/// defined conditionally in it.
fn class_body_is_plain(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| match stmt {
        Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::Pass(_) | Stmt::ClassDef(_) => {
            true
        }
        Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(_)),
        Stmt::Assign(assign) => assign
            .targets
            .iter()
            .all(|target| matches!(target, Expr::Name(name) if !name.id.as_str().starts_with("__"))),
        Stmt::AnnAssign(assign) => {
            matches!(assign.target.as_ref(), Expr::Name(name) if !name.id.as_str().starts_with("__"))
        }
        _ => false,
    })
}

/// Declared parameters in binding order: positional-only, then regular, then
/// keyword-only. `*args` / `**kwargs` are not bindable names and are omitted.
fn function_parameters(source: &str, args: &ast::Arguments) -> Vec<PythonParameter> {
    let parameter =
        |arg: &ast::ArgWithDefault, keyword_only: bool, positional_only: bool| PythonParameter {
            name: arg.def.arg.to_string(),
            default: arg
                .default
                .as_ref()
                .map(|default| text_for_range(source, default.range()).trim().to_string()),
            keyword_only,
            positional_only,
        };
    args.posonlyargs
        .iter()
        .map(|arg| parameter(arg, false, true))
        .chain(args.args.iter().map(|arg| parameter(arg, false, false)))
        .chain(
            args.kwonlyargs
                .iter()
                .map(|arg| parameter(arg, true, false)),
        )
        .collect()
}

fn owner_from_class(
    context: PythonOwnerContext<'_>,
    name: &str,
    range: TextRange,
    decorators: &[Expr],
) -> PythonOwner {
    let qualified_name = context
        .class_context
        .map(|class| format!("{}.{name}", innermost_class(class)))
        .unwrap_or_else(|| name.to_string());
    PythonOwner {
        name: name.to_string(),
        qualified_name,
        file: context.file.to_path_buf(),
        start_line: line_for_range_start(context.source, range),
        end_line: line_for_range_end(context.source, range),
        owner_kind: None,
        decorators: decorator_names(decorators),
        imports: context.imports.to_vec(),
        cli_receiver_names: collect_static_cli_receiver_names(context.source, context.imports),
        route_paths: collect_static_route_paths(context.source, decorators),
        route_methods: collect_static_route_methods(context.source, decorators),
        route_receivers: collect_static_route_receivers(context.source, decorators),
        dynamic_route_decorators: collect_dynamic_route_decorators(context.source, decorators),
        unplain_decorators: called_decorator_names(decorators),
        decorator_lines: None,
        parameters: Vec::new(),
        reexport_modules: Vec::new(),
        star_exported: false,
        ambiguous_src_modules: Vec::new(),
        rival_definition_modules: Vec::new(),
        module_constants: Vec::new(),
        same_class_callees: Vec::new(),
        module_callees: Vec::new(),
        class_path: String::new(),
        term_facts: Default::default(),
        class_facts: Default::default(),
        foreign_writes: Default::default(),
        version_conditional: false,
        cli_subcommand: None,
        nested_called_by_parent: false,
    }
}

pub(super) fn module_owner(
    file: &Path,
    source: &SourceText<'_>,
    range: TextRange,
    imports: &[PythonImport],
) -> PythonOwner {
    PythonOwner {
        name: "<module>".to_string(),
        qualified_name: "<module>".to_string(),
        file: file.to_path_buf(),
        start_line: line_for_range_start(source, range),
        end_line: line_for_range_end(source, range),
        owner_kind: Some(OwnerKind::ModuleFunction),
        decorators: Vec::new(),
        imports: imports.to_vec(),
        cli_receiver_names: collect_static_cli_receiver_names(source, imports),
        route_paths: Vec::new(),
        route_methods: Vec::new(),
        route_receivers: Vec::new(),
        dynamic_route_decorators: Vec::new(),
        unplain_decorators: Vec::new(),
        decorator_lines: None,
        parameters: Vec::new(),
        reexport_modules: Vec::new(),
        star_exported: false,
        ambiguous_src_modules: Vec::new(),
        rival_definition_modules: Vec::new(),
        module_constants: Vec::new(),
        same_class_callees: Vec::new(),
        module_callees: Vec::new(),
        class_path: String::new(),
        term_facts: Default::default(),
        class_facts: Default::default(),
        foreign_writes: Default::default(),
        version_conditional: false,
        cli_subcommand: None,
        nested_called_by_parent: false,
    }
}

#[cfg(test)]
pub(super) fn extract_tests(file: &Path, source: &str) -> Vec<PythonTest> {
    extract_source_facts(file, source).tests
}

/// Whether a sibling class lists `class_name` among its bases.
fn class_is_listed_as_base(statements: &[Stmt], class_name: &str) -> bool {
    statements.iter().any(|stmt| {
        let Stmt::ClassDef(other) = stmt else {
            return false;
        };
        other.name.as_str() != class_name
            && other.bases.iter().any(|base| match base {
                Expr::Name(name) => name.id.as_str() == class_name,
                Expr::Attribute(attribute) => attribute.attr.as_str() == class_name,
                _ => false,
            })
    })
}

/// Python decorators and method bodies use different binding scopes.
#[derive(Clone, Copy)]
pub(super) struct TestImportContext<'a> {
    pub(super) body: &'a [PythonImport],
    pub(super) definition: &'a [PythonImport],
    /// The module's never-rebound `NAME = <value>` bindings a parametrize
    /// decorator may name (#10685).
    pub(super) module_values: &'a PythonModuleNames<'a>,
    pub(super) admission: &'a PythonAdmissionContext,
    pub(super) standard_unittest_list: bool,
    /// Another local class lists this one as a base, so a subclass may replace
    /// its setup hooks.
    pub(super) subclassed: bool,
}

/// Preserve each assertion's native byte position inside the exact test slice.
fn test_assertions(
    statements: &[Stmt],
    source: &SourceText<'_>,
    test_range: TextRange,
    standard_unittest_list: bool,
) -> Vec<super::PythonAssertion> {
    let mut assertions =
        collect_assertions_from_statements(statements, source, standard_unittest_list);
    for assertion in &mut assertions {
        assertion.source_range = assertion.source_range.and_then(|range| {
            (range.start() >= test_range.start() && range.end() <= test_range.end()).then(|| {
                TextRange::new(
                    range.start() - test_range.start(),
                    range.end() - test_range.start(),
                )
            })
        });
        assertion.observed_operands.retain_mut(|(_, range)| {
            if range.start() < test_range.start() || range.end() > test_range.end() {
                return false;
            }
            *range = TextRange::new(
                range.start() - test_range.start(),
                range.end() - test_range.start(),
            );
            true
        });
    }
    assertions
}

/// Follow the default pytest `python_functions` and unittest
/// `TestLoader.testMethodPrefix`: both use `test`, not `test_`.
/// Custom collection prefixes and hooks are not resolved here.
pub(super) fn collect_tests_from_statements(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
    class_context: Option<&str>,
    in_unittest_class: bool,
    import_context: TestImportContext<'_>,
    out: &mut Vec<PythonTest>,
) {
    let imports = import_context.body;
    let admission = import_context.admission;
    let mut definition_imports = import_context.definition.to_vec();
    // A decorator in a class body looks a name up there before the module.
    let class_names = if class_context.is_some() {
        super::module_constants::class_scope_names(statements)
    } else {
        Some(Vec::new())
    };
    let base_parametrize_scope = ParametrizeScope {
        imports: &[],
        module_values: import_context.module_values,
        class_names: class_names.as_deref().unwrap_or_default(),
        class_opaque: class_names.is_none(),
    };
    let local_classes = LocalTestClasses::of(
        file,
        statements,
        import_context.definition,
        base_parametrize_scope,
    );
    let setup_receivers = if class_context.is_some() {
        super::setup_receivers::setup_receivers(
            source,
            statements,
            import_context.subclassed,
            in_unittest_class,
        )
    } else {
        Vec::new()
    };
    for (index, stmt) in statements.iter().enumerate() {
        apply_definition_import(file, stmt, &mut definition_imports);
        let first_pushed = out.len();
        match stmt {
            Stmt::FunctionDef(function) if function.name.as_str().starts_with("test") => {
                let framework = if in_unittest_class {
                    PythonFramework::Unittest
                } else {
                    PythonFramework::Pytest
                };
                let name = function.name.to_string();
                let generated_inputs = super::hypothesis::given_parameter_names(
                    &function.args,
                    &function.decorator_list,
                );
                let parametrize_scope = ParametrizeScope {
                    imports: &definition_imports,
                    ..base_parametrize_scope
                };
                let direct_parametrized = if framework == PythonFramework::Pytest {
                    super::parametrize::direct_argnames(&function.decorator_list, parametrize_scope)
                } else {
                    Vec::new()
                };
                let unsettled_indirect = if framework == PythonFramework::Pytest {
                    super::parametrize::unsettled_indirect_argnames(
                        &function.decorator_list,
                        parametrize_scope,
                    )
                } else {
                    Vec::new()
                };
                let parametrize_argnames = if framework == PythonFramework::Pytest {
                    super::parametrize::parametrize_argnames(
                        &function.decorator_list,
                        parametrize_scope,
                    )
                } else {
                    Vec::new()
                };
                let fixtures: Vec<String> = fixture_parameter_names(&function.args, framework)
                    .into_iter()
                    .filter(|name| !generated_inputs.contains(name))
                    .collect();
                let assertions = test_assertions(
                    &function.body,
                    source,
                    function.range,
                    import_context.standard_unittest_list
                        && super::oracles::unittest_list::eligible_method(
                            &function.args,
                            &function.decorator_list,
                        ),
                );
                // pytest does not parametrize unittest methods; Hypothesis
                // `@example` rows run under either framework.
                let parametrize = (framework == PythonFramework::Pytest)
                    .then(|| parametrize_cases(source, &function.decorator_list, parametrize_scope))
                    .flatten()
                    .or_else(|| {
                        super::hypothesis::example_cases(
                            source,
                            &function.args,
                            &function.decorator_list,
                        )
                    });
                let imports = test_imports(file, imports, &function.body);
                let assertion_admission = assertion_admission(
                    &PythonTestFunction {
                        body: &function.body,
                        decorators: &function.decorator_list,
                        parameters: &fixtures,
                        imports: &imports,
                        class_path: class_context,
                    },
                    &assertions,
                    parametrize.as_ref().map(|cases| cases.argnames()).as_ref(),
                    admission,
                );
                let (parametrize, body_text) = unpack_parametrize_argname(
                    source,
                    function.range,
                    &function.decorator_list,
                    &function.body,
                    parametrize,
                    &fixtures,
                );
                out.push(PythonTest {
                    qualified_name: qualified_test_name(class_context, &name),
                    name,
                    file: file.to_path_buf(),
                    line: line_for_range_start(source, function.range),
                    body_text,
                    imports,
                    decorators: decorator_names(&function.decorator_list),
                    simple_literal_parametrize: simple_literal_parametrize(
                        &function.decorator_list,
                    ),
                    activation_controls: super::test_activation::activation_controls(
                        &function.decorator_list,
                        &definition_imports,
                        super::test_activation::DeclarationSite::Function,
                    )
                    .into_iter()
                    .chain(super::test_activation::empty_parametrize_control(
                        &function.decorator_list,
                    ))
                    .chain(
                        (framework == PythonFramework::Pytest
                            && super::parametrize::collection_error(
                                &function.decorator_list,
                                parametrize_scope,
                            ))
                        .then(|| {
                            super::test_activation::parametrize_collection_error_control(
                                super::test_activation::DeclarationSite::Function,
                            )
                        }),
                    )
                    .collect(),
                    fixtures,
                    setup_receivers: setup_receivers.clone(),
                    fixture_defs: Vec::new(),
                    direct_parametrized,
                    unsettled_indirect,
                    parametrize_argnames,
                    parametrized: is_parametrized(&function.decorator_list)
                        || super::parametrize::applies_parametrize(
                            &function.decorator_list,
                            parametrize_scope,
                        ),
                    parametrize,
                    generated_inputs,
                    framework,
                    assertions,
                    assertion_admission,
                    bare_sibling_modules: Vec::new(),
                    constant_rebinding: python_test_rebinding(
                        &function.args,
                        &function.body,
                        &text_for_range(source, function.range),
                    ),
                });
            }
            Stmt::AsyncFunctionDef(function) if function.name.as_str().starts_with("test") => {
                let framework = if in_unittest_class {
                    PythonFramework::Unittest
                } else {
                    PythonFramework::Pytest
                };
                let name = function.name.to_string();
                let generated_inputs = super::hypothesis::given_parameter_names(
                    &function.args,
                    &function.decorator_list,
                );
                let parametrize_scope = ParametrizeScope {
                    imports: &definition_imports,
                    ..base_parametrize_scope
                };
                let direct_parametrized = if framework == PythonFramework::Pytest {
                    super::parametrize::direct_argnames(&function.decorator_list, parametrize_scope)
                } else {
                    Vec::new()
                };
                let unsettled_indirect = if framework == PythonFramework::Pytest {
                    super::parametrize::unsettled_indirect_argnames(
                        &function.decorator_list,
                        parametrize_scope,
                    )
                } else {
                    Vec::new()
                };
                let parametrize_argnames = if framework == PythonFramework::Pytest {
                    super::parametrize::parametrize_argnames(
                        &function.decorator_list,
                        parametrize_scope,
                    )
                } else {
                    Vec::new()
                };
                let fixtures: Vec<String> = fixture_parameter_names(&function.args, framework)
                    .into_iter()
                    .filter(|name| !generated_inputs.contains(name))
                    .collect();
                let assertions = test_assertions(&function.body, source, function.range, false);
                // pytest does not parametrize unittest methods; Hypothesis
                // `@example` rows run under either framework.
                let parametrize = (framework == PythonFramework::Pytest)
                    .then(|| parametrize_cases(source, &function.decorator_list, parametrize_scope))
                    .flatten()
                    .or_else(|| {
                        super::hypothesis::example_cases(
                            source,
                            &function.args,
                            &function.decorator_list,
                        )
                    });
                let imports = test_imports(file, imports, &function.body);
                let assertion_admission = assertion_admission(
                    &PythonTestFunction {
                        body: &function.body,
                        decorators: &function.decorator_list,
                        parameters: &fixtures,
                        imports: &imports,
                        class_path: class_context,
                    },
                    &assertions,
                    parametrize.as_ref().map(|cases| cases.argnames()).as_ref(),
                    admission,
                );
                let (parametrize, body_text) = unpack_parametrize_argname(
                    source,
                    function.range,
                    &function.decorator_list,
                    &function.body,
                    parametrize,
                    &fixtures,
                );
                out.push(PythonTest {
                    qualified_name: qualified_test_name(class_context, &name),
                    name,
                    file: file.to_path_buf(),
                    line: line_for_range_start(source, function.range),
                    body_text,
                    imports,
                    decorators: decorator_names(&function.decorator_list),
                    simple_literal_parametrize: simple_literal_parametrize(
                        &function.decorator_list,
                    ),
                    activation_controls: super::test_activation::activation_controls(
                        &function.decorator_list,
                        &definition_imports,
                        super::test_activation::DeclarationSite::Function,
                    )
                    .into_iter()
                    .chain(super::test_activation::empty_parametrize_control(
                        &function.decorator_list,
                    ))
                    .chain(
                        (framework == PythonFramework::Pytest
                            && super::parametrize::collection_error(
                                &function.decorator_list,
                                parametrize_scope,
                            ))
                        .then(|| {
                            super::test_activation::parametrize_collection_error_control(
                                super::test_activation::DeclarationSite::Function,
                            )
                        }),
                    )
                    .collect(),
                    fixtures,
                    setup_receivers: setup_receivers.clone(),
                    fixture_defs: Vec::new(),
                    direct_parametrized,
                    unsettled_indirect,
                    parametrize_argnames,
                    parametrized: is_parametrized(&function.decorator_list)
                        || super::parametrize::applies_parametrize(
                            &function.decorator_list,
                            parametrize_scope,
                        ),
                    parametrize,
                    generated_inputs,
                    framework,
                    assertions,
                    assertion_admission,
                    bare_sibling_modules: Vec::new(),
                    constant_rebinding: python_test_rebinding(
                        &function.args,
                        &function.body,
                        &text_for_range(source, function.range),
                    ),
                });
            }
            Stmt::ClassDef(class) if local_classes.is_last_definition(class) => {
                let class_name = class.name.as_str();
                let class_is_unittest =
                    in_unittest_class || local_classes.unittest.contains(class_name);
                if class_is_unittest || local_classes.pytest.contains(class_name) {
                    let first_class_test = out.len();
                    let nested_class_context = qualified_test_name(class_context, class_name);
                    // Decorators see class-local bindings; method bodies do
                    // not. Keep global/body-call identity on the old context.
                    collect_tests_from_statements(
                        file,
                        source,
                        &class.body,
                        Some(&nested_class_context),
                        class_is_unittest,
                        TestImportContext {
                            body: imports,
                            definition: &definition_imports,
                            module_values: import_context.module_values,
                            admission,
                            standard_unittest_list: class_context.is_none()
                                && super::oracles::unittest_list::eligible_class(
                                    class,
                                    &definition_imports,
                                ),
                            subclassed: class_is_listed_as_base(statements, class_name),
                        },
                        out,
                    );
                    // Mixin members this class resolves to run as this
                    // class's tests, under its selector
                    // (`test_x.py::TestConsumer::test_shared`).
                    for (mixin, members) in local_classes
                        .inherited
                        .get(class_name)
                        .into_iter()
                        .flatten()
                    {
                        let mut from_mixin = Vec::new();
                        collect_tests_from_statements(
                            file,
                            source,
                            &mixin.body,
                            Some(&nested_class_context),
                            class_is_unittest,
                            TestImportContext {
                                body: imports,
                                definition: local_classes
                                    .definition_imports
                                    .get(mixin.name.as_str())
                                    .map(Vec::as_slice)
                                    .unwrap_or(&definition_imports),
                                module_values: import_context.module_values,
                                admission,
                                standard_unittest_list: false,
                                // A consumer class runs the mixin's tests under its own hooks.
                                subclassed: true,
                            },
                            &mut from_mixin,
                        );
                        for test in &mut from_mixin {
                            test.decorators
                                .extend(decorator_names(&mixin.decorator_list));
                        }
                        out.extend(from_mixin.into_iter().filter(|test| {
                            members.contains(test.name.as_str())
                                && test.qualified_name
                                    == qualified_test_name(Some(&nested_class_context), &test.name)
                        }));
                    }
                    for test in &mut out[first_class_test..] {
                        test.activation_controls.extend(
                            local_classes
                                .class_activation_controls
                                .get(class_name)
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                        test.direct_parametrized.extend(
                            local_classes
                                .class_direct_parametrized
                                .get(class_name)
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                        test.unsettled_indirect.extend(
                            local_classes
                                .class_unsettled_indirect
                                .get(class_name)
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                        if let Some(names) =
                            local_classes.class_parametrize_argnames.get(class_name)
                        {
                            test.parametrize_argnames.extend(names.iter().cloned());
                        }
                        // A class and its method (or two classes) that
                        // parametrize one name fail collection, as does any
                        // refused mark on either (#10867): the test runs no
                        // case, so every argname is unsettled and the test
                        // carries the collection-error control.
                        if super::parametrize::has_duplicate(
                            test.parametrize_argnames.iter().map(String::as_str),
                        ) || super::test_activation::has_collection_error(
                            &test.activation_controls,
                        ) {
                            if !super::test_activation::has_collection_error(
                                &test.activation_controls,
                            ) {
                                test.activation_controls.push(
                                    super::test_activation::parametrize_collection_error_control(
                                        super::test_activation::DeclarationSite::Class,
                                    ),
                                );
                            }
                            for name in &test.parametrize_argnames {
                                if !test.direct_parametrized.contains(name) {
                                    test.direct_parametrized.push(name.clone());
                                }
                                if !test.unsettled_indirect.contains(name) {
                                    test.unsettled_indirect.push(name.clone());
                                }
                            }
                        }
                        test.decorators.extend(
                            local_classes
                                .class_decorators
                                .get(class_name)
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                    }
                }
            }
            _ => {}
        }
        // Pytest and unittest collect the last binding of a name in a scope:
        // an earlier definition (or one later replaced by `name = None`) never
        // runs.
        let defined = match stmt {
            Stmt::FunctionDef(function) => Some(function.name.as_str()),
            Stmt::AsyncFunctionDef(function) => Some(function.name.as_str()),
            Stmt::ClassDef(class) => Some(class.name.as_str()),
            _ => None,
        };
        if let Some(defined) = defined
            && super::test_activation::rebound_later(statements, index, defined, source)
        {
            for test in &mut out[first_pushed..] {
                test.activation_controls.push(
                    super::test_activation::shadowed_by_later_binding_control(
                        super::test_activation::DeclarationSite::Function,
                    ),
                );
            }
        }
    }
}

/// The narrow literal shape required for positive whole-row observation.
fn simple_literal_parametrize(decorators: &[Expr]) -> bool {
    fn literal(expr: &Expr) -> bool {
        match expr {
            Expr::Constant(_) => true,
            Expr::Tuple(value) => value.elts.iter().all(literal),
            Expr::List(value) => value.elts.iter().all(literal),
            Expr::UnaryOp(value) if matches!(value.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
                matches!(value.operand.as_ref(), Expr::Constant(constant) if matches!(constant.value, ast::Constant::Int(_) | ast::Constant::Float(_)))
            }
            _ => false,
        }
    }
    let [Expr::Call(call)] = decorators else {
        return false;
    };
    call.args.len() == 2 && call.keywords.is_empty() && call.args.iter().all(literal)
}

/// The test's cases without the argnames its body rebinds, read through a
/// leading `a, b = argname` unpack when there is one (#10832), and its body
/// text with that unpack blanked (offsets kept).
fn unpack_parametrize_argname(
    source: &str,
    range: TextRange,
    decorators: &[Expr],
    body: &[Stmt],
    parametrize: Option<super::parametrize::PythonParametrizeCases>,
    parameters: &[String],
) -> (Option<super::parametrize::PythonParametrizeCases>, String) {
    let mut body_text = text_for_range(source, range);
    let parametrize = parametrize.and_then(|cases| cases.excluding_body_bindings(body));
    // A decorator other than a pytest mark may call the test with another
    // row, as the literal-params fixture path also refuses.
    if !decorator_names(decorators)
        .iter()
        .all(|decorator| decorator.starts_with("pytest.mark."))
    {
        return (parametrize, body_text);
    }
    let Some((cases, span)) = parametrize.as_ref().and_then(|cases| {
        super::param_fixtures::unpacked_parametrize_cases(cases, body, source, parameters)
    }) else {
        return (parametrize, body_text);
    };
    let start = usize::from(range.start());
    let (Some(from), Some(to)) = (span.start.checked_sub(start), span.end.checked_sub(start))
    else {
        return (parametrize, body_text);
    };
    if !super::param_fixtures::blank_range(&mut body_text, from..to) {
        return (parametrize, body_text);
    }
    (Some(cases), body_text)
}

/// Module imports plus the imports at the top level of the test body
/// (`def test_x(): from pkg.utils import sign`), which bind the same way for
/// the rest of the test (#4567). A later import of the same name replaces the
/// earlier one, as Python binds it: `from owner import sign` followed by
/// `from other import sign` leaves only `other.sign`.
pub(super) fn test_imports(
    file: &Path,
    module_imports: &[PythonImport],
    body: &[Stmt],
) -> Vec<PythonImport> {
    let mut imports = module_imports.to_vec();
    for import in collect_imports_from_statements(file, body) {
        imports.retain(|earlier| earlier.alias != import.alias);
        imports.push(import);
    }
    imports
}

/// Decorators execute when a declaration is reached. Later imports in its
/// module/class must not retroactively change the decorator's identity.
fn apply_definition_import(file: &Path, stmt: &Stmt, imports: &mut Vec<PythonImport>) {
    if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)) {
        for mut import in collect_imports_from_statements(file, std::slice::from_ref(stmt)) {
            // `import unittest.mock` binds `unittest`, unlike
            // `import unittest.mock as helper`. Keep this declaration-only
            // binding model separate from owner/module path identities.
            if import.source_module.is_empty()
                && import.alias == import.imported
                && let Some((root, _)) = import.imported.split_once('.')
            {
                let root = root.to_string();
                import.alias.clone_from(&root);
                import.imported = root;
            }
            imports.retain(|earlier| earlier.alias != import.alias);
            imports.push(import);
        }
    }
}

fn qualified_test_name(class_context: Option<&str>, name: &str) -> String {
    class_context
        .map(|class| format!("{class}.{name}"))
        .unwrap_or_else(|| name.to_string())
}

fn fixture_parameter_names(args: &ast::Arguments, framework: PythonFramework) -> Vec<String> {
    let mut names: Vec<String> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
        .map(|arg| arg.def.arg.to_string())
        .collect();
    if let Some(arg) = &args.vararg {
        names.push(arg.arg.to_string());
    }
    if let Some(arg) = &args.kwarg {
        names.push(arg.arg.to_string());
    }
    names.retain(|name| {
        name != "self"
            && name != "cls"
            && (framework == PythonFramework::Pytest || !matches!(name.as_str(), "subTest"))
    });
    names.sort();
    names.dedup();
    names
}

pub(super) fn collect_imports_from_statements(
    file: &Path,
    statements: &[Stmt],
) -> Vec<PythonImport> {
    let mut imports = Vec::new();
    for stmt in statements {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let imported = alias.name.to_string();
                    imports.push(PythonImport {
                        alias: alias
                            .asname
                            .as_ref()
                            .map(|name| name.to_string())
                            .unwrap_or_else(|| imported.clone()),
                        imported,
                        // A plain `import X` has no `from` module source.
                        source_module: String::new(),
                    });
                }
            }
            Stmt::ImportFrom(import) => {
                // `from src.handler import validate [as v]` — the source module
                // (`src.handler`) is the free-function identity evidence. For
                // package-local tests, resolve explicit relative imports against
                // the importing file so common Python layouts (`from .pricing
                // import discount`) can still carry owner-module identity.
                let source_module = import_source_module(file, import);
                for alias in &import.names {
                    let imported = alias.name.to_string();
                    imports.push(PythonImport {
                        alias: alias
                            .asname
                            .as_ref()
                            .map(|name| name.to_string())
                            .unwrap_or_else(|| imported.clone()),
                        imported,
                        source_module: source_module.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    imports
}

fn import_source_module(file: &Path, import: &ast::StmtImportFrom) -> String {
    let module = import
        .module
        .as_ref()
        .map(|module| module.to_string())
        .unwrap_or_default();
    let level = import
        .level
        .as_ref()
        .map(|level| level.to_usize())
        .unwrap_or(0);
    if level == 0 {
        return module;
    }
    let normalized = normalized_path(file);
    let mut parts = normalized.split('/').collect::<Vec<_>>();
    parts.pop();
    let package_depth = level.saturating_sub(1);
    for _ in 0..package_depth {
        if parts.pop().is_none() {
            return String::new();
        }
    }
    if !module.is_empty() {
        parts.extend(module.split('.').filter(|part| !part.is_empty()));
    }
    parts.join(".")
}

fn is_parametrized(decorators: &[Expr]) -> bool {
    decorator_names(decorators).iter().any(|decorator| {
        decorator == "parametrize"
            || decorator.ends_with(".parametrize")
            || decorator.ends_with("mark.parametrize")
    })
}

fn is_unittest_class(class: &ast::StmtClassDef) -> bool {
    class.bases.iter().any(|base| {
        expr_full_name(base).is_some_and(|name| name == "TestCase" || name.ends_with(".TestCase"))
    })
}

/// Which same-scope classes pytest or unittest collects (#4562).
///
/// A class is collected on its own when it reaches `unittest.TestCase`
/// through same-scope bases, or when pytest collects it: a `Test*` name, and
/// neither it nor a same-scope ancestor defines `__init__`/`__new__` or is a
/// dataclass. A same-scope ancestor of a collected class that is not itself
/// collected is a mixin; each of its `test*` members runs as a test of every
/// collected subclass that resolves that member to the mixin along its C3
/// method resolution order, and is recorded under that subclass
/// (`TestConsumer.test_shared`), the selector the runner accepts. An imported base, a base defined after the subclass, or
/// a redefined name is external: reaching it first makes the resolution
/// unknown, so the member is not collected. Only the last definition of a
/// class name is collected, as the loaders see the module attribute.
#[derive(Default)]
struct LocalTestClasses<'a> {
    /// Classes unittest collects.
    unittest: BTreeSet<&'a str>,
    /// `Test*` classes pytest collects.
    pytest: BTreeSet<&'a str>,
    /// Per collected class, in its method resolution order, each mixin and
    /// the `test*` members the class resolves to that mixin.
    inherited: BTreeMap<&'a str, Vec<(&'a ast::StmtClassDef, BTreeSet<&'a str>)>>,
    last_definitions: BTreeSet<*const ast::StmtClassDef>,
    /// Marks on known local ancestors can control an overridden test too.
    class_decorators: BTreeMap<&'a str, Vec<String>>,
    /// Argnames a class-level (or ancestor) pytest parametrize supplies.
    class_direct_parametrized: BTreeMap<&'a str, Vec<String>>,
    class_unsettled_indirect: BTreeMap<&'a str, Vec<String>>,
    class_parametrize_argnames: BTreeMap<&'a str, Vec<String>>,
    class_activation_controls:
        BTreeMap<&'a str, Vec<super::test_activation::PythonActivationControl>>,
    definition_imports: BTreeMap<&'a str, Vec<PythonImport>>,
}

impl<'a> LocalTestClasses<'a> {
    fn of(
        file: &Path,
        statements: &'a [Stmt],
        initial_imports: &[PythonImport],
        parametrize_scope: ParametrizeScope<'_>,
    ) -> Self {
        let scope = ScopeClasses::of(statements);
        let mut found = Self {
            last_definitions: scope
                .defs
                .values()
                .map(|(_, class)| *class as *const _)
                .collect(),
            ..Self::default()
        };
        let mut prefix = initial_imports.to_vec();
        for stmt in statements {
            apply_definition_import(file, stmt, &mut prefix);
            if let Stmt::ClassDef(class) = stmt
                && found.is_last_definition(class)
            {
                found
                    .definition_imports
                    .insert(class.name.as_str(), prefix.clone());
            }
        }
        let mut own_unittest: BTreeSet<&'a str> = BTreeSet::new();
        let mut changed = true;
        while changed {
            changed = false;
            for (name, (_, class)) in &scope.defs {
                if !own_unittest.contains(name)
                    && (is_unittest_class(class)
                        || scope
                            .local_bases(name)
                            .iter()
                            .any(|base| own_unittest.contains(base)))
                {
                    own_unittest.insert(name);
                    changed = true;
                }
            }
        }
        for name in scope.defs.keys() {
            if name.starts_with("Test")
                && !own_unittest.contains(name)
                && !scope.ancestors_or_self(name).iter().any(|ancestor| {
                    scope
                        .defs
                        .get(ancestor)
                        .is_some_and(|(_, class)| blocks_pytest_collection(class))
                })
            {
                found.pytest.insert(name);
            }
        }
        let collected: BTreeSet<&'a str> = own_unittest.union(&found.pytest).copied().collect();
        for name in &collected {
            let decorators = scope
                .ancestors_or_self(name)
                .iter()
                .filter_map(|ancestor| scope.defs.get(ancestor))
                .flat_map(|(_, class)| decorator_names(&class.decorator_list))
                .collect();
            found.class_decorators.insert(name, decorators);
            // Each class decorator reads the imports in effect where its
            // class is defined.
            let class_scope = |ancestor: &&'a str| ParametrizeScope {
                imports: found
                    .definition_imports
                    .get(ancestor)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                ..parametrize_scope
            };
            let direct_parametrized = scope
                .ancestors_or_self(name)
                .iter()
                .filter_map(|ancestor| Some((scope.defs.get(ancestor)?, class_scope(ancestor))))
                .flat_map(|((_, class), within)| {
                    super::parametrize::direct_argnames(&class.decorator_list, within)
                })
                .collect();
            let unsettled_indirect = scope
                .ancestors_or_self(name)
                .iter()
                .filter_map(|ancestor| Some((scope.defs.get(ancestor)?, class_scope(ancestor))))
                .flat_map(|((_, class), within)| {
                    super::parametrize::unsettled_indirect_argnames(&class.decorator_list, within)
                })
                .collect();
            let parametrize_argnames: Vec<String> = scope
                .ancestors_or_self(name)
                .iter()
                .filter_map(|ancestor| Some((scope.defs.get(ancestor)?, class_scope(ancestor))))
                .flat_map(|((_, class), within)| {
                    super::parametrize::parametrize_argnames(&class.decorator_list, within)
                })
                .collect();
            let class_collection_error =
                super::parametrize::has_duplicate(parametrize_argnames.iter().map(String::as_str))
                    || scope
                        .ancestors_or_self(name)
                        .iter()
                        .filter_map(|ancestor| {
                            Some((scope.defs.get(ancestor)?, class_scope(ancestor)))
                        })
                        .any(|((_, class), within)| {
                            super::parametrize::collection_error(&class.decorator_list, within)
                        });
            found
                .class_parametrize_argnames
                .insert(name, parametrize_argnames);
            found
                .class_direct_parametrized
                .insert(name, direct_parametrized);
            found
                .class_unsettled_indirect
                .insert(name, unsettled_indirect);
            let controls = scope
                .ancestors_or_self(name)
                .iter()
                .filter_map(|ancestor| {
                    scope
                        .defs
                        .get(ancestor)
                        .zip(found.definition_imports.get(ancestor))
                })
                .flat_map(|((_, class), imports)| {
                    super::test_activation::activation_controls(
                        &class.decorator_list,
                        imports,
                        super::test_activation::DeclarationSite::Class,
                    )
                })
                .chain(class_collection_error.then(|| {
                    super::test_activation::parametrize_collection_error_control(
                        super::test_activation::DeclarationSite::Class,
                    )
                }))
                .chain(class_switched_off(&scope, name).then(|| {
                    super::test_activation::test_switch_control(
                        super::test_activation::DeclarationSite::Class,
                    )
                }))
                .collect();
            found.class_activation_controls.insert(name, controls);
            let Some(mro) = scope.mro(name, &mut Vec::new()) else {
                continue;
            };
            let mut inherited = Vec::new();
            for entry in &mro {
                let MroEntry::Local(mixin) = entry else {
                    continue;
                };
                if collected.contains(mixin) {
                    continue;
                }
                let Some((_, class)) = scope.defs.get(mixin) else {
                    continue;
                };
                let members: BTreeSet<&'a str> = test_member_names(class)
                    .into_iter()
                    .filter(|member| scope.resolve_member(&mro, member) == Some(*mixin))
                    .collect();
                if !members.is_empty() {
                    inherited.push((*class, members));
                }
            }
            if !inherited.is_empty() {
                found.inherited.insert(name, inherited);
            }
        }
        found.unittest = own_unittest;
        found
    }

    /// Whether `class` is the last definition of its name in this scope, the
    /// one pytest and unittest see as the module attribute.
    fn is_last_definition(&self, class: &ast::StmtClassDef) -> bool {
        self.last_definitions.contains(&(class as *const _))
    }
}

/// The classes of one scope, by the last definition of each name.
struct ScopeClasses<'a> {
    /// Name -> (statement index, last definition).
    defs: BTreeMap<&'a str, (usize, &'a ast::StmtClassDef)>,
    /// Names defined more than once.
    redefined: BTreeSet<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MroEntry<'a> {
    Local(&'a str),
    /// A base this scope does not define before the class (an import, a
    /// forward reference, a redefined name): its members are unknown.
    External(String),
}

impl<'a> ScopeClasses<'a> {
    fn of(statements: &'a [Stmt]) -> Self {
        let mut defs = BTreeMap::new();
        let mut redefined = BTreeSet::new();
        for (index, stmt) in statements.iter().enumerate() {
            if let Stmt::ClassDef(class) = stmt
                && defs.insert(class.name.as_str(), (index, class)).is_some()
            {
                redefined.insert(class.name.as_str());
            }
        }
        Self { defs, redefined }
    }

    /// The bases of `name` in order, local when the scope defines the base
    /// exactly once and before `name`. `object` and `TestCase` define no
    /// `test*` member and are left out.
    fn bases(&self, name: &str) -> Vec<MroEntry<'a>> {
        let Some((index, class)) = self.defs.get(name) else {
            return Vec::new();
        };
        class
            .bases
            .iter()
            .filter_map(expr_full_name)
            .filter(|base| !matches!(base.as_str(), "object" | "TestCase" | "unittest.TestCase"))
            .map(|base| match self.defs.get_key_value(base.as_str()) {
                Some((&key, (base_index, _)))
                    if base_index < index && !self.redefined.contains(key) =>
                {
                    MroEntry::Local(key)
                }
                _ => MroEntry::External(base),
            })
            .collect()
    }

    fn local_bases(&self, name: &str) -> Vec<&'a str> {
        self.bases(name)
            .into_iter()
            .filter_map(|base| match base {
                MroEntry::Local(key) => Some(key),
                MroEntry::External(_) => None,
            })
            .collect()
    }

    /// `name` and its local ancestors, each once.
    fn ancestors_or_self(&self, name: &'a str) -> Vec<&'a str> {
        let mut seen: Vec<&'a str> = Vec::new();
        let mut stack = vec![name];
        while let Some(current) = stack.pop() {
            if !seen.contains(&current) {
                seen.push(current);
                stack.extend(self.local_bases(current));
            }
        }
        seen
    }

    /// Python's C3 linearization of `name`, or None when it does not exist.
    fn mro(&self, name: &'a str, visiting: &mut Vec<&'a str>) -> Option<Vec<MroEntry<'a>>> {
        if visiting.contains(&name) {
            return None;
        }
        visiting.push(name);
        let bases = self.bases(name);
        let mut sequences: Vec<Vec<MroEntry<'a>>> = Vec::new();
        for base in &bases {
            sequences.push(match base {
                MroEntry::Local(key) => self.mro(key, visiting)?,
                MroEntry::External(_) => vec![base.clone()],
            });
        }
        sequences.push(bases);
        visiting.pop();
        let mut linear = vec![MroEntry::Local(name)];
        loop {
            sequences.retain(|sequence| !sequence.is_empty());
            if sequences.is_empty() {
                return Some(linear);
            }
            let head = sequences
                .iter()
                .map(|sequence| &sequence[0])
                .find(|candidate| {
                    !sequences
                        .iter()
                        .any(|sequence| sequence[1..].contains(candidate))
                })?
                .clone();
            for sequence in &mut sequences {
                if sequence[0] == head {
                    sequence.remove(0);
                }
            }
            linear.push(head);
        }
    }

    /// The class `member` resolves to along `mro`, or None when it is
    /// missing or an external base comes first.
    fn resolve_member(&self, mro: &[MroEntry<'a>], member: &str) -> Option<&'a str> {
        for entry in mro {
            match entry {
                MroEntry::Local(key) => {
                    if self
                        .defs
                        .get(key)
                        .is_some_and(|(_, class)| test_member_names(class).contains(member))
                    {
                        return Some(key);
                    }
                }
                MroEntry::External(_) => return None,
            }
        }
        None
    }
}

/// Whether `__test__ = False` (or an unevaluable binding) reaches `name`:
/// pytest reads the attribute through the class hierarchy, so the first class
/// along the MRO that binds it decides, and an intermediate literal `True`
/// re-enables collection for its subclasses. When the MRO is unavailable or an
/// external base precedes any binding, any ancestor that is not provably
/// `True` counts (fail-closed).
fn class_switched_off<'a>(scope: &ScopeClasses<'a>, name: &'a str) -> bool {
    use super::test_activation::{TestSwitch, test_switch};
    let switch_of = |key: &str| {
        scope
            .defs
            .get(key)
            .map(|(_, class)| test_switch(&class.body))
    };
    if let Some(mro) = scope.mro(name, &mut Vec::new()) {
        for entry in &mro {
            let MroEntry::Local(key) = entry else {
                break;
            };
            match switch_of(key) {
                Some(TestSwitch::Enabled) => return false,
                Some(TestSwitch::Disabled) => return true,
                _ => {}
            }
        }
    }
    let switches: Vec<TestSwitch> = scope
        .ancestors_or_self(name)
        .iter()
        .filter_map(|ancestor| switch_of(ancestor))
        .collect();
    match switches.first() {
        Some(TestSwitch::Enabled) => false,
        _ => switches.contains(&TestSwitch::Disabled),
    }
}

/// A class pytest will not collect as a test class: it defines `__init__` or
/// `__new__`, or a dataclass decorator generates `__init__`.
fn blocks_pytest_collection(class: &ast::StmtClassDef) -> bool {
    class.body.iter().any(|stmt| {
        matches!(
            stmt,
            Stmt::FunctionDef(function) if matches!(function.name.as_str(), "__init__" | "__new__")
        ) || matches!(
            stmt,
            Stmt::AsyncFunctionDef(function) if matches!(function.name.as_str(), "__init__" | "__new__")
        )
    }) || class.decorator_list.iter().any(|decorator| {
        let callee = match decorator {
            Expr::Call(call) => call.func.as_ref(),
            other => other,
        };
        expr_full_name(callee).is_some_and(|name| {
            name == "dataclass"
                || name.ends_with(".dataclass")
                || matches!(name.as_str(), "attr.s" | "attr.attrs" | "attrs.define" | "attrs.frozen" | "define" | "frozen")
        })
    })
}

/// The `test*` names a class body binds: methods, and class-level
/// assignments that override an inherited test (`test_x = None`).
fn test_member_names(class: &ast::StmtClassDef) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    for stmt in &class.body {
        match stmt {
            Stmt::FunctionDef(function) => {
                names.insert(function.name.as_str());
            }
            Stmt::AsyncFunctionDef(function) => {
                names.insert(function.name.as_str());
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    if let Expr::Name(name) = target {
                        names.insert(name.id.as_str());
                    }
                }
            }
            Stmt::AnnAssign(assign) => {
                if let Expr::Name(name) = assign.target.as_ref() {
                    names.insert(name.id.as_str());
                }
            }
            _ => {}
        }
    }
    names.retain(|name| name.starts_with("test"));
    names
}

/// Names of decorators written as a call, whose result (not the named
/// function) wraps the owner.
fn called_decorator_names(decorators: &[Expr]) -> Vec<String> {
    decorators
        .iter()
        .filter(|decorator| matches!(decorator, Expr::Call(_)))
        .filter_map(expr_full_name)
        .collect()
}

pub(super) fn decorator_names(decorators: &[Expr]) -> Vec<String> {
    decorators.iter().filter_map(expr_full_name).collect()
}

fn collect_static_route_paths(source: &str, decorators: &[Expr]) -> Vec<String> {
    decorators
        .iter()
        .filter_map(|decorator| {
            let name = expr_full_name(decorator)?;
            if !is_static_route_decorator(&name) {
                return None;
            }
            route_decorator_literal_argument(source, decorator, &name)
        })
        .collect()
}

/// The receiver each static route decorator hangs off (`app` in `@app.get`),
/// index-aligned with `collect_static_route_paths`.
fn collect_static_route_receivers(source: &str, decorators: &[Expr]) -> Vec<String> {
    decorators
        .iter()
        .filter_map(|decorator| {
            let name = expr_full_name(decorator)?;
            if !is_static_route_decorator(&name) {
                return None;
            }
            route_decorator_literal_argument(source, decorator, &name)?;
            Some(
                name.rsplit_once('.')
                    .map(|(receiver, _)| receiver.to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

/// The HTTP verbs each static route decorator accepts, index-aligned with
/// `collect_static_route_paths`. A verb decorator (`@app.post`) names its own
/// verb; a GET route also serves HEAD. `route` and `api_route` name verbs only
/// through a literal `methods=[...]`; without one, or with a non-literal one,
/// the entry is empty and the verb is unknown rather than guessed.
fn collect_static_route_methods(source: &str, decorators: &[Expr]) -> Vec<Vec<String>> {
    decorators
        .iter()
        .filter_map(|decorator| {
            let name = expr_full_name(decorator)?;
            if !is_static_route_decorator(&name) {
                return None;
            }
            route_decorator_literal_argument(source, decorator, &name)?;
            let verb = name.rsplit('.').next().unwrap_or_default();
            Some(match verb {
                "get" => vec!["get".to_string(), "head".to_string()],
                "post" | "put" | "patch" | "delete" | "options" | "head" => {
                    vec![verb.to_string()]
                }
                "route" | "api_route" => route_decorator_literal_methods(decorator),
                _ => Vec::new(),
            })
        })
        .collect()
}

/// Lowercase verbs from the decorator call's own `methods=[...]` /
/// `methods=(...)` keyword, read from the parsed arguments so a `methods`
/// inside another call's keywords, a string, or a comment is never mistaken
/// for it. Empty (unknown) when the keyword is absent, when the call forwards
/// `**kwargs`, or when the value is anything but a plain list or tuple of
/// string constants.
fn route_decorator_literal_methods(decorator: &Expr) -> Vec<String> {
    let Expr::Call(call) = decorator else {
        return Vec::new();
    };
    if call.keywords.iter().any(|keyword| keyword.arg.is_none()) {
        return Vec::new();
    }
    let Some(keyword) = call.keywords.iter().find(|keyword| {
        keyword
            .arg
            .as_ref()
            .is_some_and(|arg| arg.as_str() == "methods")
    }) else {
        return Vec::new();
    };
    let elements = match &keyword.value {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return Vec::new(),
    };
    let mut verbs = Vec::new();
    for element in elements {
        match element {
            Expr::Constant(constant) => match &constant.value {
                Constant::Str(verb)
                    if !verb.is_empty() && verb.bytes().all(|byte| byte.is_ascii_alphabetic()) =>
                {
                    verbs.push(verb.to_ascii_lowercase());
                }
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        }
    }
    // A GET route also serves HEAD (Flask and Starlette add it).
    if verbs.iter().any(|verb| verb == "get") && !verbs.iter().any(|verb| verb == "head") {
        verbs.push("head".to_string());
    }
    verbs
}

fn collect_dynamic_route_decorators(source: &str, decorators: &[Expr]) -> Vec<String> {
    decorators
        .iter()
        .filter_map(|decorator| {
            let name = expr_full_name(decorator)?;
            if !is_static_route_decorator(&name) {
                return None;
            }
            route_decorator_literal_argument(source, decorator, &name)
                .is_none()
                .then_some(name)
        })
        .collect()
}

fn route_decorator_literal_argument(source: &str, decorator: &Expr, name: &str) -> Option<String> {
    let text = text_for_range(source, decorator.range());
    let after_name = text
        .strip_prefix(name)
        .or_else(|| text.find(name).and_then(|idx| text.get(idx + name.len()..)))?;
    first_parenthesized_string_argument(after_name.trim_start())
}

#[cfg(test)]
mod tests;
