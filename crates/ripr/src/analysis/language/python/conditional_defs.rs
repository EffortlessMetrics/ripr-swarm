//! Functions defined under a statically decided module-level `if` (#9423).
//!
//! `if sys.version_info >= (3, 0): def versioned(): ...` defines `versioned`
//! on every Python 3 interpreter, so a test that calls it runs that body. The
//! owner walk only enumerates top-level definitions, which left the edit with
//! the `<module>` owner and an abstention. This module names the definitions
//! a decided condition makes certain, so they become ordinary function owners.
//!
//! Only a version comparison whose answer does not depend on the interpreter's
//! minor version decides a condition, and only when `sys` is the module's one
//! plain `import sys`. A definition is an owner only when its name has no
//! other live binding: the dead arms of the decided conditions may redefine
//! it (that is the point of the idiom), nothing else may. Everything else
//! (an undecided condition, a `try`/`except ImportError` fallback, a nested
//! arm, a class) keeps the `<module>` owner and its abstention.

use super::module_constants::ModuleBindingFacts;
use rustpython_parser::ast::{self, Expr, Stmt};

/// The definitions the module's decided `if` statements make certain, in
/// source order.
pub(super) fn live_conditional_functions<'a>(source: &str, module: &'a [Stmt]) -> Vec<&'a Stmt> {
    if !module.iter().any(|stmt| matches!(stmt, Stmt::If(_))) {
        return Vec::new();
    }
    let facts = ModuleBindingFacts::of(source, module);
    // `sys` must be the module's single plain `import sys`, so the condition
    // reads the real interpreter version and nothing shadows it.
    let plain_sys = module.iter().any(|stmt| match stmt {
        Stmt::Import(import) => import
            .names
            .iter()
            .any(|alias| alias.name.as_str() == "sys" && alias.asname.is_none()),
        _ => false,
    });
    if !plain_sys || !facts.bound_once("sys") {
        return Vec::new();
    }
    let mut view: Vec<Stmt> = Vec::new();
    let mut defs: Vec<&'a Stmt> = Vec::new();
    for stmt in module {
        match stmt {
            Stmt::If(if_stmt) => match live_arm(if_stmt) {
                Some(arm) => {
                    for inner in arm {
                        view.push(inner.clone());
                        if matches!(inner, Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_)) {
                            defs.push(inner);
                        }
                    }
                }
                None => view.push(stmt.clone()),
            },
            other => view.push(other.clone()),
        }
    }
    if defs.is_empty() {
        return Vec::new();
    }
    // A name the live view binds more than once, or that a dynamic writer or
    // `global` can rebind, is not certain.
    let live_facts = ModuleBindingFacts::of(source, &view);
    defs.retain(|def| {
        let name = match def {
            Stmt::FunctionDef(function) => function.name.as_str(),
            Stmt::AsyncFunctionDef(function) => function.name.as_str(),
            _ => return false,
        };
        live_facts.bound_once(name)
    });
    defs
}

/// The statements of the arm a decided condition chain certainly runs.
fn live_arm(if_stmt: &ast::StmtIf) -> Option<&[Stmt]> {
    match decided_version_condition(&if_stmt.test)? {
        true => Some(if_stmt.body.as_slice()),
        false => match if_stmt.orelse.as_slice() {
            [Stmt::If(next)] => live_arm(next),
            arm => Some(arm),
        },
    }
}

/// Whether a version comparison is decided on every Python 3 interpreter:
/// `sys.version_info >= (3,)` is true, `< (3,)` is false, and a comparison
/// against a Python 2 tuple is decided by the major version alone. A
/// comparison against `(3, 5)` or a later tuple depends on the minor version
/// and stays undecided.
fn decided_version_condition(test: &Expr) -> Option<bool> {
    let Expr::Compare(compare) = test else {
        return None;
    };
    let ([op], [Expr::Tuple(tuple)]) = (compare.ops.as_slice(), compare.comparators.as_slice())
    else {
        return None;
    };
    let Expr::Attribute(attribute) = compare.left.as_ref() else {
        return None;
    };
    let is_version_info = attribute.attr.as_str() == "version_info"
        && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "sys");
    if !is_version_info {
        return None;
    }
    let parts: Option<Vec<i64>> = tuple
        .elts
        .iter()
        .map(|element| match element {
            Expr::Constant(ast::ExprConstant {
                value: ast::Constant::Int(value),
                ..
            }) => value.to_string().parse::<i64>().ok(),
            _ => None,
        })
        .collect();
    let parts = parts?;
    let (major, rest) = parts.split_first()?;
    if *major == 3 && rest.iter().all(|part| *part == 0) {
        // `(3,)`, `(3, 0)`, `(3, 0, 0)`: every Python 3 is at least this.
        return match op {
            ast::CmpOp::GtE => Some(true),
            ast::CmpOp::Lt => Some(false),
            _ => None,
        };
    }
    if *major <= 2 && !parts.is_empty() {
        // Python 3 is above every Python 2 tuple.
        return match op {
            ast::CmpOp::Gt | ast::CmpOp::GtE => Some(true),
            ast::CmpOp::Lt | ast::CmpOp::LtE => Some(false),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::language::python::source_facts::parse_module_result;
    use std::path::Path;

    type Check = Result<(), String>;

    fn names(source: &str) -> Result<Vec<String>, String> {
        let module = match parse_module_result(Path::new("m.py"), source)? {
            ast::Mod::Module(module) => module,
            _ => return Err("expected a module".to_string()),
        };
        Ok(live_conditional_functions(source, &module.body)
            .into_iter()
            .filter_map(|stmt| match stmt {
                Stmt::FunctionDef(function) => Some(function.name.to_string()),
                _ => None,
            })
            .collect())
    }

    #[test]
    fn decided_version_conditions_name_the_live_definition() -> Check {
        let live = "import sys\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\nelse:\n    def f():\n        return 2\n";
        assert_eq!(names(live)?, ["f"]);
        let dead_first = "import sys\n\nif sys.version_info < (3,):\n    def f():\n        return 2\nelse:\n    def f():\n        return 1\n";
        assert_eq!(names(dead_first)?, ["f"]);
        let python_two =
            "import sys\n\nif sys.version_info > (2, 7):\n    def f():\n        return 1\n";
        assert_eq!(names(python_two)?, ["f"]);
        Ok(())
    }

    #[test]
    fn undecided_or_shadowed_conditions_name_nothing() -> Check {
        for source in [
            // The answer depends on the minor version.
            "import sys\n\nif sys.version_info >= (3, 8):\n    def f():\n        return 1\n",
            // Not a version comparison.
            "import sys\n\nif sys.platform == 'linux':\n    def f():\n        return 1\n",
            // `sys` is not the plain import.
            "import sys as s\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\n",
            "def sys():\n    pass\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\n",
            // A second live binding of the name.
            "import sys\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\n\nf = wrap(f)\n",
            "import sys\n\ndef f():\n    return 0\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\n",
            // A dynamic writer can rebind the name.
            "import sys\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\n\nglobals()['f'] = g\n",
            // A star import can bind `sys` or the name.
            "from os import *\nimport sys\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\n",
        ] {
            assert!(names(source)?.is_empty(), "{source}");
        }
        Ok(())
    }

    #[test]
    fn the_dead_arm_is_never_named() -> Check {
        let source = "import sys\n\nif sys.version_info >= (3, 0):\n    def f():\n        return 1\nelse:\n    def g():\n        return 2\n";
        assert_eq!(names(source)?, ["f"]);
        Ok(())
    }
}
