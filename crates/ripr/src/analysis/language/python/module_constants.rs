//! Module-level named constants for Python predicate boundaries (#4227).
//!
//! `if amount >= DISCOUNT_THRESHOLD:` with `DISCOUNT_THRESHOLD = 10_000` at
//! module scope compares against a value static evidence can see, the way the
//! Rust (`value_resolution::named_constant`) and TypeScript adapters resolve a
//! same-file `const`. Python has no `const`, so a name counts as a constant
//! only when nothing in the module can rebind it:
//!
//! - it is bound exactly once at module scope, by a top-level `NAME = <literal>`
//!   or `NAME: T = <literal>` statement;
//! - no other module-scope statement binds it (assignment, augmented
//!   assignment, `for`/`with`/`except` target, import, `def`/`class`, `del`);
//! - no function or class body declares it `global`, and no walrus (`:=`)
//!   anywhere in the module targets it;
//! - the module has no star import, no module-scope `match`, and no
//!   `globals()` call, any of which can bind names static evidence cannot see.
//!
//! Anything else stays unresolved, so the boundary rule keeps failing closed.
//! A function owner additionally drops every constant its own scope shadows
//! (a parameter or any local binding of the same name).

use super::boundary::literal_value;
use super::source_utils::{line_for_range_start, text_for_range};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

/// A module-scope name bound once to a scalar literal and never rebound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonModuleConstant {
    pub(super) name: String,
    /// Canonical literal text (`10_000` becomes `10000`).
    pub(super) value: String,
    pub(super) line: usize,
}

/// The module-scope literal constants of one parsed module.
pub(super) fn module_literal_constants(
    source: &str,
    statements: &[Stmt],
) -> Vec<PythonModuleConstant> {
    let mut bindings = ScopeBindings::default();
    collect_scope_bindings(statements, &mut bindings);
    if bindings.opaque || source.contains("globals()") {
        return Vec::new();
    }
    let mut globals = Vec::new();
    collect_global_declarations(statements, &mut globals);
    statements
        .iter()
        .filter_map(|stmt| literal_assignment(source, stmt))
        .filter(|constant| {
            bindings.count(&constant.name) == 1
                && !globals.contains(&constant.name)
                && !walrus_targets(source, &constant.name)
        })
        .collect()
}

/// The constants still visible inside a function owner: those its parameters
/// and local bindings do not shadow. `body_text` is the function's source,
/// used only to catch a walrus binding inside it.
pub(super) fn constants_visible_in_function(
    constants: &[PythonModuleConstant],
    args: &ast::Arguments,
    body: &[Stmt],
    body_text: &str,
) -> Vec<PythonModuleConstant> {
    if constants.is_empty() {
        return Vec::new();
    }
    let mut locals = ScopeBindings::default();
    collect_scope_bindings(body, &mut locals);
    if locals.opaque {
        return Vec::new();
    }
    locals.names.extend(
        args.posonlyargs
            .iter()
            .chain(&args.args)
            .chain(&args.kwonlyargs)
            .map(|arg| arg.def.arg.to_string())
            .chain(args.vararg.iter().map(|arg| arg.arg.to_string()))
            .chain(args.kwarg.iter().map(|arg| arg.arg.to_string())),
    );
    constants
        .iter()
        .filter(|constant| {
            locals.count(&constant.name) == 0 && !walrus_targets(body_text, &constant.name)
        })
        .cloned()
        .collect()
}

fn literal_assignment(source: &str, stmt: &Stmt) -> Option<PythonModuleConstant> {
    let (target, value) = match stmt {
        Stmt::Assign(assign) => match assign.targets.as_slice() {
            [target] => (target, assign.value.as_ref()),
            _ => return None,
        },
        Stmt::AnnAssign(assign) if assign.simple => {
            (assign.target.as_ref(), assign.value.as_deref()?)
        }
        _ => return None,
    };
    let Expr::Name(name) = target else {
        return None;
    };
    Some(PythonModuleConstant {
        name: name.id.to_string(),
        value: literal_value(&text_for_range(source, value.range()))?,
        line: line_for_range_start(source, stmt.range()),
    })
}

/// Every binding of a name in one scope, counted per occurrence. `opaque`
/// marks a construct that can bind names this walk does not enumerate.
#[derive(Default)]
struct ScopeBindings {
    names: Vec<String>,
    opaque: bool,
}

impl ScopeBindings {
    fn count(&self, name: &str) -> usize {
        self.names.iter().filter(|bound| *bound == name).count()
    }
}

/// Names bound in the scope that owns `statements`. Nested `def` and `class`
/// bind their own name here but open a new scope, so their bodies are not
/// walked; compound statements (`if`, `for`, `try`, `with`) share the scope.
fn collect_scope_bindings(statements: &[Stmt], out: &mut ScopeBindings) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => out.names.push(function.name.to_string()),
            Stmt::AsyncFunctionDef(function) => out.names.push(function.name.to_string()),
            Stmt::ClassDef(class) => out.names.push(class.name.to_string()),
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    collect_target_names(target, out);
                }
            }
            Stmt::AugAssign(assign) => collect_target_names(&assign.target, out),
            Stmt::AnnAssign(assign) => collect_target_names(&assign.target, out),
            Stmt::TypeAlias(alias) => collect_target_names(&alias.name, out),
            Stmt::Delete(delete) => {
                for target in &delete.targets {
                    collect_target_names(target, out);
                }
            }
            Stmt::For(stmt) => {
                collect_target_names(&stmt.target, out);
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::AsyncFor(stmt) => {
                collect_target_names(&stmt.target, out);
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::While(stmt) => {
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::If(stmt) => {
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::With(stmt) => {
                collect_with_targets(&stmt.items, out);
                collect_scope_bindings(&stmt.body, out);
            }
            Stmt::AsyncWith(stmt) => {
                collect_with_targets(&stmt.items, out);
                collect_scope_bindings(&stmt.body, out);
            }
            Stmt::Try(stmt) => collect_try_bindings(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                out,
            ),
            Stmt::TryStar(stmt) => collect_try_bindings(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                out,
            ),
            Stmt::Import(import) => {
                for alias in &import.names {
                    let bound = alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_string(),
                        ToString::to_string,
                    );
                    out.names.push(bound);
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        out.opaque = true;
                    }
                    out.names
                        .push(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            // Case patterns capture names in forms this walk does not
            // enumerate, so a `match` in scope resolves nothing.
            Stmt::Match(_) => out.opaque = true,
            _ => {}
        }
    }
}

fn collect_try_bindings(
    body: &[Stmt],
    handlers: &[ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    out: &mut ScopeBindings,
) {
    collect_scope_bindings(body, out);
    for ast::ExceptHandler::ExceptHandler(handler) in handlers {
        if let Some(name) = &handler.name {
            out.names.push(name.to_string());
        }
        collect_scope_bindings(&handler.body, out);
    }
    collect_scope_bindings(orelse, out);
    collect_scope_bindings(finalbody, out);
}

fn collect_with_targets(items: &[ast::WithItem], out: &mut ScopeBindings) {
    for item in items {
        if let Some(target) = &item.optional_vars {
            collect_target_names(target, out);
        }
    }
}

/// Names an assignment target binds: a bare name, or the names inside a
/// tuple, list or starred unpacking. Attribute and subscript targets bind no
/// name in this scope.
fn collect_target_names(target: &Expr, out: &mut ScopeBindings) {
    match target {
        Expr::Name(name) => out.names.push(name.id.to_string()),
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                collect_target_names(element, out);
            }
        }
        Expr::List(list) => {
            for element in &list.elts {
                collect_target_names(element, out);
            }
        }
        Expr::Starred(starred) => collect_target_names(&starred.value, out),
        _ => {}
    }
}

/// Every name any function or class body in the module declares `global`,
/// at any nesting depth.
fn collect_global_declarations(statements: &[Stmt], out: &mut Vec<String>) {
    for stmt in statements {
        match stmt {
            Stmt::Global(global) => out.extend(global.names.iter().map(ToString::to_string)),
            Stmt::FunctionDef(function) => collect_global_declarations(&function.body, out),
            Stmt::AsyncFunctionDef(function) => collect_global_declarations(&function.body, out),
            Stmt::ClassDef(class) => collect_global_declarations(&class.body, out),
            Stmt::For(stmt) => {
                collect_global_declarations(&stmt.body, out);
                collect_global_declarations(&stmt.orelse, out);
            }
            Stmt::AsyncFor(stmt) => {
                collect_global_declarations(&stmt.body, out);
                collect_global_declarations(&stmt.orelse, out);
            }
            Stmt::While(stmt) => {
                collect_global_declarations(&stmt.body, out);
                collect_global_declarations(&stmt.orelse, out);
            }
            Stmt::If(stmt) => {
                collect_global_declarations(&stmt.body, out);
                collect_global_declarations(&stmt.orelse, out);
            }
            Stmt::With(stmt) => collect_global_declarations(&stmt.body, out),
            Stmt::AsyncWith(stmt) => collect_global_declarations(&stmt.body, out),
            Stmt::Try(stmt) => {
                collect_global_declarations(&stmt.body, out);
                for ast::ExceptHandler::ExceptHandler(handler) in &stmt.handlers {
                    collect_global_declarations(&handler.body, out);
                }
                collect_global_declarations(&stmt.orelse, out);
                collect_global_declarations(&stmt.finalbody, out);
            }
            Stmt::TryStar(stmt) => {
                collect_global_declarations(&stmt.body, out);
                for ast::ExceptHandler::ExceptHandler(handler) in &stmt.handlers {
                    collect_global_declarations(&handler.body, out);
                }
                collect_global_declarations(&stmt.orelse, out);
                collect_global_declarations(&stmt.finalbody, out);
            }
            Stmt::Match(stmt) => {
                for case in &stmt.cases {
                    collect_global_declarations(&case.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Whether `text` holds a walrus (`NAME :=`) targeting `name`. Textual on
/// purpose: an assignment expression can sit inside any expression, and a
/// false match only leaves the constant unresolved.
fn walrus_targets(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(start, _)| {
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
        before_ok && text[start + name.len()..].trim_start().starts_with(":=")
    })
}

#[cfg(test)]
mod tests;
