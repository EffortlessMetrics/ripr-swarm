//! A changed one-`return` function or method read through a chain of
//! one-`return` callers that a test pins exactly (#9226).
//!
//! `return 2` -> `return 3` in `Ledger._ledger_pad` is reached by a test only
//! through `Ledger().render_ledger()`, whose body is `return "|" + " " *
//! self._ledger_pad() + "|"`. No test calls the owner, so the finding read
//! `no_static_path` (RIPR-SPEC-0201) although `render_ledger() == "|  |"`
//! fails under the edit. This pass evaluates the chain, with the module-value
//! evaluator's values and operators, and credits exactly that:
//!
//! - the changed line is the file's only changed line and the single `return`
//!   of an undecorated module function or method of a model class, in the old
//!   and the new module;
//! - every callable the evaluation enters is a model callable: a top-level
//!   undecorated synchronous `def` with plain positional parameters (no
//!   defaults, `*args`, keywords, or annotations other than a bare name) whose
//!   body is one `return` (a leading docstring allowed) with no `yield`,
//!   `await` or `:=`, bound once and written by no workspace file; or such a
//!   method of a model class;
//! - a model class is a top-level `class` with no bases, keywords or
//!   decorators, bound once, whose body holds only a docstring, `pass` and
//!   `def`s, with no method named `__...` (no `__init__`, attribute hook,
//!   operator or name-mangled method), no method defined twice or bound by a
//!   walrus, and no `def` whose decorator, default or annotation runs code
//!   while the class body does; it is built as `Class()` with no arguments;
//! - no workspace file, the owner module included, writes a dunder attribute
//!   or reflects (`getattr`, `inspect`, `gc`, frames, a function's
//!   `__globals__`, `setattr`, ...) or touches the import system
//!   (`sys.modules`, `meta_path`, `__path__`, `importlib`, ...), which can
//!   write the module namespace under a name built at runtime or hand an
//!   importer another module;
//! - no other workspace file has the owner's module name (its file stem, or
//!   its package directory for `__init__.py`), since an import of its dotted
//!   path may load that file instead (pytest's `prepend` mode, or a package
//!   beating a same-named module), and no compiled extension module of that
//!   name (`ledger.*.so`, `ledger.pyd`) sits beside the owner, since the
//!   import system loads it before the source;
//! - no workspace file, the owner module included, spells a name or an
//!   f-string with a non-ASCII character (Python NFKC-normalises identifiers,
//!   so `_\u{ff50}ad` binds `_pad`, while name matches here are byte-exact)
//!   or declares a source encoding other than UTF-8 (in UTF-7 an ASCII
//!   comment can hold code);
//! - no workspace source has an unchecked-hash pyc, which CPython loads
//!   without comparing it with the source;
//! - inside a callable, a name is a parameter, a call is a bare call of a model
//!   function or class of the module, or a method call on an instance of a
//!   model class (the method's receiver, a parameter holding one, or
//!   `Class()`), all with positional arguments only; besides the
//!   module-value operators, `str * int` repeats a string and an ASCII string's
//!   `.upper()`/`.lower()` change its case;
//! - a straight-line `assert <entry> == <literal>` of a test reaches the chain
//!   through a function or class it imports with `from M import name` from the
//!   owner's module and does not rebind, or through a local bound once to
//!   `Class()` of a model class before the assertion and used only as a method
//!   receiver; the test requests no fixture, carries no decorator, is the only
//!   definition of its name, may not be skipped by a conftest, and runs only
//!   assertions, `pass`, a docstring and such bindings before the credited
//!   one, every earlier assertion holding on both sides;
//! - the assertion holds under the old body and fails under the new one.
//!
//! Anything the evaluator cannot decide is not a credit, and the finding keeps
//! its verdict. Evaluation depth (16) and calls (4096) are bounded, so a
//! recursive or fanned-out chain decides nothing.

use super::bool_oracle::count_bindings;
use super::module_constants::ModuleBindingFacts;
use super::module_value::{
    ModuleEdit, ModuleValueCredit, Value, arithmetic, assertion_expression, comparison,
    keep_unshadowed, mentions_reflection, names_may_differ, straight_line_assertion,
    with_line_replaced,
};
use super::related_tests::import_source_module_matches_owner;
use super::source_facts::parse_module_result;
use super::{PythonOwner, PythonTest};
use crate::domain::OwnerKind;
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use rustpython_parser::{Mode, Tok, lexer::lex};
use std::collections::BTreeMap;

/// The deepest call nesting the evaluator follows.
const MAX_DEPTH: usize = 16;

/// The longest string an evaluation may build.
const MAX_STRING: usize = 4096;

/// The most callable runs one test's evaluation may make, so a fan-out
/// within the depth bound stays cheap.
const MAX_STEPS: usize = 4096;

/// Words that let the owner's module replace a function or method's code, or
/// an instance's class, without a binding the module facts see (those already
/// treat `setattr`, `exec`, `globals`, `vars`, `__dict__` and `patch` as
/// opaque). Reflection, frames and the garbage collector are refused through
/// [`mentions_reflection`], in the owner module and in every other file.
const DYNAMIC_WORDS: [&str; 7] = [
    "__code__",
    "__defaults__",
    "__closure__",
    "__class__",
    "__bases__",
    "eval",
    "monkeypatch",
];

/// A callable the evaluator runs: positional parameters and one `return`
/// expression. A method's receiver is its first parameter.
#[derive(Clone)]
struct Callable {
    receiver: Option<String>,
    parameters: Vec<String>,
    expression: Expr,
}

/// The model callables of one side (old or new) of the owner's module.
#[derive(Default)]
struct Program {
    functions: BTreeMap<String, Callable>,
    classes: BTreeMap<String, BTreeMap<String, Callable>>,
}

/// A value inside a chain: a scalar, or an instance of a model class.
#[derive(Clone)]
enum Item {
    Value(Value),
    Instance(String),
}

/// The names one evaluation entered, checked against workspace writes.
#[derive(Default)]
struct Visited {
    names: Vec<String>,
    steps: usize,
}

impl Visited {
    fn enter(&mut self, name: &str) {
        if !self.names.iter().any(|seen| seen == name) {
            self.names.push(name.to_string());
        }
    }
}

pub(super) fn call_chain_credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Vec<ModuleValueCredit<'a>> {
    credits(edit, owner, owners, all_tests).unwrap_or_default()
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Option<Vec<ModuleValueCredit<'a>>> {
    let ModuleEdit {
        file,
        line,
        line_text,
        old_line_text,
        new_source,
    } = *edit;
    let old_line_text = old_line_text?;
    let (class, name) = match owner.owner_kind {
        Some(OwnerKind::Function) if owner.qualified_name == owner.name => {
            (None, owner.name.as_str())
        }
        Some(OwnerKind::Method) => {
            let (class, method) = owner.qualified_name.split_once('.')?;
            if method.contains('.') || method != owner.name {
                return None;
            }
            (Some(class), method)
        }
        _ => return None,
    };
    if !owner.decorators.is_empty() || !is_return_line(line_text) || !is_return_line(old_line_text)
    {
        return None;
    }
    let module_owner = owners
        .iter()
        .find(|candidate| candidate.is_module_owner() && candidate.file == file)?;
    if module_owner.foreign_writes.may_replace_reader_code()
        || module_owner.foreign_writes.writes_dunder()
        || module_owner.foreign_writes.reflection_elsewhere()
        || module_owner.foreign_writes.shares_module_name()
        || module_owner.foreign_writes.names_may_differ_elsewhere()
    {
        return None;
    }
    let old_source = with_line_replaced(new_source, line, old_line_text)?;
    let mut programs = Vec::new();
    for source in [old_source.as_str(), new_source] {
        if has_dynamic_word(source) || mentions_reflection(source) || names_may_differ(source) {
            return None;
        }
        let Mod::Module(module) = parse_module_result(file, source).ok()? else {
            return None;
        };
        // The owner's own module may install a lookup or construction hook
        // (`Ledger.__getattribute__ = ...`) without naming a method.
        if ModuleBindingFacts::of(source, &module.body).assigns_dunder() {
            return None;
        }
        let program = Program::of(source, &module.body);
        // The owner is a model callable on both sides, so the edit is the
        // change of its one `return`.
        program.callable(class, name)?;
        programs.push(program);
    }
    let [old, new] = programs.as_slice() else {
        return None;
    };
    let entries: Vec<&String> = new
        .functions
        .keys()
        .chain(new.classes.keys())
        .filter(|entry| old.functions.contains_key(*entry) || old.classes.contains_key(*entry))
        .collect();

    let mut out = Vec::new();
    for test in all_tests {
        if super::test_activation::activation_control(test).is_some()
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.module_opaque
            || test.constant_rebinding.implicit_reach
            || test.constant_rebinding.header_reach
            || !test.fixtures.is_empty()
            || !test.decorators.is_empty()
        {
            continue;
        }
        let spellings: BTreeMap<String, String> = entries
            .iter()
            .flat_map(|entry| {
                entry_spellings(test, module_owner, entry)
                    .into_iter()
                    .map(|spelling| (spelling, (*entry).clone()))
            })
            .collect();
        // A later definition with the same name replaces this test, so its
        // body is not what pytest runs. Checked after the cheap gates.
        if spellings.is_empty()
            || all_tests.iter().any(|other| {
                !std::ptr::eq(other, test)
                    && other.file == test.file
                    && other.qualified_name == test.qualified_name
            })
        {
            continue;
        }
        let Some(body) = test_body(test) else {
            continue;
        };
        let rows = statement_rows(&test.body_text, &body);
        let instances = local_instances(test, &body, &rows, &spellings, &|class| {
            old.classes.contains_key(class) && new.classes.contains_key(class)
        });
        // Assertions run in order: a later one is reached only when every
        // earlier one passes, so each earlier assertion must evaluate true
        // on both sides before a later flip counts.
        let mut assertions: Vec<_> = test.assertions.iter().collect();
        assertions.sort_by_key(|assertion| assertion.line);
        let mut old_visited = Visited::default();
        let mut new_visited = Visited::default();
        for assertion in assertions {
            if !straight_line_assertion(test, assertion) {
                break;
            }
            let Some(expression) = assertion_expression(assertion) else {
                break;
            };
            let Some(index) = assertion.line.checked_sub(test.line) else {
                break;
            };
            let locals: BTreeMap<String, String> = instances
                .iter()
                .filter(|(row, _, _)| *row < index)
                .map(|(_, name, class)| (name.clone(), class.clone()))
                .collect();
            let scope = TestScope {
                spellings: &spellings,
                locals: &locals,
            };
            let outcome = (
                evaluate_test(&expression, old, &scope, &mut old_visited),
                evaluate_test(&expression, new, &scope, &mut new_visited),
            );
            match outcome {
                (Some(Item::Value(Value::Bool(true))), Some(Item::Value(Value::Bool(true)))) => {
                    continue;
                }
                (Some(Item::Value(Value::Bool(true))), Some(Item::Value(Value::Bool(false)))) => {}
                _ => break,
            }
            let Some(entry) = pinned_entry(&expression) else {
                break;
            };
            if !inert_prelude(&body, &rows, index, &locals) {
                break;
            }
            // The sides differ only in the owner's body, so a flip shows the
            // chain ran it. Nothing the test's assertions ran may be
            // rewritten by another workspace file.
            if !old_visited
                .names
                .iter()
                .chain(&new_visited.names)
                .any(|visited| {
                    visited
                        .split('.')
                        .any(|part| module_owner.foreign_writes.may_rebind(part, file))
                })
            {
                out.push(ModuleValueCredit {
                    test,
                    assertion,
                    constant: owner.qualified_name.clone(),
                    old: return_text(old_line_text),
                    new: return_text(line_text),
                    reader: Some(entry),
                });
            }
            break;
        }
    }
    Some(out)
}

fn is_return_line(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("return ") || text.starts_with("return(")
}

fn return_text(text: &str) -> String {
    text.trim()
        .strip_prefix("return")
        .unwrap_or(text)
        .trim()
        .to_string()
}

fn has_dynamic_word(source: &str) -> bool {
    source
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .any(|word| DYNAMIC_WORDS.contains(&word))
}

impl Program {
    fn of(source: &str, body: &[Stmt]) -> Self {
        let facts = ModuleBindingFacts::of(source, body);
        let mut program = Self::default();
        for statement in body {
            match statement {
                Stmt::FunctionDef(function) if facts.bound_once(function.name.as_str()) => {
                    if let Some(callable) = callable_of(source, function, false) {
                        program
                            .functions
                            .insert(function.name.to_string(), callable);
                    }
                }
                Stmt::ClassDef(class) if facts.bound_once(class.name.as_str()) => {
                    if let Some(methods) = model_class(source, class, &facts) {
                        program.classes.insert(class.name.to_string(), methods);
                    }
                }
                _ => {}
            }
        }
        program
    }

    fn callable(&self, class: Option<&str>, name: &str) -> Option<&Callable> {
        match class {
            Some(class) => self.classes.get(class)?.get(name),
            None => self.functions.get(name),
        }
    }
}

/// The methods of a model class, or `None` when the class is not one.
fn model_class(
    source: &str,
    class: &ast::StmtClassDef,
    facts: &ModuleBindingFacts,
) -> Option<BTreeMap<String, Callable>> {
    if !class.bases.is_empty() || !class.keywords.is_empty() || !class.decorator_list.is_empty() {
        return None;
    }
    let mut methods = BTreeMap::new();
    let mut seen: Vec<&str> = Vec::new();
    for (index, statement) in class.body.iter().enumerate() {
        match statement {
            Stmt::Pass(_) => {}
            Stmt::Expr(docstring) if index == 0 && is_string(&docstring.value) => {}
            Stmt::FunctionDef(function) => {
                let name = function.name.as_str();
                // A dunder changes construction or lookup; a `__private`
                // name is mangled outside the class body. A decorator or
                // default runs while the class body executes and may bind a
                // method name there (a walrus), so any def carrying one keeps
                // the class out of the model.
                if name.starts_with("__")
                    || seen.contains(&name)
                    || facts.may_assign_attribute(name)
                    || facts.may_rebind(name)
                    || !function.decorator_list.is_empty()
                    || runs_definition_code(function)
                {
                    return None;
                }
                seen.push(name);
                if let Some(callable) = callable_of(source, function, true) {
                    methods.insert(name.to_string(), callable);
                }
            }
            _ => return None,
        }
    }
    Some(methods)
}

/// Whether a `def` evaluates anything when it runs: a default, or an
/// annotation other than a bare name or string.
fn runs_definition_code(function: &ast::StmtFunctionDef) -> bool {
    let arguments = &function.args;
    let plain = |annotation: Option<&Expr>| {
        annotation
            .is_none_or(|annotation| matches!(annotation, Expr::Name(_)) || is_string(annotation))
    };
    arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
        .any(|argument| argument.default.is_some() || !plain(argument.def.annotation.as_deref()))
        || [&arguments.vararg, &arguments.kwarg]
            .into_iter()
            .flatten()
            .any(|argument| !plain(argument.annotation.as_deref()))
        || !plain(function.returns.as_deref())
}

fn is_string(expr: &Expr) -> bool {
    matches!(expr, Expr::Constant(constant) if matches!(constant.value, ast::Constant::Str(_)))
}

/// The callable a `def` is, or `None` when it is not a model callable.
fn callable_of(source: &str, function: &ast::StmtFunctionDef, method: bool) -> Option<Callable> {
    let arguments = &function.args;
    if !function.decorator_list.is_empty()
        || suspends_or_binds(source, function)
        || function
            .returns
            .as_deref()
            .is_some_and(|returns| !matches!(returns, Expr::Name(_)))
        || !arguments.posonlyargs.is_empty()
        || !arguments.kwonlyargs.is_empty()
        || arguments.vararg.is_some()
        || arguments.kwarg.is_some()
        || arguments.args.iter().any(|argument| {
            argument.default.is_some()
                || argument
                    .def
                    .annotation
                    .as_deref()
                    .is_some_and(|annotation| !matches!(annotation, Expr::Name(_)))
        })
    {
        return None;
    }
    let mut parameters: Vec<String> = arguments
        .args
        .iter()
        .map(|argument| argument.def.arg.to_string())
        .collect();
    let receiver = if method {
        if parameters.is_empty() {
            return None;
        }
        Some(parameters.remove(0))
    } else {
        None
    };
    let statements: &[Stmt] = match function.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..] if is_string(&docstring.value) => rest,
        all => all,
    };
    let [
        Stmt::Return(ast::StmtReturn {
            value: Some(expression),
            ..
        }),
    ] = statements
    else {
        return None;
    };
    Some(Callable {
        receiver,
        parameters,
        expression: expression.as_ref().clone(),
    })
}

/// Whether the `def`'s text holds `yield` (a generator returns a generator,
/// whichever branch holds it), `await`, or a walrus binding.
fn suspends_or_binds(source: &str, function: &ast::StmtFunctionDef) -> bool {
    let range = function.range();
    let Some(text) = source.get(usize::from(range.start())..usize::from(range.end())) else {
        return true;
    };
    lex(text, Mode::Module).any(|token| {
        matches!(
            token,
            Err(_) | Ok((Tok::Yield | Tok::Await | Tok::ColonEqual, _))
        )
    })
}

/// The entry text of `<entry> == <literal>`, either way round.
fn pinned_entry(expression: &Expr) -> Option<String> {
    let Expr::Compare(compare) = expression else {
        return None;
    };
    let ([ast::CmpOp::Eq], [right]) = (compare.ops.as_slice(), compare.comparators.as_slice())
    else {
        return None;
    };
    let call = match (compare.left.as_ref(), right) {
        (Expr::Call(call), Expr::Constant(_)) | (Expr::Constant(_), Expr::Call(call)) => call,
        _ => return None,
    };
    super::expr_full_name(&call.func)
}

/// The test's own `def`, parsed from its body text.
fn test_body(test: &PythonTest) -> Option<Vec<Stmt>> {
    let Mod::Module(module) = parse_module_result(&test.file, &test.body_text).ok()? else {
        return None;
    };
    // The test's own `def`, by name: the body text is its source span.
    let name = test.name.rsplit('.').next()?;
    module
        .body
        .into_iter()
        .find_map(|statement| match statement {
            Stmt::FunctionDef(function) if function.name.as_str() == name => Some(function.body),
            _ => None,
        })
}

/// The locals of the test bound once to `Class()` of a model class and used
/// only as a method receiver, each with the body row of its binding. Built
/// once per test, so its cost does not grow with the assertion count.
fn local_instances(
    test: &PythonTest,
    body: &[Stmt],
    rows: &[usize],
    spellings: &BTreeMap<String, String>,
    model_class: &dyn Fn(&str) -> bool,
) -> Vec<(usize, String, String)> {
    let Ok(tokens) = lex(&test.body_text, Mode::Module).collect::<Result<Vec<_>, _>>() else {
        return Vec::new();
    };
    let tokens: Vec<&Tok> = tokens.iter().map(|(token, _)| token).collect();
    let mut locals = Vec::new();
    for (statement, &row) in body.iter().zip(rows) {
        let Stmt::Assign(assign) = statement else {
            continue;
        };
        let ([Expr::Name(target)], Expr::Call(call)) = (assign.targets.as_slice(), &*assign.value)
        else {
            continue;
        };
        if !call.args.is_empty() || !call.keywords.is_empty() {
            continue;
        }
        // Only `Class()` of a model class on both sides builds an instance
        // without running code; a model function's call is not a binding.
        let Some(class) = super::expr_full_name(&call.func)
            .and_then(|spelling| spellings.get(&spelling).cloned())
            .filter(|entry| model_class(entry))
        else {
            continue;
        };
        if count_bindings(body, target.id.as_str()) == 1
            && used_only_as_receiver(&tokens, target.id.as_str())
        {
            locals.push((row, target.id.to_string(), class));
        }
    }
    locals
}

/// Whether every statement before the assertion at body line `index` is an
/// assertion, a docstring, `pass`, or the binding of a model instance local.
/// Any other call may skip or end the test, or change what the chain runs.
fn inert_prelude(
    body: &[Stmt],
    rows: &[usize],
    index: usize,
    locals: &BTreeMap<String, String>,
) -> bool {
    for (position, (statement, &row)) in body.iter().zip(rows).enumerate() {
        if row >= index {
            return true;
        }
        let inert = match statement {
            Stmt::Assert(_) | Stmt::Pass(_) => true,
            Stmt::Expr(docstring) => position == 0 && is_string(&docstring.value),
            Stmt::Assign(assign) => matches!(
                assign.targets.as_slice(),
                [Expr::Name(target)] if locals.contains_key(target.id.as_str())
            ),
            _ => false,
        };
        if !inert {
            return false;
        }
    }
    false
}

/// The body row of each top-level statement of a test body, counted once.
/// A statement whose offset does not fit reads as past every assertion.
fn statement_rows(body_text: &str, body: &[Stmt]) -> Vec<usize> {
    let mut rows = Vec::with_capacity(body.len());
    let (mut seen, mut row) = (0, 0);
    for statement in body {
        let offset = usize::try_from(u32::from(statement.range().start())).unwrap_or(usize::MAX);
        match body_text.get(seen..offset) {
            Some(gap) => {
                row += gap.matches('\n').count();
                seen = offset;
                rows.push(row);
            }
            None => rows.push(usize::MAX),
        }
    }
    rows
}

/// The spellings a test calls the owner module's `name` by: only `from M
/// import name` of the owner module, which reads `name` from the loaded module
/// itself. `import pkg.mod as alias` and `from pkg import mod` resolve `mod`
/// as an attribute of the package, which its `__init__.py` may rebind.
fn entry_spellings(test: &PythonTest, module_owner: &PythonOwner, name: &str) -> Vec<String> {
    let spellings = test
        .imports
        .iter()
        .filter(|import| {
            import.imported == name
                && import_source_module_matches_owner(import, module_owner, &test.file)
        })
        .map(|import| import.alias.clone())
        .collect();
    keep_unshadowed(test, spellings)
}

/// Whether every mention of `local` in the test is its one binding or the
/// receiver of a method call (`local.name(`). An instance attribute write
/// (`local._pad = ...`), a `del`, or handing the instance to other code may
/// change what its methods run, so any other mention keeps the local out.
fn used_only_as_receiver(tokens: &[&Tok], local: &str) -> bool {
    tokens.iter().enumerate().all(|(index, token)| {
        let Tok::Name { name } = token else {
            return true;
        };
        if name.as_str() != local {
            return true;
        }
        let after = |offset: usize| tokens.get(index + offset).copied();
        let binding = matches!(after(1), Some(Tok::Equal))
            && !matches!(
                index.checked_sub(1).and_then(|before| tokens.get(before)),
                Some(Tok::Dot)
            );
        let receiver = matches!(after(1), Some(Tok::Dot))
            && matches!(after(2), Some(Tok::Name { .. }))
            && matches!(after(3), Some(Tok::Lpar));
        binding || receiver
    })
}

/// What the names of a test assertion mean.
struct TestScope<'a> {
    /// Import spellings of the owner module's model functions and classes.
    spellings: &'a BTreeMap<String, String>,
    /// Locals bound to an instance of a model class.
    locals: &'a BTreeMap<String, String>,
}

fn evaluate_test(
    expression: &Expr,
    program: &Program,
    scope: &TestScope<'_>,
    visited: &mut Visited,
) -> Option<Item> {
    match expression {
        Expr::Compare(_) | Expr::Constant(_) | Expr::BinOp(_) | Expr::UnaryOp(_) => {
            evaluate(expression, program, &Frame::test(scope), visited, 0)
        }
        _ => None,
    }
}

/// The meaning of names inside one evaluation step.
struct Frame<'a> {
    /// Parameter values of the running callable.
    parameters: Vec<(String, Item)>,
    /// Set while evaluating the test's own assertion.
    test: Option<&'a TestScope<'a>>,
}

impl<'a> Frame<'a> {
    fn test(scope: &'a TestScope<'a>) -> Self {
        Self {
            parameters: Vec::new(),
            test: Some(scope),
        }
    }

    fn parameter(&self, name: &str) -> Option<&Item> {
        self.parameters
            .iter()
            .find(|(parameter, _)| parameter == name)
            .map(|(_, item)| item)
    }
}

fn evaluate(
    expression: &Expr,
    program: &Program,
    frame: &Frame<'_>,
    visited: &mut Visited,
    depth: usize,
) -> Option<Item> {
    if depth > MAX_DEPTH {
        return None;
    }
    let value = |expression: &Expr, visited: &mut Visited| -> Option<Value> {
        match evaluate(expression, program, frame, visited, depth)? {
            Item::Value(value) => Some(value),
            Item::Instance(_) => None,
        }
    };
    match expression {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(number) => number.to_string().parse().ok().map(Value::Int),
            ast::Constant::Bool(flag) => Some(Value::Bool(*flag)),
            ast::Constant::Str(text) => Some(Value::Str(text.clone())),
            _ => None,
        }
        .map(Item::Value),
        Expr::Name(name) => {
            let name = name.id.as_str();
            match frame.test {
                Some(scope) => scope
                    .locals
                    .get(name)
                    .map(|class| Item::Instance(class.clone())),
                None => frame.parameter(name).cloned(),
            }
        }
        Expr::UnaryOp(unary) => {
            let operand = value(&unary.operand, visited)?;
            match unary.op {
                ast::UnaryOp::Not => Some(Value::Bool(!operand.truthy())),
                ast::UnaryOp::USub => match operand {
                    Value::Int(number) => number.checked_neg().map(Value::Int),
                    Value::Bool(flag) => Some(Value::Int(-i128::from(flag))),
                    _ => None,
                },
                _ => None,
            }
            .map(Item::Value)
        }
        Expr::BinOp(binary) => {
            let left = value(&binary.left, visited)?;
            let right = value(&binary.right, visited)?;
            repetition(&binary.op, &left, &right)
                .or_else(|| arithmetic(&binary.op, &left, &right, false))
                .filter(|value| !matches!(value, Value::Str(text) if text.len() > MAX_STRING))
                .map(Item::Value)
        }
        Expr::Compare(compare) => {
            let mut left = value(&compare.left, visited)?;
            for (op, comparator) in compare.ops.iter().zip(&compare.comparators) {
                let right = value(comparator, visited)?;
                if !comparison(op, &left, &right, false)? {
                    return Some(Item::Value(Value::Bool(false)));
                }
                left = right;
            }
            Some(Item::Value(Value::Bool(true)))
        }
        Expr::IfExp(conditional) => {
            if value(&conditional.test, visited)?.truthy() {
                evaluate(&conditional.body, program, frame, visited, depth)
            } else {
                evaluate(&conditional.orelse, program, frame, visited, depth)
            }
        }
        Expr::Call(call) if call.keywords.is_empty() => {
            call_item(call, program, frame, visited, depth)
        }
        _ => None,
    }
}

/// `str * int` and `int * str`, as Python repeats a string.
fn repetition(op: &ast::Operator, left: &Value, right: &Value) -> Option<Value> {
    if !matches!(op, ast::Operator::Mult) {
        return None;
    }
    let (text, count) = match (left, right) {
        (Value::Str(text), Value::Int(count)) | (Value::Int(count), Value::Str(text)) => {
            (text, *count)
        }
        (Value::Str(text), Value::Bool(flag)) | (Value::Bool(flag), Value::Str(text)) => {
            (text, i128::from(*flag))
        }
        _ => return None,
    };
    let count = usize::try_from(count.max(0)).ok()?;
    if text.len().checked_mul(count)? > MAX_STRING {
        return None;
    }
    Some(Value::Str(text.repeat(count)))
}

fn call_item(
    call: &ast::ExprCall,
    program: &Program,
    frame: &Frame<'_>,
    visited: &mut Visited,
    depth: usize,
) -> Option<Item> {
    match call.func.as_ref() {
        // A bare call: a model function of the module, or, in the test, an
        // imported one or a class built with no arguments.
        Expr::Name(name) => {
            let name = name.id.as_str();
            let target = match frame.test {
                Some(scope) => scope.spellings.get(name)?.as_str(),
                None if frame.parameter(name).is_some() => return None,
                None => name,
            };
            if program.classes.contains_key(target) {
                return call
                    .args
                    .is_empty()
                    .then(|| Item::Instance(target.to_string()));
            }
            let callable = program.functions.get(target)?;
            visited.enter(target);
            run(callable, None, &call.args, program, frame, visited, depth)
        }
        Expr::Attribute(attribute) => {
            let method = attribute.attr.as_str();
            // `module.function(...)` or `module.Class()` in the test.
            if let Some(scope) = frame.test
                && let Some(full) = super::expr_full_name(&call.func)
                && let Some(target) = scope.spellings.get(&full)
            {
                if program.classes.contains_key(target) {
                    return call.args.is_empty().then(|| Item::Instance(target.clone()));
                }
                let callable = program.functions.get(target)?;
                visited.enter(target);
                return run(callable, None, &call.args, program, frame, visited, depth);
            }
            let receiver = evaluate(&attribute.value, program, frame, visited, depth);
            match receiver? {
                Item::Instance(class) => {
                    // An instance only comes from a model `Class()`, so its
                    // methods are the model's.
                    let callable = program.classes.get(&class)?.get(method)?;
                    visited.enter(&format!("{class}.{method}"));
                    run(
                        callable,
                        Some(class.as_str()),
                        &call.args,
                        program,
                        frame,
                        visited,
                        depth,
                    )
                }
                Item::Value(Value::Str(text)) if call.args.is_empty() && text.is_ascii() => {
                    match method {
                        "upper" => Some(Item::Value(Value::Str(text.to_ascii_uppercase()))),
                        "lower" => Some(Item::Value(Value::Str(text.to_ascii_lowercase()))),
                        _ => None,
                    }
                }
                Item::Value(_) => None,
            }
        }
        _ => None,
    }
}

fn run(
    callable: &Callable,
    class: Option<&str>,
    arguments: &[Expr],
    program: &Program,
    frame: &Frame<'_>,
    visited: &mut Visited,
    depth: usize,
) -> Option<Item> {
    visited.steps += 1;
    if arguments.len() != callable.parameters.len() || visited.steps > MAX_STEPS {
        return None;
    }
    let mut parameters = Vec::new();
    for (parameter, argument) in callable.parameters.iter().zip(arguments) {
        if matches!(argument, Expr::Starred(_)) {
            return None;
        }
        let item = evaluate(argument, program, frame, visited, depth)?;
        parameters.push((parameter.clone(), item));
    }
    match (&callable.receiver, class) {
        (Some(receiver), Some(class)) => {
            parameters.push((receiver.clone(), Item::Instance(class.to_string())));
        }
        (None, None) => {}
        _ => return None,
    }
    let inner = Frame {
        parameters,
        test: None,
    };
    evaluate(&callable.expression, program, &inner, visited, depth + 1)
}
