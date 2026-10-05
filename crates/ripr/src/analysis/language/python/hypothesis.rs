//! Hypothesis `@given` inputs and `@example` rows for one test function
//! (#6601).
//!
//! `@given(...)` supplies test parameters from strategies, so they are not
//! pytest fixtures: naming them `unresolved_pytest_fixture` sends the user to
//! a conftest that does not exist. Positional strategies fill the rightmost
//! positional parameters; keyword strategies fill the parameters they name.
//!
//! `@example(...)` rows always run, with literal values, so they bind owner
//! inputs the way literal `parametrize` cases do (`parametrize.rs`). Positional
//! values fill the rightmost parameters, as many as there are values (as
//! Hypothesis zips them with the tail of the signature); keyword values fill
//! the parameters they name. A chained example
//! (`@example(1).xfail()`, `.via(...)`) is skipped: it may be expected to
//! fail, and any example that is skipped only removes a row.

use super::expr_full_name;
use super::parametrize::PythonParametrizeCases;
use super::source_utils::text_for_range;
use rustpython_parser::ast::{self, Expr, Ranged};
use std::collections::BTreeMap;

/// Hypothesis's own decorator spelled bare or through the module.
fn is_hypothesis_decorator(name: &str, decorator: &str) -> bool {
    name == decorator || name == format!("hypothesis.{decorator}")
}

/// The `@given(...)` call among a test's decorators.
fn given_call(decorators: &[Expr]) -> Option<&ast::ExprCall> {
    decorators.iter().find_map(|decorator| match decorator {
        Expr::Call(call)
            if expr_full_name(&call.func)
                .is_some_and(|name| is_hypothesis_decorator(&name, "given")) =>
        {
            Some(call)
        }
        _ => None,
    })
}

/// Positional parameters a strategy or example value can fill, without a
/// leading `self` or `cls`.
fn positional_parameters(args: &ast::Arguments) -> Vec<String> {
    args.posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.to_string())
        .filter(|name| name != "self" && name != "cls")
        .collect()
}

/// The positional parameters `@given`'s positional strategies fill: the
/// rightmost ones, in order.
fn given_positional_names(args: &ast::Arguments, given: &ast::ExprCall) -> Option<Vec<String>> {
    if given.args.iter().any(|arg| matches!(arg, Expr::Starred(_))) {
        return None;
    }
    let positional = positional_parameters(args);
    let count = given.args.len();
    (count <= positional.len()).then(|| positional[positional.len() - count..].to_vec())
}

/// Every test parameter `@given` supplies. Empty when the test has no
/// `@given`; when its strategies cannot be paired with parameters (a starred
/// strategy, `**kwargs`), every parameter is treated as supplied so none is
/// mistaken for a fixture.
pub(super) fn given_parameter_names(args: &ast::Arguments, decorators: &[Expr]) -> Vec<String> {
    let Some(given) = given_call(decorators) else {
        return Vec::new();
    };
    let keyword_names = given
        .keywords
        .iter()
        .map(|keyword| keyword.arg.as_ref().map(|arg| arg.to_string()))
        .collect::<Option<Vec<_>>>();
    let mut names = match (given_positional_names(args, given), keyword_names) {
        (Some(mut positional), Some(keywords)) => {
            positional.extend(keywords);
            positional
        }
        _ => positional_parameters(args)
            .into_iter()
            .chain(args.kwonlyargs.iter().map(|arg| arg.def.arg.to_string()))
            .collect(),
    };
    names.sort();
    names.dedup();
    names
}

/// The literal-or-source rows of a `@given` test's `@example(...)`
/// decorators, as parametrize-style cases, or None when there is no readable
/// example.
pub(super) fn example_cases(
    source: &str,
    args: &ast::Arguments,
    decorators: &[Expr],
) -> Option<PythonParametrizeCases> {
    given_call(decorators)?;
    let positional = positional_parameters(args);
    let cases: Vec<BTreeMap<String, String>> = decorators
        .iter()
        .filter_map(|decorator| match decorator {
            Expr::Call(call)
                if matches!(call.func.as_ref(), Expr::Name(_) | Expr::Attribute(_))
                    && expr_full_name(&call.func)
                        .is_some_and(|name| is_hypothesis_decorator(&name, "example")) =>
            {
                example_row(source, call, &positional)
            }
            _ => None,
        })
        .collect();
    (!cases.is_empty()).then_some(PythonParametrizeCases { cases })
}

fn example_row(
    source: &str,
    call: &ast::ExprCall,
    positional: &[String],
) -> Option<BTreeMap<String, String>> {
    if call.args.len() > positional.len()
        || call.args.iter().any(|arg| matches!(arg, Expr::Starred(_)))
    {
        return None;
    }
    let value = |expr: &Expr| text_for_range(source, expr.range()).trim().to_string();
    let mut row: BTreeMap<String, String> = positional[positional.len() - call.args.len()..]
        .iter()
        .cloned()
        .zip(call.args.iter().map(value))
        .collect();
    for keyword in &call.keywords {
        let name = keyword.arg.as_ref()?.to_string();
        if row.insert(name, value(&keyword.value)).is_some() {
            return None;
        }
    }
    (!row.is_empty()).then_some(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpython_parser::ast::Stmt;

    fn function(source: &str) -> Option<ast::StmtFunctionDef> {
        let module =
            super::super::source_facts::parse_module(std::path::Path::new("t.py"), source)?;
        let ast::Mod::Module(module) = module else {
            return None;
        };
        match module.body.into_iter().last() {
            Some(Stmt::FunctionDef(function)) => Some(function),
            _ => None,
        }
    }

    fn given_names(source: &str) -> Vec<String> {
        function(source)
            .map(|f| given_parameter_names(&f.args, &f.decorator_list))
            .unwrap_or_default()
    }

    fn examples(source: &str) -> Option<Vec<Vec<(String, String)>>> {
        let f = function(source)?;
        example_cases(source, &f.args, &f.decorator_list).map(|found| {
            found
                .cases
                .into_iter()
                .map(|case| case.into_iter().collect())
                .collect()
        })
    }

    #[test]
    fn positional_strategies_fill_the_rightmost_parameters() {
        assert_eq!(
            given_names("@given(st.integers())\ndef test_a(tmp_path, x):\n    pass\n"),
            vec!["x".to_string()]
        );
        assert_eq!(
            given_names(
                "@given(st.integers(), st.integers())\ndef test_a(self, lo, hi):\n    pass\n"
            ),
            vec!["hi".to_string(), "lo".to_string()]
        );
    }

    #[test]
    fn keyword_strategies_fill_the_parameters_they_name() {
        assert_eq!(
            given_names("@hypothesis.given(x=st.integers())\ndef test_a(capsys, x):\n    pass\n"),
            vec!["x".to_string()]
        );
    }

    #[test]
    fn a_test_without_given_supplies_nothing() {
        assert!(given_names("@example(1)\ndef test_a(x):\n    pass\n").is_empty());
        assert!(given_names("def test_a(x):\n    pass\n").is_empty());
    }

    #[test]
    fn example_rows_bind_the_given_parameters() {
        let source = "@given(st.integers(0, 100))\n@example(49)\n@example(score=50)\ndef test_a(score):\n    pass\n";
        assert_eq!(
            examples(source),
            Some(vec![
                vec![("score".to_string(), "49".to_string())],
                vec![("score".to_string(), "50".to_string())],
            ])
        );
    }

    #[test]
    fn chained_examples_and_examples_without_given_record_no_row() {
        assert_eq!(
            examples("@given(st.integers())\n@example(1).xfail()\ndef test_a(x):\n    pass\n"),
            None
        );
        assert_eq!(examples("@example(1)\ndef test_a(x):\n    pass\n"), None);
        assert_eq!(
            examples("@given(st.integers())\n@example(1, 2)\ndef test_a(x):\n    pass\n"),
            None
        );
    }

    #[test]
    fn positional_example_values_fill_the_rightmost_parameters() {
        // Hypothesis zips `@example(5)` with the last parameter, whatever
        // `@given` supplies.
        assert_eq!(
            examples(
                "@given(st.integers(), st.integers())\n@example(5)\ndef test_a(lo, hi):\n    pass\n"
            ),
            Some(vec![vec![("hi".to_string(), "5".to_string())]])
        );
    }
}
