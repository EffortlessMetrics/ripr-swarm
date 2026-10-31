//! #10707: a boundary value that reaches the owner call through a test-local
//! name still places the input. `s = 50; assert tier(s) == "basic"` calls
//! `tier(50)` exactly as `assert tier(50) == "basic"` does, when the name is
//! bound once, to a literal, by a statement that runs unconditionally before
//! the call, and nothing else in the test can write or shadow it.

use super::boundary::literal_value;
use super::module_constants::mentions_identifier;
use super::source_facts::parse_module_result;
use super::{PythonTest, static_limits::is_simple_python_identifier};
use rustpython_parser::ast::{self, Expr, ExprContext, Fold, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;

/// The canonical literal `name` holds at the call whose argument list starts at
/// `call_start` (a byte offset into `test.body_text`), or None unless every
/// condition above holds. Fails closed: any parse failure, second binding,
/// shadowing parameter, nested binding form, deletion or `global`/`nonlocal`
/// declaration of the name keeps the input unresolved.
pub(super) fn test_local_literal(
    test: &PythonTest,
    name: &str,
    call_start: usize,
) -> Option<String> {
    if !is_simple_python_identifier(name) {
        return None;
    }
    let module = parse_module_result(&test.file, &test.body_text).ok()?;
    let ast::Mod::Module(module) = &module else {
        return None;
    };
    // The body text is the candidate test alone; refuse anything else so a
    // sibling function's assignment can never supply the value.
    let [statement] = module.body.as_slice() else {
        return None;
    };
    let function = match statement {
        Stmt::FunctionDef(function) => (&function.args, &function.body),
        Stmt::AsyncFunctionDef(function) => (&function.args, &function.body),
        _ => return None,
    };
    let (arguments, body) = function;
    if arguments_name(arguments, name) {
        return None;
    }
    let (binding_range, value) = body.iter().find_map(|statement| {
        let Stmt::Assign(assign) = statement else {
            return None;
        };
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            return None;
        };
        if target.id.as_str() != name || target.ctx != ExprContext::Store {
            return None;
        }
        let value = test.body_text.get(
            usize::from(assign.value.range().start())..usize::from(assign.value.range().end()),
        )?;
        Some((target.range, literal_value(value.trim())?))
    })?;
    if usize::from(binding_range.end()) >= call_start {
        return None;
    }
    let mut uses = LocalUses {
        name,
        binding_start: usize::from(binding_range.start()),
        binding_seen: false,
    };
    module_fold(&mut uses, &module.body).ok()?;
    uses.binding_seen.then_some(value)
}

fn arguments_name(arguments: &ast::Arguments, name: &str) -> bool {
    arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
        .any(|arg| arg.def.arg.as_str() == name)
        || arguments
            .vararg
            .as_ref()
            .is_some_and(|arg| arg.arg.as_str() == name)
        || arguments
            .kwarg
            .as_ref()
            .is_some_and(|arg| arg.arg.as_str() == name)
}

fn module_fold(uses: &mut LocalUses<'_>, body: &[Stmt]) -> Result<(), ()> {
    for statement in body {
        uses.fold_stmt(statement.clone())?;
    }
    Ok(())
}

/// Rejects every way the name can be written or shadowed other than the one
/// binding statement: another store or deletion (including walrus, loop,
/// `with` and comprehension targets), a parameter of a nested function or
/// lambda, a nested definition or import of that name, an exception or match
/// capture, and `global`/`nonlocal` declarations.
struct LocalUses<'a> {
    name: &'a str,
    binding_start: usize,
    binding_seen: bool,
}

impl Fold<TextRange> for LocalUses<'_> {
    type TargetU = TextRange;
    type Error = ();
    type UserContext = ();

    fn will_map_user(&mut self, _: &TextRange) {}

    fn map_user(&mut self, range: TextRange, _: ()) -> Result<TextRange, ()> {
        Ok(range)
    }

    fn fold_stmt(&mut self, node: Stmt) -> Result<Stmt, ()> {
        let writes_name = match &node {
            Stmt::Global(statement) => statement.names.iter().any(|id| id.as_str() == self.name),
            Stmt::Nonlocal(statement) => statement.names.iter().any(|id| id.as_str() == self.name),
            Stmt::FunctionDef(statement) => statement.name.as_str() == self.name,
            Stmt::AsyncFunctionDef(statement) => statement.name.as_str() == self.name,
            Stmt::ClassDef(statement) => statement.name.as_str() == self.name,
            Stmt::Import(statement) => statement.names.iter().any(|alias| self.binds(alias)),
            Stmt::ImportFrom(statement) => statement.names.iter().any(|alias| self.binds(alias)),
            // Capture patterns bind names without an `ExprName` store.
            Stmt::Match(_) => true,
            _ => false,
        };
        if writes_name {
            return Err(());
        }
        ast::fold::fold_stmt(self, node)
    }

    fn fold_excepthandler(&mut self, node: ast::ExceptHandler) -> Result<ast::ExceptHandler, ()> {
        let ast::ExceptHandler::ExceptHandler(handler) = &node;
        if handler
            .name
            .as_ref()
            .is_some_and(|id| id.as_str() == self.name)
        {
            return Err(());
        }
        ast::fold::fold_excepthandler(self, node)
    }

    fn fold_arguments(&mut self, node: ast::Arguments) -> Result<ast::Arguments, ()> {
        if arguments_name(&node, self.name) {
            return Err(());
        }
        ast::fold::fold_arguments(self, node)
    }

    fn fold_expr_name(&mut self, node: ast::ExprName) -> Result<ast::ExprName, ()> {
        if node.id.as_str() == self.name && node.ctx != ExprContext::Load {
            if node.ctx == ExprContext::Store
                && usize::from(node.range.start()) == self.binding_start
                && !self.binding_seen
            {
                self.binding_seen = true;
            } else {
                return Err(());
            }
        }
        Ok(node)
    }
}

impl LocalUses<'_> {
    fn binds(&self, alias: &ast::Alias) -> bool {
        alias.asname.as_ref().map_or_else(
            || alias.name.split('.').next() == Some(self.name),
            |asname| asname.as_str() == self.name,
        )
    }
}

/// The literal a bare name reads when it is a scalar constant of the test's
/// own module (`TH = 50` above the tests), or None. The constant must be bound
/// once at module scope with no `global` rebind or attribute write (the
/// module's stable-name rule), the test must not bind the name itself, and
/// neither the test nor its module may run code that could replace it
/// (#10707): an autouse fixture, `pytestmark` or setup hook, or a
/// `monkeypatch`, `patch`, `setattr`, `globals`, `vars`, `exec` or `eval` in the
/// body. A workspace file that writes the test module's constant
/// (`conftest.py` doing `module.TH = 49`, `setattr`, or `monkeypatch.setattr`)
/// also refuses credit (#11424). The parsed body must also never bind the
/// name in any scope (store, comprehension or loop target, lambda or nested
/// parameter, import, match). A test file that is also changed in the diff
/// gets no credit, since its constant may carry the edit.
pub(super) fn test_module_constant(test: &PythonTest, name: &str) -> Option<String> {
    const REPLACERS: [&str; 7] = [
        "monkeypatch",
        "patch",
        "setattr",
        "globals",
        "vars",
        "exec",
        "eval",
    ];
    let rebinding = &test.constant_rebinding;
    if !is_simple_python_identifier(name)
        || rebinding.rebinds(name)
        || rebinding.module_opaque
        || rebinding.implicit_reach
        || rebinding.edited_in_diff
        || rebinding.assigns_attribute(name)
        || REPLACERS
            .iter()
            .any(|word| mentions_identifier(&test.body_text, word))
    {
        return None;
    }
    if !body_never_binds(test, name) {
        return None;
    }
    rebinding
        .module_constants
        .iter()
        .find(|constant| constant.name == name)
        .map(|constant| constant.value.clone())
}

/// Whether the parsed test body binds `name` nowhere, in any scope.
fn body_never_binds(test: &PythonTest, name: &str) -> bool {
    let Ok(module) = parse_module_result(&test.file, &test.body_text) else {
        return false;
    };
    let ast::Mod::Module(module) = &module else {
        return false;
    };
    // No store is the binding, so any store of the name is refused.
    let mut uses = LocalUses {
        name,
        binding_start: usize::MAX,
        binding_seen: false,
    };
    module_fold(&mut uses, &module.body).is_ok()
}
