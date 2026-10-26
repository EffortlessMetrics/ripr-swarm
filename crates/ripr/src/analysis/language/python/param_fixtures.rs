//! A conftest or module fixture whose literal `params=` rows reach the test
//! through `request.param` (#9619, `py-fixtures-fee-boundary`).
//!
//! ```python
//! @pytest.fixture(params=[(99, 5), (100, 0)], ids=["below", "at"])
//! def fee_case(request):
//!     return request.param
//!
//! def test_handling_fee_boundary(fee_case):
//!     amount, fee = fee_case
//!     assert handling_fee(amount) == fee
//! ```
//!
//! pytest runs the test once per row, and the leading unpack binds the row's
//! values, so the test is the same as
//! `@pytest.mark.parametrize("amount, fee", [(99, 5), (100, 0)])` over a body
//! without the unpack. That test is what the workspace pass hands the
//! classifier: the rows become the test's parametrize cases and the unpack
//! statement is blanked from its body text (offsets kept), so every
//! downstream reading sees argnames bound once by pytest.
//!
//! Only shapes whose generated values are statically certain are read:
//!
//! - the fixture's only decorator is `@pytest.fixture(params=<rows>)` with at
//!   most a literal `ids=` list of strings; no `scope=`, `autouse=`, `name=`;
//! - the fixture takes exactly `request` and its body is an optional docstring
//!   and `return request.param`;
//! - every row is a scalar literal or a tuple of scalar literals. A list row
//!   is refused: it is mutable, and a fixture imported from outside the read
//!   scope could change it through the request before the test runs;
//! - the fixture is isolated the way `fixtures.rs` isolates a literal-built
//!   instance (nothing else in the test's module or conftest chain names it,
//!   overrides it, walks the request or runs a hook);
//! - the test is a module-level pytest test with no parametrize mark, no
//!   decorator other than a pytest mark, and takes the fixture as an injected
//!   parameter; its first statement (after a docstring) is `name = fixture`
//!   or `a, b = fixture` with every row of the target's arity; the rest of the
//!   body never names the fixture and never rebinds a target name.
//!
//! Anything else leaves the test as extracted, and the
//! `unresolved_pytest_fixture` limit stands.

use super::fixtures::{injected_parameter, is_literal_argument, module_binding_count};
use super::module_constants::mentions_identifier;
use super::owners_tests::decorator_names;
use super::parametrize::PythonParametrizeCases;
use super::related_tests::text_binds_name;
use super::source_utils::text_for_range;
use super::{PythonFramework, PythonTest};
use rustpython_parser::ast::{Constant, Expr, Mod, Ranged, Stmt};
use rustpython_parser::{Mode, parse};
use std::collections::BTreeMap;

/// One literal `params=` row: its source text and, for a tuple row, the
/// source text of each element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FixtureParamRow {
    pub(super) text: String,
    pub(super) elements: Option<Vec<String>>,
}

/// The rows of a fixture that hands each test `request.param` from a literal
/// `params=` list, or `None` for any other shape.
pub(super) fn literal_params(
    stmt: &Stmt,
    statements: &[Stmt],
    source: &str,
    is_fixture_name: impl Fn(&str) -> bool,
) -> Option<Vec<FixtureParamRow>> {
    let Stmt::FunctionDef(function) = stmt else {
        return None;
    };
    let [Expr::Call(call)] = function.decorator_list.as_slice() else {
        return None;
    };
    if !call.args.is_empty()
        || !decorator_names(std::slice::from_ref(&*call.func))
            .iter()
            .any(|name| is_fixture_name(name))
    {
        return None;
    }
    let mut params = None;
    for keyword in &call.keywords {
        match keyword.arg.as_ref().map(|arg| arg.as_str()) {
            Some("params") => params = Some(&keyword.value),
            // Ids only name the generated tests; a callable could run code.
            Some("ids") if literal_strings(&keyword.value) => {}
            _ => return None,
        }
    }
    let rows = match params? {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    let args = &function.args;
    let [request] = args.args.as_slice() else {
        return None;
    };
    if request.def.arg.as_str() != "request"
        || request.default.is_some()
        || !args.posonlyargs.is_empty()
        || args.vararg.is_some()
        || !args.kwonlyargs.is_empty()
        || args.kwarg.is_some()
    {
        return None;
    }
    let body = match function.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..] if is_string(&docstring.value) => rest,
        body => body,
    };
    let [Stmt::Return(returned)] = body else {
        return None;
    };
    let Some(Expr::Attribute(attribute)) = returned.value.as_deref() else {
        return None;
    };
    if attribute.attr.as_str() != "param"
        || !matches!(&*attribute.value, Expr::Name(name) if name.id.as_str() == "request")
    {
        return None;
    }
    if module_binding_count(statements, source, function.name.as_str()) != Some(1) {
        return None;
    }
    let text = |expr: &Expr| text_for_range(source, expr.range()).trim().to_string();
    let rows: Option<Vec<FixtureParamRow>> = rows
        .iter()
        .map(|row| match row {
            Expr::Tuple(tuple) if tuple.elts.iter().all(is_literal_argument) => {
                Some(FixtureParamRow {
                    text: text(row),
                    elements: Some(tuple.elts.iter().map(text).collect()),
                })
            }
            row if is_literal_argument(row) => Some(FixtureParamRow {
                text: text(row),
                elements: None,
            }),
            _ => None,
        })
        .collect();
    rows.filter(|rows| !rows.is_empty())
}

fn is_string(expr: &Expr) -> bool {
    matches!(expr, Expr::Constant(constant) if matches!(constant.value, Constant::Str(_)))
}

fn literal_strings(expr: &Expr) -> bool {
    match expr {
        Expr::List(list) => list.elts.iter().all(is_string),
        Expr::Tuple(tuple) => tuple.elts.iter().all(is_string),
        _ => false,
    }
}

/// Rewrite each test that unpacks an isolated literal-params fixture into
/// the parametrized test it is (module docs). Runs after
/// `mark_isolated_fixture_defs`.
pub(super) fn apply_literal_param_fixtures(tests: &mut [PythonTest]) {
    for test in tests {
        if let Some((cases, unpack)) = literal_param_cases(test) {
            blank_range(&mut test.body_text, unpack);
            test.parametrize = Some(cases);
        }
    }
}

fn literal_param_cases(
    test: &PythonTest,
) -> Option<(PythonParametrizeCases, std::ops::Range<usize>)> {
    if test.framework != PythonFramework::Pytest
        || test.parametrized
        || test.parametrize.is_some()
        || !test.generated_inputs.is_empty()
        || test.qualified_name != test.name
        // A test decorator other than a pytest mark may swap the argument.
        || !test
            .decorators
            .iter()
            .all(|decorator| decorator.starts_with("pytest.mark."))
    {
        return None;
    }
    let Ok(Mod::Module(module)) = parse(&test.body_text, Mode::Module, "<python-test-body>") else {
        return None;
    };
    let Some(Stmt::FunctionDef(function)) = module.body.first() else {
        return None;
    };
    let (unpack, rest) = match function.body.as_slice() {
        [Stmt::Expr(docstring), unpack, rest @ ..] if is_string(&docstring.value) => (unpack, rest),
        [unpack, rest @ ..] => (unpack, rest),
        [] => return None,
    };
    let Stmt::Assign(assign) = unpack else {
        return None;
    };
    let ([target], Expr::Name(source)) = (assign.targets.as_slice(), &*assign.value) else {
        return None;
    };
    let fixture = source.id.as_str();
    // Only a bare name takes the whole row; `x, = fixture` unpacks it.
    let bare = matches!(target, Expr::Name(_));
    let names: Vec<String> = match target {
        Expr::Name(name) => vec![name.id.to_string()],
        Expr::Tuple(tuple) => tuple
            .elts
            .iter()
            .map(|element| match element {
                Expr::Name(name) => Some(name.id.to_string()),
                _ => None,
            })
            .collect::<Option<_>>()?,
        _ => return None,
    };
    let distinct: std::collections::BTreeSet<&String> = names.iter().collect();
    if distinct.len() != names.len()
        || names
            .iter()
            .any(|name| name == fixture || test.fixtures.contains(name))
        || !test.fixtures.iter().any(|parameter| parameter == fixture)
        || !injected_parameter(test, fixture)
    {
        return None;
    }
    let def = test.fixture_defs.iter().find(|def| def.name == fixture)?;
    let rows = def.literal_params.as_ref()?;
    let rebinding = &test.constant_rebinding;
    if !def.isolated
        || (def.file != test.file
            && (rebinding.module_opaque
                || rebinding.module_bound.iter().any(|name| name == fixture)))
    {
        return None;
    }
    let range_of = |stmt: &Stmt| usize::from(stmt.range().start())..usize::from(stmt.range().end());
    // The unpack is alone on its lines but for a comment (no `;` neighbour,
    // no one-line def),
    // so blanking it leaves a body that still parses.
    let span = range_of(unpack);
    let line_start = test.body_text[..span.start]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let line_end = test.body_text[span.end..]
        .find('\n')
        .map_or(test.body_text.len(), |at| span.end + at);
    if !test.body_text[line_start..span.start].trim().is_empty()
        || !{
            let after = test.body_text[span.end..line_end].trim();
            after.is_empty() || after.starts_with('#')
        }
    {
        return None;
    }
    // Every later statement, nested scopes included, must leave the fixture
    // and the unpacked names alone.
    for stmt in rest {
        let text = test.body_text.get(range_of(stmt))?;
        if mentions_identifier(text, fixture)
            || names.iter().any(|name| text_binds_name(text, name))
        {
            return None;
        }
    }
    let cases: Option<Vec<BTreeMap<String, String>>> = rows
        .iter()
        .map(|row| match (names.as_slice(), &row.elements) {
            ([name], _) if bare => Some(BTreeMap::from([(name.clone(), row.text.clone())])),
            (names, Some(elements)) if elements.len() == names.len() => Some(
                names
                    .iter()
                    .cloned()
                    .zip(elements.iter().cloned())
                    .collect(),
            ),
            _ => None,
        })
        .collect();
    let cases = PythonParametrizeCases { cases: cases? }.excluding_body_bindings(rest)?;
    names
        .iter()
        .all(|name| cases.binds(name))
        .then(|| (cases, range_of(unpack)))
}

/// Replace a statement's text with spaces, keeping every byte offset and
/// line number of the rest of the body.
fn blank_range(text: &mut String, range: std::ops::Range<usize>) {
    let Some(span) = text.get(range.clone()) else {
        return;
    };
    let blanked: String = span
        .chars()
        .map(|ch| if ch == '\n' { '\n' } else { ' ' })
        .collect();
    // Multi-byte characters become one space each; pad to keep offsets.
    let blanked = format!("{blanked:<width$}", width = span.len());
    text.replace_range(range, &blanked);
}
