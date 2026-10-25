//! A module-level constant edit read through the assertions that depend on it
//! (#9423).
//!
//! `LIMIT = 10` -> `LIMIT = 12` has no callable owner: the edit sits at module
//! scope, so only a filename heuristic related tests and the finding abstained.
//! The edit is still observable by a test whose assertion statically depends
//! on the value, and this pass reads exactly that and nothing more:
//!
//! - the changed line is the one top-level `NAME = <literal>` statement that
//!   binds `NAME` ([`module_literal_constants`]: bound once, no `global`, no
//!   walrus, no attribute write, no star import, no dynamic namespace writer),
//!   and it is the file's only changed line, so the old module is the new one
//!   with that line put back;
//! - the assertion is a straight-line `assert` of a test that imports the
//!   module, and its condition is evaluated for both values. It reads the
//!   constant itself (`from settings import LIMIT`) or calls a module function
//!   with literal arguments (`over_limit(11)`) whose body is one `return
//!   <expression>` over its parameters, literals and the constant;
//! - the assertion holds under the old value and fails under the new one.
//!
//! Anything the evaluator cannot decide (a name, a call, an operator or a
//! value kind it does not model) is not a credit, and the finding keeps the
//! `missing_import_graph` abstention. A value no assertion distinguishes is not
//! a gap either: other code or tests may observe it.

use super::bool_oracle::count_bindings;
use super::module_constants::{
    ModuleBindingFacts, PythonModuleConstant, assigned_attributes, mentions_identifier,
    module_literal_constants,
};
use super::related_tests::{
    import_source_module_matches_owner, owner_module_callees, submodule_receivers,
};
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, is_test_file};
use super::{PythonAssertion, PythonOwner, PythonTest, expr_full_name};
use crate::domain::OwnerKind;
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// Words that mark a file able to rewrite another module's attribute at
/// runtime without an assignment statement.
const DYNAMIC_WRITERS: [&str; 10] = [
    "setattr",
    "patch",
    "monkeypatch",
    "reload",
    "importlib",
    "__import__",
    "__dict__",
    "globals",
    "vars",
    "exec",
];

/// What the workspace's other Python files can do to a module's attributes:
/// assigned attribute names, and every word of a file that holds a dynamic
/// writer (`setattr`, `patch`, `monkeypatch`, `importlib.reload`,
/// `sys.modules`, ...). Filled for the module owner of a changed file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ForeignWrites {
    attributes: Vec<String>,
    dynamic_words: Vec<String>,
    /// A workspace file could not be read or the scan hit its bound, so the
    /// absence of a write is unknown.
    unknown: bool,
}

/// Most distinct words retained in the workspace reference index.
const MAX_WORKSPACE_WORDS: usize = 200_000;

/// Spellings that can hide an imported module's name from the word index.
const DYNAMIC_LOADERS: [&str; 13] = [
    "importlib",
    "__import__",
    "exec",
    "eval",
    "compile",
    "runpy",
    "load_module",
    "exec_module",
    "import_module",
    "getattr",
    "builtins",
    "__builtins__",
    "__dict__",
];

/// What each workspace source contributes to [`ForeignWrites`], parsed and
/// tokenized once so a diff that changes many Python files does not repeat
/// the whole-workspace work per file (#10125).
pub(super) struct ForeignIndex<'a> {
    files: Vec<IndexedSource<'a>>,
    /// Each word's only source, or None when two sources mention it. A
    /// malformed file's own function/name is not an import.
    references: std::collections::BTreeMap<&'a str, Option<&'a Path>>,
    hidden_import: bool,
    /// The reference index passed its bound or a file went unread.
    unknown: bool,
}

// Parses the foreign index has done on this thread, so a test can pin that
// the workspace is parsed once per run and not once per changed file (#10125).
#[cfg(test)]
thread_local! {
    pub(super) static FOREIGN_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct IndexedSource<'a> {
    file: &'a Path,
    /// None when the file does not parse.
    assigned: Option<Vec<String>>,
    /// Holds a dynamic writer, or does not parse: every word is a possible write.
    dynamic_words: Option<Vec<&'a str>>,
}

impl<'a> ForeignIndex<'a> {
    pub(super) fn build(
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
    ) -> Self {
        let mut files = Vec::new();
        let mut references = std::collections::BTreeMap::new();
        let mut hidden_import = false;
        for (file, source) in sources {
            hidden_import |= DYNAMIC_LOADERS
                .iter()
                .any(|loader| mentions_identifier(source, loader))
                || source.contains("sys.modules")
                || (file.file_name().is_some_and(|name| name == "__init__.py")
                    && mentions_identifier(source, "__all__"));
            for word in source_words(source) {
                let first = references.entry(word).or_insert(Some(file));
                if first.is_some_and(|first| first != file) {
                    *first = None;
                }
                if references.len() > MAX_WORKSPACE_WORDS {
                    return Self {
                        files: Vec::new(),
                        references: std::collections::BTreeMap::new(),
                        hidden_import,
                        unknown: true,
                    };
                }
            }
            #[cfg(test)]
            FOREIGN_PARSES.with(|parses| parses.set(parses.get() + 1));
            let assigned = assigned_attributes(source);
            let dynamic = assigned.is_none()
                || DYNAMIC_WRITERS
                    .iter()
                    .any(|writer| mentions_identifier(source, writer))
                || source.contains("sys.modules");
            files.push(IndexedSource {
                file,
                assigned,
                dynamic_words: dynamic.then(|| source_words(source).collect()),
            });
        }
        Self {
            files,
            references,
            hidden_import,
            unknown: unread_files,
        }
    }

    /// The writes in every source but `own`, with `unknown` when some
    /// workspace file went unread.
    pub(super) fn excluding(&self, own: &Path) -> ForeignWrites {
        if self.unknown && self.files.is_empty() {
            return ForeignWrites {
                unknown: true,
                ..ForeignWrites::default()
            };
        }
        let mut attributes = std::collections::BTreeSet::new();
        let mut words = std::collections::BTreeSet::new();
        let mut unparsed = Vec::new();
        for indexed in self.files.iter().filter(|indexed| indexed.file != own) {
            match &indexed.assigned {
                Some(assigned) => attributes.extend(assigned.iter().cloned()),
                None => unparsed.push(indexed.file),
            }
            if let Some(dynamic) = &indexed.dynamic_words {
                words.extend(dynamic.iter().map(|word| word.to_string()));
            }
        }
        // An unreferenced malformed production module cannot poison every
        // binding. Retain its words as potential writes, and fail closed on
        // references, implicit collection/initializers and hidden imports.
        let unknown = self.unknown
            || (!unparsed.is_empty() && self.hidden_import)
            || unparsed.iter().any(|file| {
                is_test_file(file)
                    || file
                        .file_name()
                        .is_some_and(|name| name == "__init__.py" || name == "conftest.py")
                    || file
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .is_none_or(|stem| {
                            self.references
                                .get(stem)
                                .is_some_and(|first| first.is_none_or(|first| first != *file))
                        })
            });
        ForeignWrites {
            attributes: attributes.into_iter().collect(),
            dynamic_words: words.into_iter().collect(),
            unknown,
        }
    }
}

impl ForeignWrites {
    /// The writes in every source but `own`; one-shot form of
    /// [`ForeignIndex`] for callers that scan a single file.
    #[cfg(test)]
    pub(super) fn scan<'a>(
        own: &Path,
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
    ) -> Self {
        ForeignIndex::build(sources, unread_files).excluding(own)
    }

    /// Whether another file may rebind `name` of the module at `module_file`:
    /// it assigns an attribute of that name, or holds a dynamic writer and
    /// mentions the name or the module's file stem. An unfilled value (a
    /// context that never scanned) rebinds nothing it knows of; the diff
    /// pipeline always fills it.
    pub(super) fn may_rebind(&self, name: &str, module_file: &Path) -> bool {
        let stem = module_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        self.unknown
            || self
                .attributes
                .iter()
                .any(|attribute| attribute == name || attribute == "*")
            || self.dynamic_words.iter().any(|word| word == name)
            || (!stem.is_empty() && self.dynamic_words.iter().any(|word| word == stem))
    }
}

fn source_words(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
}

/// One assertion that fails when the constant takes its new value.
pub(super) struct ModuleValueCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
    /// The edited constant, with its old and new literal text.
    pub(super) constant: String,
    pub(super) old: String,
    pub(super) new: String,
    /// The module function the assertion reads the constant through, when it
    /// does not read the constant directly.
    pub(super) reader: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Int(i128),
    Bool(bool),
    Str(String),
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Self::Int(value) => *value != 0,
            Self::Bool(value) => *value,
            Self::Str(value) => !value.is_empty(),
        }
    }

    fn number(&self) -> Option<i128> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Bool(value) => Some(i128::from(*value)),
            Self::Str(_) => None,
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Bool(true) => "True".to_string(),
            Self::Bool(false) => "False".to_string(),
            Self::Str(value) => format!("{value:?}"),
        }
    }
}

/// A module function the evaluator can run: positional parameters and one
/// `return` expression.
struct Reader {
    parameters: Vec<String>,
    expression: Expr,
}

/// What an expression's names and calls mean while it is evaluated.
struct Scope<'a> {
    constant: &'a str,
    value: &'a Value,
    /// Reader mode: the parameter values of one call. `None` evaluates a test
    /// expression, where no bare name but a constant spelling is known.
    parameters: Option<&'a [(String, Value)]>,
    /// Test mode: the spellings that read the constant itself.
    constant_spellings: &'a [String],
    /// Test mode: the spellings that call the reader, and the reader.
    reader_spellings: &'a [String],
    reader: Option<&'a Reader>,
}

/// The changed line of a module-level edit and the new-side source it sits in.
pub(super) struct ModuleEdit<'a> {
    pub(super) file: &'a Path,
    pub(super) line: usize,
    pub(super) line_text: &'a str,
    pub(super) old_line_text: Option<&'a str>,
    pub(super) new_source: &'a str,
}

/// The credits for an edit of a module-level constant. Empty when the edit is
/// not a certain constant replacement or no assertion distinguishes the values.
pub(super) fn module_constant_credits<'a>(
    edit: &ModuleEdit<'_>,
    module_owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Vec<ModuleValueCredit<'a>> {
    credits(edit, module_owner, owners, all_tests).unwrap_or_default()
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    module_owner: &PythonOwner,
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
    let (name, new_value) = constant_line(line_text)?;
    let (old_name, old_value) = constant_line(old_line_text?)?;
    if name != old_name || old_value == new_value {
        return None;
    }
    // The new module, with the changed line the one constant binding.
    let Mod::Module(new_module) = parse_module_result(file, new_source).ok()? else {
        return None;
    };
    literal_constant(new_source, &new_module.body, &name, line)?;
    // Another workspace file (production code, a conftest, a helper module)
    // that writes the attribute or patches the module can rebind it for every
    // test.
    if module_owner.foreign_writes.may_rebind(&name, file) {
        return None;
    }
    // Another test file writing the attribute can rebind it for every test.
    if all_tests
        .iter()
        .any(|test| test.constant_rebinding.attributes.contains(&name))
    {
        return None;
    }
    // The old module: the new one with the one changed line put back.
    let old_source = with_line_replaced(new_source, line, old_line_text?)?;
    let Mod::Module(old_module) = parse_module_result(file, &old_source).ok()? else {
        return None;
    };
    literal_constant(&old_source, &old_module.body, &name, line)?;

    let new_facts = ModuleBindingFacts::of(new_source, &new_module.body);
    let old_facts = ModuleBindingFacts::of(&old_source, &old_module.body);
    // Module functions that read the constant, as the old and new module
    // define them.
    let mut readers: Vec<(&PythonOwner, Reader, Reader)> = Vec::new();
    for owner in owners.iter().filter(|owner| {
        owner.file == file
            && owner.owner_kind == Some(OwnerKind::Function)
            && owner.qualified_name == owner.name
            && owner.class_path.is_empty()
            && owner.decorators.is_empty()
            && owner.name != name
    }) {
        if !new_facts.bound_once(&owner.name)
            || !old_facts.bound_once(&owner.name)
            || module_owner.foreign_writes.may_rebind(&owner.name, file)
        {
            continue;
        }
        let (Some(new_reader), Some(old_reader)) = (
            reader_of(&new_module.body, &owner.name, &name),
            reader_of(&old_module.body, &owner.name, &name),
        ) else {
            continue;
        };
        readers.push((owner, old_reader, new_reader));
    }

    let mut out = Vec::new();
    for test in all_tests {
        // A test file edited in the same diff may assert the new value; only
        // an assertion the diff leaves alone is read as a pre-existing oracle.
        if super::test_activation::activation_control(test).is_some()
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.assigns_attribute(&name)
        {
            continue;
        }
        let constant_spellings = constant_spellings(test, module_owner, &name);
        for assertion in &test.assertions {
            let Some(expression) = assertion_expression(assertion) else {
                continue;
            };
            if !straight_line_assertion(test, assertion) {
                continue;
            }
            let Some(preceding) = test
                .assertions
                .iter()
                .filter(|prior| prior.line < assertion.line)
                .map(assertion_expression)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            // The constant read directly.
            if !constant_spellings.is_empty()
                && flips(
                    &expression,
                    &name,
                    (&old_value, &new_value),
                    &constant_spellings,
                    &[],
                    (None, None),
                    &preceding,
                )
            {
                out.push(credit(
                    test,
                    assertion,
                    &name,
                    (&old_value, &new_value),
                    None,
                ));
                break;
            }
            // The constant read through a module function the test calls.
            let mut credited = false;
            for (owner, old_reader, new_reader) in &readers {
                if test.constant_rebinding.assigns_attribute(&owner.name) {
                    continue;
                }
                let spellings = reader_spellings(test, owner);
                if spellings.is_empty()
                    || !flips(
                        &expression,
                        &name,
                        (&old_value, &new_value),
                        &constant_spellings,
                        &spellings,
                        (Some(old_reader), Some(new_reader)),
                        &preceding,
                    )
                {
                    continue;
                }
                out.push(credit(
                    test,
                    assertion,
                    &name,
                    (&old_value, &new_value),
                    Some(owner.name.clone()),
                ));
                credited = true;
                break;
            }
            if credited {
                break;
            }
        }
    }
    Some(out)
}

fn credit<'a>(
    test: &'a PythonTest,
    assertion: &'a PythonAssertion,
    name: &str,
    values: (&Value, &Value),
    reader: Option<String>,
) -> ModuleValueCredit<'a> {
    ModuleValueCredit {
        test,
        assertion,
        constant: name.to_string(),
        old: values.0.render(),
        new: values.1.render(),
        reader,
    }
}

/// Whether the assertion holds under the old value and fails under the new
/// one, with every part of both evaluations decided.
fn flips(
    expression: &Expr,
    name: &str,
    values: (&Value, &Value),
    constant_spellings: &[String],
    reader_spellings: &[String],
    readers: (Option<&Reader>, Option<&Reader>),
    preceding: &[Expr],
) -> bool {
    let outcome = |value: &Value, reader: Option<&Reader>| {
        let scope = Scope {
            constant: name,
            value,
            parameters: None,
            constant_spellings,
            reader_spellings,
            reader,
        };
        if !preceding
            .iter()
            .all(|prior| evaluate(prior, &scope).is_some_and(|value| value.truthy()))
        {
            return None;
        }
        evaluate(expression, &scope).map(|result| result.truthy())
    };
    outcome(values.0, readers.0) == Some(true) && outcome(values.1, readers.1) == Some(false)
}

fn constant_line(text: &str) -> Option<(String, Value)> {
    let module = parse_module_result(Path::new("line.py"), text.trim_end()).ok()?;
    let Mod::Module(module) = module else {
        return None;
    };
    let [stmt] = module.body.as_slice() else {
        return None;
    };
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
    let empty = Scope {
        constant: "",
        value: &Value::Bool(false),
        parameters: None,
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
    };
    Some((name.id.to_string(), evaluate(value, &empty)?))
}

fn literal_constant(
    source: &str,
    body: &[Stmt],
    name: &str,
    line: usize,
) -> Option<PythonModuleConstant> {
    let text = SourceText::new(source);
    module_literal_constants(&text, body)
        .into_iter()
        .find(|constant| constant.name == name && constant.line == line)
}

/// `source` with the 1-indexed `line` replaced by one line of `replacement`.
fn with_line_replaced(source: &str, line: usize, replacement: &str) -> Option<String> {
    if replacement.contains('\n') || replacement.contains('\r') {
        return None;
    }
    let mut out = String::with_capacity(source.len());
    let mut replaced = false;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            out.push_str(replacement);
            if text.ends_with('\n') {
                out.push('\n');
            }
            replaced = true;
        } else {
            out.push_str(text);
        }
    }
    replaced.then_some(out)
}

/// The top-level function `function` as an evaluable reader of `constant`:
/// undecorated, positional parameters only, a body of one `return` expression
/// (after an optional docstring) that names the constant. Defined exactly once
/// in the module.
fn reader_of(module: &[Stmt], function: &str, constant: &str) -> Option<Reader> {
    if count_bindings(module, function) != 1 {
        return None;
    }
    let definition = module.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(definition) if definition.name.as_str() == function => Some(definition),
        _ => None,
    })?;
    let args = &definition.args;
    if !definition.decorator_list.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || !args.kwonlyargs.is_empty()
        || args
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .any(|arg| arg.default.is_some())
    {
        return None;
    }
    let parameters: Vec<String> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.to_string())
        .collect();
    // A parameter named like the constant shadows it.
    if parameters.iter().any(|parameter| parameter == constant) {
        return None;
    }
    let body = match definition.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..]
            if matches!(docstring.value.as_ref(), Expr::Constant(_)) && !rest.is_empty() =>
        {
            rest
        }
        all => all,
    };
    let [Stmt::Return(ret)] = body else {
        return None;
    };
    let expression = ret.value.as_deref()?.clone();
    names_in(&expression)
        .iter()
        .any(|used| used == constant)
        .then_some(Reader {
            parameters,
            expression,
        })
}

/// Every bare name an expression reads.
fn names_in(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_names(expr, &mut out);
    out
}

fn collect_names(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Name(name) => out.push(name.id.to_string()),
        Expr::BoolOp(op) => op.values.iter().for_each(|value| collect_names(value, out)),
        Expr::BinOp(op) => {
            collect_names(&op.left, out);
            collect_names(&op.right, out);
        }
        Expr::UnaryOp(op) => collect_names(&op.operand, out),
        Expr::IfExp(op) => {
            collect_names(&op.test, out);
            collect_names(&op.body, out);
            collect_names(&op.orelse, out);
        }
        Expr::Compare(op) => {
            collect_names(&op.left, out);
            op.comparators
                .iter()
                .for_each(|value| collect_names(value, out));
        }
        _ => {}
    }
}

/// The spellings with which `test` reads the module's constant: the name it
/// imports from the module (`from settings import LIMIT as L`) and `module.LIMIT`
/// for a receiver that reaches the module. A spelling whose root the test binds
/// itself is not one.
fn constant_spellings(test: &PythonTest, module_owner: &PythonOwner, name: &str) -> Vec<String> {
    let mut spellings = Vec::new();
    for import in &test.imports {
        if import.imported == name
            && import_source_module_matches_owner(import, module_owner, &test.file)
        {
            spellings.push(import.alias.clone());
        }
        for receiver in submodule_receivers(import, module_owner, &test.file) {
            spellings.push(format!("{receiver}.{name}"));
        }
    }
    keep_unshadowed(test, spellings)
}

fn reader_spellings(test: &PythonTest, owner: &PythonOwner) -> Vec<String> {
    keep_unshadowed(test, owner_module_callees(test, owner))
}

fn keep_unshadowed(test: &PythonTest, mut spellings: Vec<String>) -> Vec<String> {
    spellings.retain(|spelling| {
        let root = spelling.split('.').next().unwrap_or(spelling);
        !test.constant_rebinding.rebinds(root)
            && !test.constant_rebinding.module_rebinds(root)
            && !test.fixtures.iter().any(|fixture| fixture == root)
    });
    spellings.sort();
    spellings.dedup();
    spellings
}

/// The condition of a bare `assert <condition>` assertion.
fn assertion_expression(assertion: &PythonAssertion) -> Option<Expr> {
    let text = assertion.gate_text.trim();
    if !text.starts_with("assert ") {
        return None;
    }
    let Mod::Module(module) = parse_module_result(Path::new("assertion.py"), text).ok()? else {
        return None;
    };
    match module.body.as_slice() {
        [Stmt::Assert(assert)] => Some(assert.test.as_ref().clone()),
        _ => None,
    }
}

/// Statement words that can leave a test before a later assertion runs, or
/// run it conditionally.
const GUARDING_STATEMENTS: [&str; 22] = [
    "return", "raise", "yield", "if", "elif", "else", "try", "except", "finally", "with", "for",
    "while", "del", "global", "nonlocal", "def", "class", "lambda", "break", "continue", "match",
    "async",
];

/// Words that end or skip a test when called.
const LEAVING_CALLS: [&str; 12] = [
    "skipTest",
    "SkipTest",
    "quit",
    "pytest_exit",
    "Exit",
    "skip",
    "xfail",
    "importorskip",
    "exit",
    "_exit",
    "abort",
    "fail",
];

/// Whether the assertion is a direct statement of the test body that every run
/// reaches: nothing before it returns, raises, branches, loops, enters a
/// context or calls a skipping function.
fn straight_line_assertion(test: &PythonTest, assertion: &PythonAssertion) -> bool {
    // A call of a generator test does not execute its body.
    if rustpython_parser::lexer::lex(&test.body_text, rustpython_parser::Mode::Module).any(
        |token| match token {
            Ok((rustpython_parser::Tok::Yield, _)) | Err(_) => true,
            // The lexer keeps formatted expressions inside a string token;
            // they may contain `yield` before or after this assertion.
            Ok((rustpython_parser::Tok::String { kind, .. }, _)) => kind.is_any_fstring(),
            _ => false,
        },
    ) {
        return false;
    }
    let lines: Vec<&str> = test.body_text.lines().collect();
    let Some(header) = lines.iter().position(|line| {
        let line = line.trim_start();
        line.starts_with("def ") || line.starts_with("async def ")
    }) else {
        return false;
    };
    if !lines[header].trim_end().ends_with(':') {
        return false;
    }
    let Some(index) = assertion.line.checked_sub(test.line) else {
        return false;
    };
    if index <= header || index >= lines.len() {
        return false;
    }
    let indent = |line: &str| line.len() - line.trim_start().len();
    let statement = |line: &&str| {
        let trimmed = line.trim_start();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    };
    let Some(base) = lines[header + 1..]
        .iter()
        .copied()
        .find(statement)
        .map(indent)
    else {
        return false;
    };
    let target = lines[index];
    if indent(target) != base || !target.trim_start().starts_with("assert ") {
        return false;
    }
    // Bind the extracted assertion to the statement that actually starts this
    // physical line; an earlier `assert False;` must not stand in for it.
    // Semantic gate text may be normalized by the boolean-oracle pass.
    let Some(first_line) = assertion.text.lines().next() else {
        return false;
    };
    let Some(tail) = target.trim_start().strip_prefix(first_line.trim()) else {
        return false;
    };
    if !tail.trim_start().is_empty() && !tail.trim_start().starts_with('#') {
        return false;
    }
    lines[header + 1..index].iter().all(|line| {
        let trimmed = line.trim_start();
        if !statement(line) {
            return true;
        }
        // A continuation line can finish a base-suite statement and then
        // return (`value = (\n        1); return`). Its physical indentation
        // does not make the trailing statement a nested suite.
        let guarding = trimmed.split(';').any(|part| {
            let first_word: String = part
                .trim_start()
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            GUARDING_STATEMENTS.contains(&first_word.as_str())
        });
        !guarding
            && !LEAVING_CALLS
                .iter()
                .any(|word| mentions_identifier(trimmed, word))
    })
}

fn evaluate(expr: &Expr, scope: &Scope<'_>) -> Option<Value> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(value) => value.to_string().parse().ok().map(Value::Int),
            ast::Constant::Bool(value) => Some(Value::Bool(*value)),
            ast::Constant::Str(value) => Some(Value::Str(value.clone())),
            _ => None,
        },
        Expr::Name(name) => match scope.parameters {
            Some(parameters) => parameters
                .iter()
                .find(|(parameter, _)| parameter == name.id.as_str())
                .map(|(_, value)| value.clone())
                .or_else(|| (name.id.as_str() == scope.constant).then(|| scope.value.clone())),
            None => scope
                .constant_spellings
                .iter()
                .any(|spelling| spelling == name.id.as_str())
                .then(|| scope.value.clone()),
        },
        Expr::Attribute(_) if scope.parameters.is_none() => {
            let full = expr_full_name(expr)?;
            scope
                .constant_spellings
                .contains(&full)
                .then(|| scope.value.clone())
        }
        Expr::UnaryOp(unary) => {
            let operand = evaluate(&unary.operand, scope)?;
            match unary.op {
                ast::UnaryOp::Not => Some(Value::Bool(!operand.truthy())),
                ast::UnaryOp::USub => operand.number()?.checked_neg().map(Value::Int),
                ast::UnaryOp::UAdd => operand.number().map(Value::Int),
                ast::UnaryOp::Invert => None,
            }
        }
        Expr::BinOp(binary) => {
            let left = evaluate(&binary.left, scope)?;
            let right = evaluate(&binary.right, scope)?;
            arithmetic(&binary.op, &left, &right)
        }
        Expr::BoolOp(bool_op) => {
            let mut last = None;
            for operand in &bool_op.values {
                let value = evaluate(operand, scope)?;
                let stops = match bool_op.op {
                    ast::BoolOp::And => !value.truthy(),
                    ast::BoolOp::Or => value.truthy(),
                };
                if stops {
                    return Some(value);
                }
                last = Some(value);
            }
            last
        }
        Expr::IfExp(conditional) => {
            if evaluate(&conditional.test, scope)?.truthy() {
                evaluate(&conditional.body, scope)
            } else {
                evaluate(&conditional.orelse, scope)
            }
        }
        Expr::Compare(compare) => {
            let mut left = evaluate(&compare.left, scope)?;
            for (op, comparator) in compare.ops.iter().zip(&compare.comparators) {
                let right = evaluate(comparator, scope)?;
                if !comparison(op, &left, &right)? {
                    return Some(Value::Bool(false));
                }
                left = right;
            }
            Some(Value::Bool(true))
        }
        Expr::Call(call) if scope.parameters.is_none() => {
            let callee = expr_full_name(&call.func)?;
            if !call.keywords.is_empty() || !scope.reader_spellings.contains(&callee) {
                return None;
            }
            let reader = scope.reader?;
            if call.args.len() != reader.parameters.len() {
                return None;
            }
            let arguments: Vec<(String, Value)> = reader
                .parameters
                .iter()
                .zip(&call.args)
                .map(|(parameter, argument)| Some((parameter.clone(), evaluate(argument, scope)?)))
                .collect::<Option<_>>()?;
            let inner = Scope {
                constant: scope.constant,
                value: scope.value,
                parameters: Some(&arguments),
                constant_spellings: &[],
                reader_spellings: &[],
                reader: None,
            };
            evaluate(&reader.expression, &inner)
        }
        _ => None,
    }
}

fn arithmetic(op: &ast::Operator, left: &Value, right: &Value) -> Option<Value> {
    if let (Value::Str(left), Value::Str(right), ast::Operator::Add) = (left, right, op) {
        return Some(Value::Str(format!("{left}{right}")));
    }
    let (left, right) = (left.number()?, right.number()?);
    match op {
        ast::Operator::Add => left.checked_add(right),
        ast::Operator::Sub => left.checked_sub(right),
        ast::Operator::Mult => left.checked_mul(right),
        ast::Operator::FloorDiv => {
            if right == 0 {
                return None;
            }
            let quotient = left.checked_div(right)?;
            let adjust = left.checked_rem(right)? != 0 && ((left < 0) != (right < 0));
            quotient.checked_sub(i128::from(adjust))
        }
        ast::Operator::Mod => {
            if right == 0 {
                return None;
            }
            let remainder = left.checked_rem(right)?;
            if remainder != 0 && ((remainder < 0) != (right < 0)) {
                remainder.checked_add(right)
            } else {
                Some(remainder)
            }
        }
        _ => None,
    }
    .map(Value::Int)
}

fn comparison(op: &ast::CmpOp, left: &Value, right: &Value) -> Option<bool> {
    let ordering = match (left, right) {
        (Value::Str(left), Value::Str(right)) => Some(left.cmp(right)),
        _ => left
            .number()
            .zip(right.number())
            .map(|(left, right)| left.cmp(&right)),
    };
    match op {
        ast::CmpOp::Eq => Some(ordering == Some(std::cmp::Ordering::Equal)),
        ast::CmpOp::NotEq => Some(ordering != Some(std::cmp::Ordering::Equal)),
        ast::CmpOp::Lt => Some(ordering? == std::cmp::Ordering::Less),
        ast::CmpOp::LtE => Some(ordering? != std::cmp::Ordering::Greater),
        ast::CmpOp::Gt => Some(ordering? == std::cmp::Ordering::Greater),
        ast::CmpOp::GtE => Some(ordering? != std::cmp::Ordering::Less),
        _ => None,
    }
}
