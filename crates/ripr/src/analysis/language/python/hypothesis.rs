//! Hypothesis `@given` inputs and `@example` rows for one test function
//! (#6601).
//!
//! `@given(...)` supplies test parameters from strategies, so they are not
//! pytest fixtures: naming them `unresolved_pytest_fixture` sends the user to
//! a conftest that does not exist. Positional strategies fill the rightmost
//! positional parameters; keyword strategies fill the parameters they name.
//!
//! `@example(...)` rows run only when Hypothesis's effective settings still
//! include `Phase.explicit`; otherwise they are withheld. Positional
//! values fill the rightmost parameters, as many as there are values (as
//! Hypothesis zips them with the tail of the signature); keyword values fill
//! the parameters they name. A chained example
//! (`@example(1).xfail()`, `.via(...)`) is skipped: it may be expected to
//! fail, and any example that is skipped only removes a row.

use super::PythonImport;
use super::expr_full_name;
use super::parametrize::PythonParametrizeCases;
use super::source_utils::text_for_range;
use rustpython_parser::ast::{self, Expr, Ranged};
use std::collections::{BTreeMap, BTreeSet};

/// Hypothesis phase names ripr can read statically from `Phase.*` or string
/// literals in a `phases=` list (#10752).
const STATIC_PHASE_NAMES: &[&str] = &["explicit", "reuse", "generate", "shrink", "target"];

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

/// The `phases=` value of a `settings(...)` call, or `Err(())` when a `**`
/// keyword expansion could supply it (unreadable, so the caller fails closed).
fn settings_phases(call: &ast::ExprCall) -> Result<Option<&Expr>, ()> {
    if call.keywords.iter().any(|keyword| keyword.arg.is_none()) {
        return Err(());
    }
    Ok(call
        .keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|arg| arg.as_str() == "phases")
        })
        .map(|keyword| &keyword.value))
}

/// Hypothesis's `settings`, bare, through the module, or imported under an
/// alias (`from hypothesis import settings as config`).
fn is_settings_decorator(name: &str, imports: &[PythonImport]) -> bool {
    is_hypothesis_decorator(name, "settings")
        || imports.iter().rev().any(|import| {
            import.alias == name
                && import.source_module == "hypothesis"
                && import.imported == "settings"
        })
}

fn resolves_to_hypothesis_module(name: &str, imports: &[PythonImport]) -> bool {
    if name == "hypothesis" {
        return true;
    }
    imports.iter().rev().any(|import| {
        import.alias == name && import.source_module.is_empty() && import.imported == "hypothesis"
    })
}

fn resolves_to_hypothesis_phase_type(type_name: &str, imports: &[PythonImport]) -> bool {
    if type_name == "hypothesis.Phase" {
        return true;
    }
    if let Some(module) = type_name.strip_suffix(".Phase") {
        return resolves_to_hypothesis_module(module, imports);
    }
    imports.iter().rev().any(|import| {
        import.alias == type_name
            && import.source_module == "hypothesis"
            && import.imported == "Phase"
    })
}

fn static_phase_member(expr: &Expr, imports: &[PythonImport]) -> Option<String> {
    let full = expr_full_name(expr)?;
    let (type_name, member) = full.rsplit_once('.')?;
    if !STATIC_PHASE_NAMES.contains(&member) {
        return None;
    }
    resolves_to_hypothesis_phase_type(type_name, imports).then(|| member.to_string())
}

fn static_phase_list(expr: &Expr, imports: &[PythonImport]) -> Option<BTreeSet<String>> {
    let elements = match expr {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    let mut phases = BTreeSet::new();
    for element in elements {
        if let Some(name) = static_phase_member(element, imports) {
            phases.insert(name);
            continue;
        }
        let Expr::Constant(constant) = element else {
            return None;
        };
        let ast::Constant::Str(text) = &constant.value else {
            return None;
        };
        if STATIC_PHASE_NAMES.contains(&text.as_str()) {
            phases.insert(text.to_string());
        } else {
            return None;
        }
    }
    Some(phases)
}

/// Whether `@example` rows still run: false when any `@settings(phases=...)`
/// on the test statically omits `Phase.explicit`, or when its phases cannot be
/// read conservatively (unresolved names, `**` expansion).
fn explicit_examples_enabled(decorators: &[Expr], imports: &[PythonImport]) -> bool {
    for decorator in decorators {
        let Expr::Call(call) = decorator else {
            continue;
        };
        if !expr_full_name(&call.func).is_some_and(|name| is_settings_decorator(&name, imports)) {
            continue;
        }
        let Ok(phases) = settings_phases(call) else {
            return false;
        };
        let Some(phases_expr) = phases else {
            continue;
        };
        let Some(phases) = static_phase_list(phases_expr, imports) else {
            return false;
        };
        if !phases.contains("explicit") {
            return false;
        }
    }
    true
}

/// The literal-or-source rows of a `@given` test's `@example(...)`
/// decorators, as parametrize-style cases, or None when there is no readable
/// example or `@settings` withholds the explicit phase.
pub(super) fn example_cases(
    source: &str,
    args: &ast::Arguments,
    decorators: &[Expr],
    definition_imports: &[PythonImport],
) -> Option<PythonParametrizeCases> {
    given_call(decorators)?;
    if !explicit_examples_enabled(decorators, definition_imports) {
        return None;
    }
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

    fn examples(source: &str, imports: &[PythonImport]) -> Option<Vec<Vec<(String, String)>>> {
        let f = function(source)?;
        example_cases(source, &f.args, &f.decorator_list, imports).map(|found| {
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

    fn phase_imports() -> Vec<PythonImport> {
        vec![PythonImport {
            imported: "Phase".to_string(),
            alias: "Phase".to_string(),
            source_module: "hypothesis".to_string(),
        }]
    }

    #[test]
    fn example_rows_bind_the_given_parameters() {
        let source = "@given(st.integers(0, 100))\n@example(49)\n@example(score=50)\ndef test_a(score):\n    pass\n";
        assert_eq!(
            examples(source, &[]),
            Some(vec![
                vec![("score".to_string(), "49".to_string())],
                vec![("score".to_string(), "50".to_string())],
            ])
        );
    }

    #[test]
    fn chained_examples_and_examples_without_given_record_no_row() {
        assert_eq!(
            examples(
                "@given(st.integers())\n@example(1).xfail()\ndef test_a(x):\n    pass\n",
                &[]
            ),
            None
        );
        assert_eq!(
            examples("@example(1)\ndef test_a(x):\n    pass\n", &[]),
            None
        );
        assert_eq!(
            examples(
                "@given(st.integers())\n@example(1, 2)\ndef test_a(x):\n    pass\n",
                &[]
            ),
            None
        );
    }

    #[test]
    fn positional_example_values_fill_the_rightmost_parameters() {
        // Hypothesis zips `@example(5)` with the last parameter, whatever
        // `@given` supplies.
        assert_eq!(
            examples(
                "@given(st.integers(), st.integers())\n@example(5)\ndef test_a(lo, hi):\n    pass\n",
                &[],
            ),
            Some(vec![vec![("hi".to_string(), "5".to_string())]])
        );
    }

    #[test]
    fn examples_withheld_when_settings_omit_explicit_phase() {
        let source = "@settings(phases=[Phase.generate])\n@given(st.integers(0, 100))\n@example(49)\ndef test_a(score):\n    pass\n";
        assert_eq!(examples(source, &phase_imports()), None);
    }

    #[test]
    fn examples_admitted_when_settings_include_explicit_phase() {
        let source = "@settings(phases=[Phase.generate, Phase.explicit])\n@given(st.integers(0, 100))\n@example(49)\ndef test_a(score):\n    pass\n";
        assert_eq!(
            examples(source, &phase_imports()),
            Some(vec![vec![("score".to_string(), "49".to_string())]])
        );
    }

    #[test]
    fn settings_below_given_still_governs_examples() {
        let source = "@given(st.integers(0, 100))\n@settings(phases=[Phase.generate])\n@example(49)\ndef test_a(score):\n    pass\n";
        assert_eq!(examples(source, &phase_imports()), None);
    }

    #[test]
    fn unresolved_phases_withhold_examples() {
        let source = "@settings(phases=enabled_phases())\n@given(st.integers())\n@example(1)\ndef test_a(x):\n    pass\n";
        assert_eq!(examples(source, &[]), None);
    }

    #[test]
    fn shadowed_phase_name_withholds_examples() {
        let source = "@settings(phases=[Phase.generate])\n@given(st.integers())\n@example(1)\ndef test_a(x):\n    pass\n";
        assert_eq!(examples(source, &[]), None);
    }

    #[test]
    fn aliased_settings_decorator_governs_examples() {
        let source = "@config(phases=[Phase.generate])\n@given(st.integers(0, 100))\n@example(50)\ndef test_a(score):\n    pass\n";
        let mut imports = phase_imports();
        imports.push(PythonImport {
            imported: "settings".to_string(),
            alias: "config".to_string(),
            source_module: "hypothesis".to_string(),
        });
        assert_eq!(examples(source, &imports), None);
    }

    #[test]
    fn expanded_settings_keywords_withhold_examples() {
        let source = "@settings(**{'phases': [Phase.generate]})\n@given(st.integers(0, 100))\n@example(50)\ndef test_a(score):\n    pass\n";
        assert_eq!(examples(source, &phase_imports()), None);
    }

    #[test]
    fn changing_only_settings_phases_toggles_example_rows() {
        let base = "@given(st.integers(0, 100))\n@example(50)\ndef test_a(score):\n    pass\n";
        let disabled = format!("@settings(phases=[Phase.generate])\n{base}");
        let enabled = format!("@settings(phases=[Phase.explicit, Phase.generate])\n{base}");
        let imports = phase_imports();
        assert_eq!(examples(&disabled, &imports), None);
        assert_eq!(
            examples(&enabled, &imports),
            Some(vec![vec![("score".to_string(), "50".to_string())]])
        );
    }
}
