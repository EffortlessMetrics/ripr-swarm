//! Exported factories that build an instance of a getter owner's own class.
//!
//! A test cannot construct a class the module only exposes through a factory,
//! and it often reads the changed getter on the value the factory returned:
//!
//! ```ts
//! export class Cached<T> {
//!   get value(): T { /* changed */ }
//! }
//! export function cached<T>(getter: () => T): Cached<T> {
//!   return new Cached(getter);
//! }
//! ```
//!
//! A test with `const box = cached(() => 1); expect(box.value).toBe(1)` runs
//! the getter. This module records, on each getter owner, the exported
//! factories that return a fresh instance of the owner's class, so the
//! relation layer can treat a receiver bound to such a factory call like a
//! receiver bound to `new Class(...)` (RIPR-SPEC-0243 decision 23, #10402). The analysis is
//! syntax-only, same-file and fail-closed. A factory qualifies only when:
//!
//! - it is the only top-level declaration of its name: a plain (not async, not
//!   generator) function declaration that is never assigned, or a `const`
//!   binding of a plain arrow or function expression;
//! - it is exported (directly, by `export { local as exported }`, or as the
//!   default export);
//! - every `return` the function itself runs (nested functions and classes
//!   are not entered) is `new Class(...)` of the owner's class, and the last
//!   top-level statement is a `return` or `throw`, so no path yields
//!   something else;
//! - its parameters never mention the class name and its body mentions the
//!   class name only as `new Class`, so nothing shadows or aliases the class.
//!
//! The owner's class must be declared once in the file and never reassigned,
//! declare the getter once, carry no instance field, `this.<getter> =`
//! assignment, `defineProperty` or `Class.prototype` edit that could replace
//! the getter on the instance, have no constructor that returns a value, and
//! the file must not mention `__proto__`. A class that extends another is
//! admitted only when every ancestor is a plain same-file class that passes the
//! same checks (an imported, called or member base refuses, because its
//! constructor could return another object or define the member on `this`).

use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum factories recorded per owner. Dropping entries only withholds
/// reach, so the bound is fail-closed.
const INSTANCE_FACTORY_MAX_PER_OWNER: usize = 16;

/// A top-level binding the module declares, with the class its value builds
/// when it is a qualifying factory.
struct FactoryDeclaration<'a> {
    local: &'a str,
    class_name: Option<String>,
}

/// Record, on each getter owner, the exported factories of its class.
pub(crate) fn assign_instance_factories(
    program: &oxc_ast::ast::Program<'_>,
    source: &SourceText<'_>,
    owners: &mut [TypeScriptOwner],
    instance_fields: &BTreeMap<String, BTreeSet<String>>,
    repeated_names: &BTreeSet<String>,
) {
    let wanted: BTreeSet<&str> = owners
        .iter()
        .filter(|owner| is_getter_owner(owner))
        .filter_map(|owner| owner.class_name.as_deref())
        .filter(|class_name| !repeated_names.contains(*class_name))
        .collect();
    if wanted.is_empty() {
        return;
    }
    let text = source.text();
    let mut declarations: Vec<FactoryDeclaration<'_>> = Vec::new();
    let mut declared_counts: BTreeMap<&str, usize> = BTreeMap::new();
    // `local -> exported names` and whether an export named the default.
    let mut exports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for statement in &program.body {
        collect_statement(
            statement,
            text,
            &wanted,
            &mut declarations,
            &mut declared_counts,
            &mut exports,
        );
    }
    let class_member_sources = class_member_sources(owners);
    let (supers, decorated_classes) = class_supers(program);
    let facts = ClassFacts {
        file_text: text,
        instance_fields,
        member_sources: &class_member_sources,
        supers: &supers,
        decorated_classes: &decorated_classes,
        repeated_names,
    };
    let mut additions: Vec<(usize, Vec<TypeScriptModuleEntry>)> = Vec::new();
    for (index, owner) in owners.iter().enumerate() {
        if !is_getter_owner(owner) {
            continue;
        }
        let Some(class_name) = owner.class_name.as_deref() else {
            continue;
        };
        if repeated_names.contains(class_name)
            || !is_safe_javascript_identifier(&owner.name)
            || text.contains("__proto__")
            || super::tests_extract::identifier_written_strictly(text, class_name)
            || owner.decorated
            || owners
                .iter()
                .filter(|other| {
                    other.file == owner.file
                        && other.class_name.as_deref() == Some(class_name)
                        && other.name == owner.name
                        && other.method_kind == TypeScriptMethodKind::Getter
                })
                .count()
                != 1
            || !class_chain_cannot_replace_getter(class_name, &owner.name, &owner.file, &facts)
        {
            continue;
        }
        let mut entries: Vec<TypeScriptModuleEntry> = Vec::new();
        for declaration in &declarations {
            if declaration.class_name.as_deref() != Some(class_name)
                || declared_counts.get(declaration.local) != Some(&1)
            {
                continue;
            }
            for exported in exports.get(declaration.local).into_iter().flatten() {
                entries.push(TypeScriptModuleEntry {
                    name: exported.clone(),
                    exported_as_default: exported == "default",
                    class_member: None,
                    decorated: None,
                    instance_factory: true,
                });
            }
        }
        entries.sort();
        entries.dedup();
        entries.truncate(INSTANCE_FACTORY_MAX_PER_OWNER);
        if !entries.is_empty() {
            additions.push((index, entries));
        }
    }
    for (index, entries) in additions {
        owners[index].module_entries.extend(entries);
    }
}

/// `class name -> extends`: `None` for a class with no `extends` clause,
/// `Some(base)` for `extends Base`, `Some("")` for any other heritage
/// expression (a call, a member, a mixin).
fn class_supers(
    program: &oxc_ast::ast::Program<'_>,
) -> (BTreeMap<String, Option<String>>, BTreeSet<String>) {
    let mut out = BTreeMap::new();
    let mut decorated = BTreeSet::new();
    let mut visit = |class: &Class<'_>| {
        let Some(id) = class.id.as_ref() else {
            return;
        };
        // A class decorator can return a replacement class.
        if !class.decorators.is_empty() {
            decorated.insert(id.name.as_str().to_string());
        }
        let base = class
            .super_class
            .as_ref()
            .map(|base| match unwrap_type_wrappers(base) {
                Expression::Identifier(ident) => ident.name.as_str().to_string(),
                _ => String::new(),
            });
        out.insert(id.name.as_str().to_string(), base);
    };
    for statement in &program.body {
        match statement {
            Statement::ClassDeclaration(class) => visit(class),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = export.declaration.as_ref() {
                    visit(class);
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    visit(class);
                }
            }
            _ => {}
        }
    }
    (out, decorated)
}

type MemberSources<'a> = BTreeMap<(PathBuf, String), Vec<(TypeScriptMethodKind, &'a str)>>;

/// File-wide class facts the getter-replacement gate reads.
struct ClassFacts<'a> {
    file_text: &'a str,
    instance_fields: &'a BTreeMap<String, BTreeSet<String>>,
    member_sources: &'a MemberSources<'a>,
    supers: &'a BTreeMap<String, Option<String>>,
    decorated_classes: &'a BTreeSet<String>,
    repeated_names: &'a BTreeSet<String>,
}

/// The class and every same-file ancestor it extends must be unable to
/// replace the getter on an instance. An ancestor that is not a plain
/// same-file class (imported, a call, a member) refuses: its constructor could
/// return another object or define the member on `this`.
fn class_chain_cannot_replace_getter(
    class_name: &str,
    name: &str,
    file: &Path,
    facts: &ClassFacts<'_>,
) -> bool {
    let mut current = class_name.to_string();
    for _ in 0..MAX_CLASS_CHAIN {
        let sources = facts
            .member_sources
            .get(&(file.to_path_buf(), current.clone()));
        if facts.repeated_names.contains(&current)
            || facts.decorated_classes.contains(&current)
            || !class_cannot_replace_getter(
                &current,
                name,
                facts.file_text,
                facts.instance_fields,
                sources,
            )
        {
            return false;
        }
        match facts.supers.get(&current) {
            Some(None) => return true,
            Some(Some(base)) if !base.is_empty() => current = base.clone(),
            _ => return false,
        }
    }
    false
}

/// Longest `extends` chain followed; a longer one refuses.
const MAX_CLASS_CHAIN: usize = 4;

fn is_getter_owner(owner: &TypeScriptOwner) -> bool {
    owner.owner_kind == OwnerKind::Method
        && owner.method_kind == TypeScriptMethodKind::Getter
        && owner.class_name.is_some()
}

/// Source text of every method of each `(file, class)`, so an instance-level
/// rebinding anywhere in the class shadows the getter.
fn class_member_sources(owners: &[TypeScriptOwner]) -> MemberSources<'_> {
    let mut out: MemberSources<'_> = BTreeMap::new();
    for owner in owners {
        if owner.owner_kind == OwnerKind::Method
            && let Some(class_name) = owner.class_name.as_ref()
            && let Some(source) = owner.source_text.as_deref()
        {
            out.entry((owner.file.clone(), class_name.clone()))
                .or_default()
                .push((owner.method_kind, source));
        }
    }
    out
}

/// `false` when something can put a different `name` on an instance of
/// `class_name`: an instance field of that name (or a computed one), a
/// `this.name = ...` / `defineProperty(this` in a class method, a
/// `defineProperty` call naming it, or any `Class.prototype` mention.
fn class_cannot_replace_getter(
    class_name: &str,
    name: &str,
    file_text: &str,
    instance_fields: &BTreeMap<String, BTreeSet<String>>,
    member_sources: Option<&Vec<(TypeScriptMethodKind, &str)>>,
) -> bool {
    if instance_fields
        .get(class_name)
        .is_some_and(|fields| fields.contains(name) || fields.contains("*"))
    {
        return false;
    }
    if member_sources.is_some_and(|sources| {
        sources.iter().any(|(kind, source)| {
            rebinds_this_member(source, name)
                // A constructor that returns a value hands back another object.
                || (*kind == TypeScriptMethodKind::Constructor
                    && word_offsets(source, "return").next().is_some())
        })
    }) {
        return false;
    }
    if file_text.contains(&format!("{class_name}.prototype"))
        || file_text.contains("setPrototypeOf")
    {
        return false;
    }
    !["defineProperty(", "defineProperties("].iter().any(|call| {
        file_text.match_indices(call).any(|(idx, _)| {
            let window: String = file_text[idx..].chars().take(240).collect();
            ['\'', '"', '`']
                .iter()
                .any(|quote| window.contains(&format!("{quote}{name}{quote}")))
                || window.contains(&format!("{{ {name}:"))
        })
    })
}

fn collect_statement<'a>(
    statement: &'a Statement<'a>,
    text: &str,
    wanted: &BTreeSet<&str>,
    declarations: &mut Vec<FactoryDeclaration<'a>>,
    declared_counts: &mut BTreeMap<&'a str, usize>,
    exports: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match statement {
        Statement::FunctionDeclaration(func) => {
            collect_function(func, text, wanted, declarations, declared_counts);
        }
        Statement::VariableDeclaration(decl) => {
            collect_variables(decl, text, wanted, declarations, declared_counts);
        }
        Statement::ClassDeclaration(class) => count_class(class, declared_counts),
        Statement::ExportNamedDeclaration(export) => {
            if export.export_kind == ImportOrExportKind::Type {
                return;
            }
            match export.declaration.as_ref() {
                Some(Declaration::FunctionDeclaration(func)) => {
                    if let Some(name) =
                        collect_function(func, text, wanted, declarations, declared_counts)
                    {
                        record_export(exports, name, name);
                    }
                }
                Some(Declaration::VariableDeclaration(decl)) => {
                    for name in collect_variables(decl, text, wanted, declarations, declared_counts)
                    {
                        record_export(exports, name, name);
                    }
                }
                Some(Declaration::ClassDeclaration(class)) => count_class(class, declared_counts),
                Some(_) => {}
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
                if let Some(id) = func.id.as_ref() {
                    let name = id.name.as_str();
                    if collect_function(func, text, wanted, declarations, declared_counts).is_some()
                    {
                        record_export(exports, name, "default");
                    }
                }
            }
            ExportDefaultDeclarationKind::Identifier(ident) => {
                record_export(exports, ident.name.as_str(), "default");
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                count_class(class, declared_counts);
            }
            _ => {}
        },
        _ => {}
    }
}

fn count_class<'a>(class: &'a Class<'a>, declared_counts: &mut BTreeMap<&'a str, usize>) {
    if let Some(id) = class.id.as_ref() {
        *declared_counts.entry(id.name.as_str()).or_default() += 1;
    }
}

fn record_export(exports: &mut BTreeMap<String, BTreeSet<String>>, local: &str, exported: &str) {
    exports
        .entry(local.to_string())
        .or_default()
        .insert(exported.to_string());
}

/// Record a function declaration; returns its name when it is a qualifying
/// factory (overload signatures carry no body and never qualify).
fn collect_function<'a>(
    func: &'a Function<'a>,
    text: &str,
    wanted: &BTreeSet<&str>,
    declarations: &mut Vec<FactoryDeclaration<'a>>,
    declared_counts: &mut BTreeMap<&'a str, usize>,
) -> Option<&'a str> {
    let name = func.id.as_ref()?.name.as_str();
    *declared_counts.entry(name).or_default() += 1;
    let class_name = function_factory_class(func, text, wanted)
        // A reassigned function declaration is some other value at the call.
        .filter(|_| !super::tests_extract::identifier_written_strictly(text, name));
    let qualifies = class_name.is_some();
    declarations.push(FactoryDeclaration {
        local: name,
        class_name,
    });
    qualifies.then_some(name)
}

/// Record the `const` declarators this module can follow and count every other
/// declarator so a duplicate declaration disqualifies the name.
fn collect_variables<'a>(
    decl: &'a VariableDeclaration<'a>,
    text: &str,
    wanted: &BTreeSet<&str>,
    declarations: &mut Vec<FactoryDeclaration<'a>>,
    declared_counts: &mut BTreeMap<&'a str, usize>,
) -> Vec<&'a str> {
    let mut names = Vec::new();
    let is_const = decl.kind == oxc_ast::ast::VariableDeclarationKind::Const;
    for declarator in &decl.declarations {
        let Some(name) = binding_identifier_name(&declarator.id) else {
            continue;
        };
        *declared_counts.entry(name).or_default() += 1;
        let class_name = declarator
            .init
            .as_ref()
            .filter(|_| is_const)
            .and_then(|init| match unwrap_type_wrappers(init) {
                Expression::ArrowFunctionExpression(arrow) => {
                    arrow_factory_class(arrow, text, wanted)
                }
                Expression::FunctionExpression(func) => function_factory_class(func, text, wanted),
                _ => None,
            });
        if class_name.is_some() {
            names.push(name);
        }
        declarations.push(FactoryDeclaration {
            local: name,
            class_name,
        });
    }
    names
}

fn function_factory_class(
    func: &Function<'_>,
    text: &str,
    wanted: &BTreeSet<&str>,
) -> Option<String> {
    if func.r#async || func.generator {
        return None;
    }
    let body = func.body.as_ref()?;
    let mut returned = Vec::new();
    collect_returns(&body.statements, &mut returned);
    let class_name = single_class(&returned, wanted)?;
    if !ends_in_return_or_throw(&body.statements)
        || span_mentions_word(text, func.params.span, &class_name)
        || !only_new_mentions(text, body.span, &class_name)
    {
        return None;
    }
    Some(class_name)
}

fn arrow_factory_class(
    arrow: &ArrowFunctionExpression<'_>,
    text: &str,
    wanted: &BTreeSet<&str>,
) -> Option<String> {
    if arrow.r#async {
        return None;
    }
    let mut returned = Vec::new();
    if arrow.expression {
        let Some(Statement::ExpressionStatement(statement)) = arrow.body.statements.first() else {
            return None;
        };
        returned.push(new_class_name(&statement.expression));
    } else {
        collect_returns(&arrow.body.statements, &mut returned);
        if !ends_in_return_or_throw(&arrow.body.statements) {
            return None;
        }
    }
    let class_name = single_class(&returned, wanted)?;
    if span_mentions_word(text, arrow.params.span, &class_name)
        || !only_new_mentions(text, arrow.body.span, &class_name)
    {
        return None;
    }
    Some(class_name)
}

/// The one class every returned value constructs, when it is a wanted class.
fn single_class(returned: &[Option<String>], wanted: &BTreeSet<&str>) -> Option<String> {
    let first = returned.first()?.as_ref()?;
    (wanted.contains(first.as_str()) && returned.iter().all(|r| r.as_ref() == Some(first)))
        .then(|| first.clone())
}

/// `Some(Class)` for `new Class(...)` (type wrappers stripped).
fn new_class_name(expression: &Expression<'_>) -> Option<String> {
    let Expression::NewExpression(new) = unwrap_type_wrappers(expression) else {
        return None;
    };
    let Expression::Identifier(callee) = unwrap_type_wrappers(&new.callee) else {
        return None;
    };
    Some(callee.name.as_str().to_string())
}

fn ends_in_return_or_throw(statements: &[Statement<'_>]) -> bool {
    matches!(
        statements.last(),
        Some(Statement::ReturnStatement(_) | Statement::ThrowStatement(_))
    )
}

/// The class each `return` the function itself runs constructs: `None` for a
/// bare return or any other value. Nested functions and classes are not
/// entered.
fn collect_returns(statements: &[Statement<'_>], out: &mut Vec<Option<String>>) {
    for statement in statements {
        collect_statement_returns(statement, out);
    }
}

fn collect_statement_returns(statement: &Statement<'_>, out: &mut Vec<Option<String>>) {
    match statement {
        Statement::ReturnStatement(ret) => {
            out.push(ret.argument.as_ref().and_then(new_class_name));
        }
        Statement::BlockStatement(block) => collect_returns(&block.body, out),
        Statement::IfStatement(branch) => {
            collect_statement_returns(&branch.consequent, out);
            if let Some(alternate) = branch.alternate.as_ref() {
                collect_statement_returns(alternate, out);
            }
        }
        Statement::ForStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::ForInStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::ForOfStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::WhileStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::DoWhileStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::LabeledStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::WithStatement(inner) => collect_statement_returns(&inner.body, out),
        Statement::TryStatement(inner) => {
            collect_returns(&inner.block.body, out);
            if let Some(handler) = inner.handler.as_ref() {
                collect_returns(&handler.body.body, out);
            }
            if let Some(finalizer) = inner.finalizer.as_ref() {
                collect_returns(&finalizer.body, out);
            }
        }
        Statement::SwitchStatement(inner) => {
            for case in &inner.cases {
                collect_returns(&case.consequent, out);
            }
        }
        _ => {}
    }
}

fn span_text(text: &str, span: oxc_span::Span) -> &str {
    text.get(span.start as usize..span.end as usize)
        .unwrap_or_default()
}

fn word_offsets<'t>(text: &'t str, word: &'t str) -> impl Iterator<Item = usize> + 't {
    text.match_indices(word).filter_map(move |(idx, _)| {
        let before = text[..idx].chars().next_back();
        let after = text[idx + word.len()..].chars().next();
        (before.is_none_or(|ch| !is_javascript_identifier_char(ch) && ch != '.')
            && after.is_none_or(|ch| !is_javascript_identifier_char(ch)))
        .then_some(idx)
    })
}

fn span_mentions_word(text: &str, span: oxc_span::Span, word: &str) -> bool {
    word_offsets(span_text(text, span), word).next().is_some()
}

/// `true` when every mention of `word` in the span is the operand of a `new`.
fn only_new_mentions(text: &str, span: oxc_span::Span, word: &str) -> bool {
    let body = span_text(text, span);
    word_offsets(body, word).all(|idx| {
        body[..idx]
            .trim_end()
            .strip_suffix("new")
            .is_some_and(|rest| {
                rest.chars()
                    .next_back()
                    .is_none_or(|ch| !is_javascript_identifier_char(ch))
            })
    })
}
