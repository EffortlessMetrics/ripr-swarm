//! Same-file value helpers a test calls to reach its owner (#15350).
//!
//! A test module often wraps the owner call in a small helper and asserts on
//! what it returns:
//!
//! ```python
//! def parse(input):
//!     from iniconfig._parse import parse_lines
//!     return parse_lines("sample", input.splitlines(True))
//!
//! def test_tokenize(input, expected):
//!     assert parse(input) == expected
//! ```
//!
//! The test's own body never spells `parse_lines(`, so every relation missed
//! it and the change read `no_static_path`. This pass records, for each test,
//! the provable value helpers its body calls, so `related_tests.rs` can relate
//! the test through the helper's body as a reach-only `helper_owner_call`.
//! The relation never carries an oracle: the assertion compares what the
//! helper returns, which is the owner's result only when the helper passes it
//! through unchanged, and nothing here establishes that. So this pass can turn a
//! false `no_static_path` into a weak reading, never into `exposed`.
//!
//! A helper qualifies only when it is a module-level, undecorated, synchronous
//! `def` not named `test*`, the only binding of its name anywhere in the file,
//! whose body (after an optional docstring) is straight-line: imports,
//! assignments and expression statements, ending in `return <value>`. Nested
//! scopes, branches, loops, `with`, `try` and `global` refuse, and so does any
//! `yield`, `lambda` or `:=` spelled anywhere in the body text (a deferred or
//! generator call never runs on the helper's return). A test that binds the
//! helper's name itself (a fixture or a local) does not reach it. The helper's
//! parameters and assignment targets are kept as `local_names`, so a relation
//! never reads `owner(` in the helper as the owner when the helper binds that
//! name itself (`def run(total): return total(1)`).

use super::owners_tests::collect_imports_from_statements;
use super::related_tests::{contains_call_name, text_binds_name};
use super::source_utils::text_for_range;
use super::{PythonImport, PythonTest};
use rustpython_parser::ast::{Expr, Ranged, Stmt, StmtFunctionDef};
use rustpython_parser::text_size::TextRange;
use std::path::Path;

/// One provable value helper: its name, its body as written and the imports
/// its own body makes (a function-local `from M import owner`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonValueHelper {
    pub(super) name: String,
    pub(super) body_text: String,
    pub(super) imports: Vec<PythonImport>,
    /// Every name the helper binds itself: parameters and assignment targets.
    pub(super) local_names: Vec<String>,
}

/// Attach to each test the value helpers its body calls by bare name.
pub(super) fn attach_value_helpers(
    file: &Path,
    source: &str,
    module: &[Stmt],
    tests: &mut [PythonTest],
) {
    if tests.is_empty() {
        return;
    }
    let helpers = collect_value_helpers(file, source, module);
    if helpers.is_empty() {
        return;
    }
    for test in tests {
        test.value_helpers = helpers
            .iter()
            .filter(|helper| {
                contains_call_name(&test.body_text, &helper.name)
                    && !test.fixtures.contains(&helper.name)
                    && !text_binds_name(&test.body_text, &helper.name)
            })
            .cloned()
            .collect();
    }
}

fn collect_value_helpers(file: &Path, source: &str, module: &[Stmt]) -> Vec<PythonValueHelper> {
    module
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::FunctionDef(function) => Some(function),
            _ => None,
        })
        .filter(|function| {
            !function.name.as_str().starts_with("test")
                && function.decorator_list.is_empty()
                && super::bool_oracle::count_bindings(module, function.name.as_str()) == 1
                && straight_line_return(&function.body)
        })
        .map(|function| {
            let body_text = function
                .body
                .first()
                .zip(function.body.last())
                .map(|(first, last)| {
                    text_for_range(
                        source,
                        TextRange::new(first.range().start(), last.range().end()),
                    )
                })
                .unwrap_or_default();
            (function, body_text)
        })
        .filter(|(_, body_text)| {
            !body_text.contains("yield")
                && !body_text.contains("lambda")
                && !body_text.contains(":=")
        })
        .map(|(function, body_text)| PythonValueHelper {
            name: function.name.to_string(),
            body_text,
            imports: collect_imports_from_statements(file, &function.body),
            local_names: local_names(function),
        })
        .collect()
}

/// The body is straight-line and ends in `return <value>`.
fn straight_line_return(body: &[Stmt]) -> bool {
    let Some((Stmt::Return(ret), rest)) = body.split_last() else {
        return false;
    };
    ret.value.is_some()
        && rest.iter().all(|stmt| {
            matches!(
                stmt,
                Stmt::Import(_)
                    | Stmt::ImportFrom(_)
                    | Stmt::Assign(_)
                    | Stmt::AnnAssign(_)
                    | Stmt::Expr(_)
            )
        })
}

/// The helper's parameters and the names its assignments bind.
fn local_names(function: &StmtFunctionDef) -> Vec<String> {
    let args = &function.args;
    let mut names: Vec<String> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
        .map(|arg| arg.def.arg.to_string())
        .chain(args.vararg.iter().map(|arg| arg.arg.to_string()))
        .chain(args.kwarg.iter().map(|arg| arg.arg.to_string()))
        .collect();
    for stmt in &function.body {
        match stmt {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    target_names(target, &mut names);
                }
            }
            Stmt::AnnAssign(assign) => target_names(&assign.target, &mut names),
            _ => {}
        }
    }
    names
}

fn target_names(target: &Expr, names: &mut Vec<String>) {
    match target {
        Expr::Name(name) => names.push(name.id.to_string()),
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| target_names(elt, names)),
        Expr::List(list) => list.elts.iter().for_each(|elt| target_names(elt, names)),
        Expr::Starred(starred) => target_names(&starred.value, names),
        _ => {}
    }
}
