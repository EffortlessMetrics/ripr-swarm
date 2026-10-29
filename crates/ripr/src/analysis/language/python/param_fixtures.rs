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
//!   most a literal `ids=` list of strings, one per row; no `scope=`,
//!   `autouse=` or `name=`;
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
    let mut ids = None;
    for keyword in &call.keywords {
        match keyword.arg.as_ref().map(|arg| arg.as_str()) {
            Some("params") => params = Some(&keyword.value),
            // Ids only name the generated tests; a callable could run code.
            Some("ids") => ids = Some(literal_strings(&keyword.value)?),
            _ => return None,
        }
    }
    let rows = match params? {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    // An id list of another length is a collection error: no case runs.
    if ids.is_some_and(|count| count != rows.len()) {
        return None;
    }
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

/// The length of a list or tuple of string literals.
fn literal_strings(expr: &Expr) -> Option<usize> {
    let elements = match expr {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    elements.iter().all(is_string).then_some(elements.len())
}

/// Rewrite each test that unpacks an isolated literal-params fixture into
/// the parametrized test it is (module docs). Runs after
/// `mark_isolated_fixture_defs`.
pub(super) fn apply_literal_param_fixtures(tests: &mut [PythonTest]) {
    for test in tests {
        if let Some((cases, unpack)) = literal_param_cases(test)
            && blank_range(&mut test.body_text, unpack)
        {
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
    // The unpacked fixture must be an isolated literal-param fixture the test
    // requests (checked below), so a test without one skips the parse.
    if !test.fixture_defs.iter().any(|def| {
        def.isolated && def.literal_params.is_some() && test.fixtures.contains(&def.name)
    }) {
        return None;
    }
    let Ok(Mod::Module(module)) = parse(&test.body_text, Mode::Module, "<python-test-body>") else {
        return None;
    };
    let Some(Stmt::FunctionDef(function)) = module.body.first() else {
        return None;
    };
    let unpack = leading_unpack(&function.body, &test.body_text)?;
    let fixture = unpack.source;
    let names = &unpack.names;
    if names
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
    let cases: Option<Vec<BTreeMap<String, String>>> = rows
        .iter()
        .map(|row| match (names.as_slice(), &row.elements) {
            ([name], _) if unpack.bare => Some(BTreeMap::from([(name.clone(), row.text.clone())])),
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
    let cases = PythonParametrizeCases { cases: cases? }.excluding_body_bindings(unpack.rest)?;
    names
        .iter()
        .all(|name| cases.binds(name))
        .then_some((cases, unpack.span))
}

/// A test body's first statement (after a docstring) `name = source` or
/// `a, b = source`, alone on its lines, with the rest of the body leaving
/// `source` and the target names alone.
pub(super) struct LeadingUnpack<'b> {
    /// The unpacked name: a fixture or a parametrize argname.
    pub(super) source: &'b str,
    pub(super) names: Vec<String>,
    /// A bare name target takes the whole row; `x, = source` unpacks it.
    pub(super) bare: bool,
    /// The statement's byte range in the text its ranges index.
    pub(super) span: std::ops::Range<usize>,
    pub(super) rest: &'b [Stmt],
}

/// The [`LeadingUnpack`] of a test body whose statement ranges index `text`.
pub(super) fn leading_unpack<'b>(body: &'b [Stmt], text: &str) -> Option<LeadingUnpack<'b>> {
    let (unpack, rest) = match body {
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
    let source = source.id.as_str();
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
    if distinct.len() != names.len() || names.iter().any(|name| name == source) {
        return None;
    }
    let range_of = |stmt: &Stmt| usize::from(stmt.range().start())..usize::from(stmt.range().end());
    // The unpack is alone on its lines but for a comment (no `;` neighbour,
    // no one-line def), so blanking it leaves a body that still parses.
    let span = range_of(unpack);
    let line_start = text.get(..span.start)?.rfind('\n').map_or(0, |at| at + 1);
    let line_end = text
        .get(span.end..)?
        .find('\n')
        .map_or(text.len(), |at| span.end + at);
    if !text.get(line_start..span.start)?.trim().is_empty() || {
        let after = text.get(span.end..line_end)?.trim();
        !(after.is_empty() || after.starts_with('#'))
    } {
        return None;
    }
    // Every later statement, nested scopes included, must leave the source
    // and the unpacked names alone.
    for stmt in rest {
        let text = text.get(range_of(stmt))?;
        if mentions_identifier(text, source) || names.iter().any(|name| text_binds_name(text, name))
        {
            return None;
        }
    }
    Some(LeadingUnpack {
        source,
        names,
        bare,
        span,
        rest,
    })
}

/// A parametrized test whose first statement unpacks one argname
/// (`amount, fee = fee_case` under `parametrize("fee_case", [(50, 5)])`) is
/// the test parametrized by the unpacked names over a body without the
/// unpack (#10832), the same reading the literal-params fixture gets. The
/// cases and the unpack's range in `source` come back, or None unless every
/// case value is a scalar literal (bare target) or a tuple of scalar
/// literals of the targets' arity, no target is another parameter, and the
/// rest of the body neither names the argname nor rebinds a target. Literal
/// rows keep the read off mutable lists a fixture could change and off a
/// row value that shadows the owner (`quantity, bulk_discount = case`).
pub(super) fn unpacked_parametrize_cases(
    cases: &PythonParametrizeCases,
    body: &[Stmt],
    source: &str,
    parameters: &[String],
) -> Option<(PythonParametrizeCases, std::ops::Range<usize>)> {
    let unpack = leading_unpack(body, source)?;
    let argname = unpack.source;
    if !cases.binds(argname) || unpack.names.iter().any(|name| parameters.contains(name)) {
        return None;
    }
    let unpacked: Option<Vec<BTreeMap<String, String>>> = cases
        .cases
        .iter()
        .map(|case| {
            let value = case.get(argname)?;
            let elements = literal_row_elements(value, unpack.bare)?;
            let elements = if unpack.bare {
                vec![value.clone()]
            } else if elements.len() == unpack.names.len() {
                elements
            } else {
                return None;
            };
            let mut case = case.clone();
            case.remove(argname);
            case.extend(unpack.names.iter().cloned().zip(elements));
            Some(case)
        })
        .collect();
    let cases = PythonParametrizeCases { cases: unpacked? }.excluding_body_bindings(unpack.rest)?;
    unpack
        .names
        .iter()
        .all(|name| cases.binds(name))
        .then_some((cases, unpack.span))
}

/// The element texts of a case value that is a tuple of scalar literals,
/// or, for a bare target, the value itself when it is a scalar literal or
/// such a tuple. Lists, names, calls and starred elements are refused.
fn literal_row_elements(text: &str, bare: bool) -> Option<Vec<String>> {
    let Ok(Mod::Expression(expression)) = parse(text, Mode::Expression, "<parametrize-row>") else {
        return None;
    };
    let value = &*expression.body;
    if bare && is_literal_argument(value) {
        return Some(vec![text.trim().to_string()]);
    }
    let Expr::Tuple(tuple) = value else {
        return None;
    };
    if !tuple.elts.iter().all(is_literal_argument) {
        return None;
    }
    tuple
        .elts
        .iter()
        .map(|element| text.get(usize::from(element.start())..usize::from(element.end())))
        .map(|element| element.map(|element| element.trim().to_string()))
        .collect()
}

/// Replace a statement's text with spaces, keeping every byte offset and
/// line number of the rest of the body.
/// False, leaving the text unchanged, when the range is not in it.
pub(super) fn blank_range(text: &mut String, range: std::ops::Range<usize>) -> bool {
    let Some(span) = text.get(range.clone()) else {
        return false;
    };
    let blanked: String = span
        .chars()
        .map(|ch| if ch == '\n' { '\n' } else { ' ' })
        .collect();
    // Multi-byte characters become one space each; pad to keep offsets.
    let blanked = format!("{blanked:<width$}", width = span.len());
    text.replace_range(range, &blanked);
    true
}
