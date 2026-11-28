//! Type-checking-only blocks (#14156).
//!
//! The body of `if TYPE_CHECKING:` never runs: `typing.TYPE_CHECKING` is
//! `False` outside a static type checker. A changed line there (an import
//! for annotations, a `TypeAlias`, a `Protocol` stub) has no runtime behavior
//! for a test to discriminate, so it yields no probe, like a comment or a
//! docstring line (#1279).
//!
//! The recognizer is closed. The test must be the bare name or one attribute
//! read on a module alias, the name must come from `typing` /
//! `typing_extensions` (or be the `TYPE_CHECKING = False` spelling that
//! avoids the import), and no statement in the file may bind either name any
//! other way (an assignment, a `def`, a parameter, a loop or `with` target,
//! an `except` name, a `:=` target, another import). Everything else keeps its probes: `if not TYPE_CHECKING:`, a compound
//! test, the `else:` branch, a body that shares the `if` line, and a file
//! that does not parse.

use super::source_facts::parse_module_result;
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use rustpython_parser::lexer::lex;
use rustpython_parser::{Mode, Tok};
use std::collections::BTreeSet;
use std::ops::RangeInclusive;
use std::path::Path;

const FLAG: &str = "TYPE_CHECKING";
const TYPING_MODULES: &[&str] = &["typing", "typing_extensions"];

/// One-based line spans of every type-checking-only block body in `source`.
/// Empty when the file has none, does not parse, or binds the flag in a way
/// this module does not read.
pub(super) fn type_checking_only_line_ranges(
    path: &Path,
    source: &str,
) -> Vec<RangeInclusive<usize>> {
    if !source.contains(FLAG) {
        return Vec::new();
    }
    let Ok(Mod::Module(module)) = parse_module_result(path, source) else {
        return Vec::new();
    };
    let mut bindings = Bindings::default();
    bindings.read(&module.body);
    if bindings.unreadable || rebinds_by_named_expression(source, &bindings) {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    collect(source, &module.body, &bindings, &mut ranges);
    ranges
}

/// Whether a named expression (`name := value`) anywhere in `source` binds
/// the flag or a typing module alias. A walrus can sit in any expression (a
/// call argument, an `if` test, a comprehension), and its target is always a
/// bare name, so the token stream finds every one without walking each
/// expression shape. The lexer drops comments and line breaks inside
/// brackets, so a target split from `:=` by them is still adjacent. A file
/// the lexer rejects counts as rebinding.
fn rebinds_by_named_expression(source: &str, bindings: &Bindings) -> bool {
    let mut previous_name: Option<String> = None;
    for token in lex(source, Mode::Module) {
        let Ok((token, _)) = token else {
            return true;
        };
        match token {
            Tok::ColonEqual => {
                if previous_name
                    .as_deref()
                    .is_some_and(|name| name == FLAG || bindings.typing_aliases.contains(name))
                {
                    return true;
                }
                previous_name = None;
            }
            Tok::Name { name } => previous_name = Some(name),
            _ => previous_name = None,
        }
    }
    false
}

/// How the file binds the flag and the typing module aliases.
#[derive(Default)]
struct Bindings {
    /// `from typing import TYPE_CHECKING` or `TYPE_CHECKING = False`.
    flag_is_typing: bool,
    /// Local names bound by `import typing` / `import typing as t`.
    typing_aliases: BTreeSet<String>,
    /// Names bound by anything other than an import this module reads.
    other_names: BTreeSet<String>,
    /// The flag is bound in a way that may make it true at runtime.
    unreadable: bool,
}

impl Bindings {
    fn read(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.read_statement(statement);
        }
    }

    fn bind_target(&mut self, target: &Expr, value_is_false: bool) {
        match target {
            Expr::Name(name) if name.id.as_str() == FLAG => {
                if value_is_false {
                    self.flag_is_typing = true;
                } else {
                    self.unreadable = true;
                }
            }
            Expr::Name(name) => {
                self.other_names.insert(name.id.to_string());
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.bind_target(element, false);
                }
            }
            Expr::List(list) => {
                for element in &list.elts {
                    self.bind_target(element, false);
                }
            }
            Expr::Starred(starred) => self.bind_target(&starred.value, false),
            // `typing.TYPE_CHECKING = True` turns every `typing.TYPE_CHECKING`
            // guard on; any write to an attribute of that name is unread.
            Expr::Attribute(attribute) if attribute.attr.as_str() == FLAG => {
                self.unreadable = true;
            }
            _ => {}
        }
    }

    fn bind_name(&mut self, name: &str) {
        if name == FLAG {
            self.unreadable = true;
        } else {
            self.other_names.insert(name.to_string());
        }
    }

    fn read_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let module = alias.name.as_str();
                    let local = alias
                        .asname
                        .as_ref()
                        .map_or(module, |asname| asname.as_str());
                    if TYPING_MODULES.contains(&module) {
                        self.typing_aliases.insert(local.to_string());
                    } else {
                        // `import a.b` binds `a`.
                        self.bind_name(local.split('.').next().unwrap_or(local));
                    }
                }
            }
            Stmt::ImportFrom(import) => {
                let from_typing = import.level.is_none_or(|level| level.to_u32() == 0)
                    && import
                        .module
                        .as_ref()
                        .is_some_and(|module| TYPING_MODULES.contains(&module.as_str()));
                for alias in &import.names {
                    let imported = alias.name.as_str();
                    let local = alias
                        .asname
                        .as_ref()
                        .map_or(imported, |asname| asname.as_str());
                    if imported == "*" {
                        // A star import may bind any name, the flag included.
                        if !from_typing {
                            self.unreadable = true;
                        }
                    } else if from_typing && imported == FLAG && local == FLAG {
                        self.flag_is_typing = true;
                    } else {
                        self.bind_name(local);
                    }
                }
            }
            Stmt::Assign(assign) => {
                let value_is_false = is_false(&assign.value);
                for target in &assign.targets {
                    self.bind_target(target, value_is_false);
                }
            }
            Stmt::AnnAssign(assign) => {
                // A bare annotation (`TYPE_CHECKING: bool`) binds nothing.
                if let Some(value) = &assign.value {
                    self.bind_target(&assign.target, is_false(value));
                }
            }
            Stmt::AugAssign(assign) => self.bind_target(&assign.target, false),
            Stmt::Delete(delete) => {
                for target in &delete.targets {
                    self.bind_target(target, false);
                }
            }
            Stmt::FunctionDef(function) => {
                self.bind_name(function.name.as_str());
                self.read_arguments(&function.args);
                self.read(&function.body);
            }
            Stmt::AsyncFunctionDef(function) => {
                self.bind_name(function.name.as_str());
                self.read_arguments(&function.args);
                self.read(&function.body);
            }
            Stmt::ClassDef(class) => {
                self.bind_name(class.name.as_str());
                self.read(&class.body);
            }
            Stmt::If(statement) => {
                self.read(&statement.body);
                self.read(&statement.orelse);
            }
            Stmt::For(statement) => {
                self.bind_target(&statement.target, false);
                self.read(&statement.body);
                self.read(&statement.orelse);
            }
            Stmt::AsyncFor(statement) => {
                self.bind_target(&statement.target, false);
                self.read(&statement.body);
                self.read(&statement.orelse);
            }
            Stmt::While(statement) => {
                self.read(&statement.body);
                self.read(&statement.orelse);
            }
            Stmt::With(statement) => {
                self.read_with_items(&statement.items);
                self.read(&statement.body);
            }
            Stmt::AsyncWith(statement) => {
                self.read_with_items(&statement.items);
                self.read(&statement.body);
            }
            Stmt::Try(statement) => {
                self.read(&statement.body);
                self.read_handlers(&statement.handlers);
                self.read(&statement.orelse);
                self.read(&statement.finalbody);
            }
            Stmt::TryStar(statement) => {
                self.read(&statement.body);
                self.read_handlers(&statement.handlers);
                self.read(&statement.orelse);
                self.read(&statement.finalbody);
            }
            Stmt::Match(statement) => {
                // A capture pattern can bind any name; a file that mentions
                // the flag inside a `match` is not read.
                self.unreadable = true;
                for case in &statement.cases {
                    self.read(&case.body);
                }
            }
            Stmt::Global(global) => {
                for name in &global.names {
                    self.bind_name(name.as_str());
                }
            }
            Stmt::Nonlocal(nonlocal) => {
                for name in &nonlocal.names {
                    self.bind_name(name.as_str());
                }
            }
            _ => {}
        }
    }

    fn read_arguments(&mut self, arguments: &ast::Arguments) {
        for argument in arguments
            .posonlyargs
            .iter()
            .chain(&arguments.args)
            .chain(&arguments.kwonlyargs)
        {
            self.bind_name(argument.def.arg.as_str());
        }
        for argument in arguments.vararg.iter().chain(&arguments.kwarg) {
            self.bind_name(argument.arg.as_str());
        }
    }

    fn read_with_items(&mut self, items: &[ast::WithItem]) {
        for item in items {
            if let Some(target) = &item.optional_vars {
                self.bind_target(target, false);
            }
        }
    }

    fn read_handlers(&mut self, handlers: &[ast::ExceptHandler]) {
        for handler in handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            if let Some(name) = &handler.name {
                self.bind_name(name.as_str());
            }
            self.read(&handler.body);
        }
    }

    /// Whether `test` reads the typing flag and nothing else.
    fn is_flag_test(&self, test: &Expr) -> bool {
        match test {
            Expr::Name(name) => {
                // Any other binding of the flag already made the file
                // unreadable.
                name.id.as_str() == FLAG && self.flag_is_typing
            }
            Expr::Attribute(attribute) => {
                attribute.attr.as_str() == FLAG
                    && matches!(
                        attribute.value.as_ref(),
                        Expr::Name(module)
                            if self.typing_aliases.contains(module.id.as_str())
                                && !self.other_names.contains(module.id.as_str())
                    )
            }
            _ => false,
        }
    }
}

fn is_false(value: &Expr) -> bool {
    matches!(
        value,
        Expr::Constant(constant) if matches!(constant.value, ast::Constant::Bool(false))
    )
}

fn line_at(source: &str, offset: usize) -> usize {
    source
        .get(..offset)
        .map_or(0, |before| before.matches('\n').count() + 1)
}

fn collect(
    source: &str,
    statements: &[Stmt],
    bindings: &Bindings,
    out: &mut Vec<RangeInclusive<usize>>,
) {
    for statement in statements {
        match statement {
            Stmt::If(statement) => {
                if bindings.is_flag_test(&statement.test)
                    && let (Some(first), Some(last)) =
                        (statement.body.first(), statement.body.last())
                {
                    let if_line = line_at(source, usize::from(statement.range().start()));
                    let test_line = line_at(source, usize::from(statement.test.range().end()));
                    let start = line_at(source, usize::from(first.range().start()));
                    let end = line_at(source, usize::from(last.range().end()));
                    // `if TYPE_CHECKING: import x` shares the line with the
                    // test and is left alone. Otherwise the block is its body
                    // and the one-line `if` that guards it: a test that is
                    // always false has nothing to discriminate either.
                    if test_line != 0 && start > test_line && end >= start {
                        out.push(if if_line == test_line { if_line } else { start }..=end);
                    }
                } else {
                    collect(source, &statement.body, bindings, out);
                }
                collect(source, &statement.orelse, bindings, out);
            }
            Stmt::FunctionDef(function) => collect(source, &function.body, bindings, out),
            Stmt::AsyncFunctionDef(function) => collect(source, &function.body, bindings, out),
            Stmt::ClassDef(class) => collect(source, &class.body, bindings, out),
            Stmt::For(statement) => {
                collect(source, &statement.body, bindings, out);
                collect(source, &statement.orelse, bindings, out);
            }
            Stmt::AsyncFor(statement) => {
                collect(source, &statement.body, bindings, out);
                collect(source, &statement.orelse, bindings, out);
            }
            Stmt::While(statement) => {
                collect(source, &statement.body, bindings, out);
                collect(source, &statement.orelse, bindings, out);
            }
            Stmt::With(statement) => collect(source, &statement.body, bindings, out),
            Stmt::AsyncWith(statement) => collect(source, &statement.body, bindings, out),
            Stmt::Try(statement) => {
                collect(source, &statement.body, bindings, out);
                for handler in &statement.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect(source, &handler.body, bindings, out);
                }
                collect(source, &statement.orelse, bindings, out);
                collect(source, &statement.finalbody, bindings, out);
            }
            Stmt::TryStar(statement) => {
                collect(source, &statement.body, bindings, out);
                for handler in &statement.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect(source, &handler.body, bindings, out);
                }
                collect(source, &statement.orelse, bindings, out);
                collect(source, &statement.finalbody, bindings, out);
            }
            _ => {}
        }
    }
}

/// Whether the workspace holds its own top-level `typing` or
/// `typing_extensions` module (at the root or under `src/`), which an
/// absolute import could resolve to instead of the standard one. The
/// flag's value is then unknown, so no block is read (#14156). A module of
/// that name inside a package does not shadow an absolute import.
pub(super) fn workspace_may_shadow_typing<'a>(
    paths: impl IntoIterator<Item = &'a std::path::PathBuf>,
) -> bool {
    paths.into_iter().any(|path| {
        let mut components = path.iter().map(|component| component.to_string_lossy());
        let mut top = components.next();
        if top.as_deref() == Some("src") {
            top = components.next();
        }
        top.is_some_and(|component| {
            let stem = component
                .strip_suffix(".pyi")
                .or_else(|| component.strip_suffix(".py"))
                .unwrap_or(&component);
            TYPING_MODULES.contains(&stem)
        })
    })
}
