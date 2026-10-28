//! Literal `@pytest.mark.parametrize` cases for one test function (#4559).
//!
//! A parametrized test calls the owner with argument NAMES (`sign(x)`), so
//! boundary activation (`boundary.rs`) sees no literal input and would keep
//! the oracle verdict. The decorator's argvalues are usually literal and
//! visible, so each generated case is recorded here as argname -> source text.
//!
//! Only shapes whose generated cases are statically certain are recorded:
//!
//! - argnames as a `"a, b"` string or a list/tuple of strings;
//! - argvalues as a list/tuple literal whose rows are tuples/lists of the
//!   argnames' arity (a bare value for a single argname), each optionally
//!   wrapped in `pytest.param(...)`;
//! - stacked decorators combine as their Cartesian product, capped at
//!   [`MAX_PARAMETRIZE_CASES`].
//!
//! Anything else (argvalues named by a variable, `indirect=`, starred rows, a
//! row of the wrong arity, an oversized product) records no cases, and the
//! caller keeps today's unresolved behavior. A case value is kept as source
//! text; the caller decides whether it is a literal.

use super::module_constants::{PythonModuleNames, body_bound_names};
use super::source_utils::text_for_range;
use super::{PythonImport, expr_full_name};
use rustpython_parser::ast::{self, Expr, Ranged};
use std::collections::{BTreeMap, BTreeSet};

/// Product cap for stacked decorators; above it no case is recorded.
const MAX_PARAMETRIZE_CASES: usize = 256;

/// The generated cases of a parametrized test: one map per case from
/// argname to the argvalue's source text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PythonParametrizeCases {
    pub(super) cases: Vec<BTreeMap<String, String>>,
}

impl PythonParametrizeCases {
    /// Every argname the parametrize stack binds. Unlike
    /// [`Self::excluding_body_bindings`], body rebinding does not remove a
    /// name: pytest still supplies that parameter, not a fixture.
    pub(super) fn argnames(&self) -> BTreeSet<String> {
        self.cases
            .iter()
            .flat_map(|case| case.keys().cloned())
            .collect()
    }

    /// The cases without the argnames the test body rebinds (an assignment,
    /// a tuple, loop, `with` or `except` target, an import, `del`), so a
    /// rebound name never pairs with its case value; None when the body's
    /// bindings cannot be enumerated or it defines a nested scope.
    pub(super) fn excluding_body_bindings(mut self, body: &[ast::Stmt]) -> Option<Self> {
        let bound = body_bound_names(body)?;
        for case in &mut self.cases {
            case.retain(|name, _| !bound.contains(name));
        }
        Some(self)
    }

    /// Whether `name` is an argname of every recorded case.
    pub(super) fn binds(&self, name: &str) -> bool {
        !self.cases.is_empty() && self.cases.iter().all(|case| case.contains_key(name))
    }
}

/// The parametrize cases of a test's decorators, or None when the test is
/// not parametrized or any parametrize decorator is not statically certain.
pub(super) fn parametrize_cases(
    source: &str,
    decorators: &[Expr],
    scope: ParametrizeScope<'_>,
) -> Option<PythonParametrizeCases> {
    let mut product: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    let mut any = false;
    for decorator in decorators {
        // The same pytest mark the routing read accepts (#10867): a mark
        // that may be another library's `parametrize` records nothing.
        let call = match scope.parametrize_call(decorator) {
            None => continue,
            Some((_, MarkKind::Unsettled)) => return None,
            Some((call, MarkKind::Pytest)) => call,
        };
        any = true;
        let rows = decorator_cases(source, call)?;
        let mut next = Vec::with_capacity(product.len().saturating_mul(rows.len()));
        for base in &product {
            for row in &rows {
                if next.len() >= MAX_PARAMETRIZE_CASES {
                    return None;
                }
                let mut case = base.clone();
                for (name, value) in row {
                    // The same argname in two stacked decorators is a pytest
                    // collection error; record nothing rather than guess.
                    if case.insert(name.clone(), value.clone()).is_some() {
                        return None;
                    }
                }
                next.push(case);
            }
        }
        product = next;
    }
    (any && !product.is_empty()).then_some(PythonParametrizeCases { cases: product })
}

/// How a parametrize argname reaches the test, read from `indirect=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Routing {
    /// pytest passes the value to the test (no `indirect=`, literal
    /// `False`, or a literal string list that omits the name).
    Direct,
    /// Literal `True` or a literal string list naming it: the value goes to
    /// the fixture of that name.
    Routed,
    /// A value the read cannot settle (`indirect=IND`, a computed list, a
    /// non-boolean constant): either may happen (#10685). Also a mark pytest
    /// refuses at collection (`indirect=None`, an `indirect` name the mark
    /// does not parametrize, an argname parametrized twice): the test never
    /// runs, so no reading of its value earns credit (#10867).
    Unsettled,
}

/// The names a test's parametrize decorators may lean on (#10685): the
/// imports in effect where the test is defined, the module's never-rebound
/// names and `NAME = <value>` bindings, and the names the enclosing class
/// body binds (empty at module scope; a class body is looked up before the
/// module).
#[derive(Clone, Copy)]
pub(super) struct ParametrizeScope<'a> {
    pub(super) imports: &'a [PythonImport],
    pub(super) module_values: &'a PythonModuleNames<'a>,
    pub(super) class_names: &'a [String],
    /// The enclosing class body binds names the walk cannot enumerate.
    pub(super) class_opaque: bool,
}

impl<'a> ParametrizeScope<'a> {
    /// The module value a bare name in a decorator evaluates to: bound once,
    /// before the decorator runs, and not shadowed by the class body.
    fn module_value(&self, name: &str, use_offset: usize) -> Option<&'a Expr> {
        if self.class_shadows(name) {
            return None;
        }
        self.module_values
            .values
            .iter()
            .find(|value| value.name == name && value.offset < use_offset)
            .map(|value| value.value)
    }

    /// The enclosing class body binds `name` (or binds names the walk
    /// cannot enumerate), so it is looked up there, not in the module.
    fn class_shadows(&self, name: &str) -> bool {
        self.class_opaque || self.class_names.iter().any(|bound| bound == name)
    }

    /// `expr` with one bare-name indirection read through the module.
    fn resolve<'e>(&self, expr: &'e Expr) -> &'e Expr
    where
        'a: 'e,
    {
        match expr {
            Expr::Name(name) => self
                .module_value(name.id.as_str(), usize::from(name.range.start()))
                .unwrap_or(expr),
            _ => expr,
        }
    }

    /// [`Self::resolve`] for argnames and `indirect`, which pytest reads
    /// when the decorator runs: a module list (or other mutable display)
    /// may have changed by then (`IND = []; IND.append("q")`), so only a
    /// constant or a tuple is read through the name.
    fn resolve_value<'e>(&self, expr: &'e Expr) -> &'e Expr
    where
        'a: 'e,
    {
        match self.resolve(expr) {
            value @ (Expr::Constant(_) | Expr::Tuple(_)) => value,
            _ => expr,
        }
    }

    /// Whether `func` names pytest's own `parametrize`. A root an import
    /// binds (`import pytest as pt`, `from pytest import mark as m`)
    /// resolves through it when the module binds the root only there and
    /// the class body does not rebind it; a module alias of the callee
    /// (`parametrize = pytest.mark.parametrize`) reads through once. A root
    /// no import binds keeps the canonical spellings
    /// [`is_parametrize_name`] accepts.
    fn parametrize_kind(&self, func: &Expr) -> Option<MarkKind> {
        let func = self.resolve(func);
        if !matches!(func, Expr::Name(_) | Expr::Attribute(_)) {
            return None;
        }
        let name = expr_full_name(func)?;
        let canonical = is_parametrize_name(&name);
        // A spelling pytest's own mark could have, whose root the read
        // cannot pin: its argnames still count toward the fixture limit
        // and make no fixture edge.
        let unsettled = canonical.then_some(MarkKind::Unsettled);
        let (root, tail) = name.split_once('.').unwrap_or((name.as_str(), ""));
        if self.class_shadows(root) {
            return unsettled;
        }
        let Some(import) = self
            .imports
            .iter()
            .rev()
            .find(|import| import.alias == root)
        else {
            return canonical.then_some(MarkKind::Pytest);
        };
        let mut resolved = if import.source_module.is_empty() {
            import.imported.clone()
        } else {
            format!("{}.{}", import.source_module, import.imported)
        };
        if !tail.is_empty() {
            resolved.push('.');
            resolved.push_str(tail);
        }
        if resolved != "pytest.mark.parametrize" {
            return unsettled;
        }
        if self.module_values.holds(root) {
            Some(MarkKind::Pytest)
        } else {
            Some(MarkKind::Unsettled)
        }
    }

    /// The pytest parametrize call a decorator applies, and how certain
    /// the read is: the decorator itself, or the call a module name holds
    /// (`cases = pytest.mark.parametrize(...)`; `@cases`).
    fn parametrize_call<'e>(&self, decorator: &'e Expr) -> Option<(&'e ast::ExprCall, MarkKind)>
    where
        'a: 'e,
    {
        match self.resolve(decorator) {
            Expr::Call(call) => Some((call, self.parametrize_kind(&call.func)?)),
            _ => None,
        }
    }
}

/// Whether a parametrize decorator is pytest's own mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkKind {
    Pytest,
    /// It may be pytest's mark or another `parametrize`: every argname is
    /// [`Routing::Unsettled`].
    Unsettled,
}

/// Whether any decorator applies pytest's parametrize through an alias or a
/// module name that the spelling check in `is_parametrized` cannot see.
pub(super) fn applies_parametrize(decorators: &[Expr], scope: ParametrizeScope<'_>) -> bool {
    decorators
        .iter()
        .any(|decorator| scope.parametrize_call(decorator).is_some())
}

/// Each argname of the test's pytest parametrize decorators with its
/// routing, read syntactically and independent of the literal-cases parser
/// (which gives up on variable argvalues, `pytest.param(marks=...)`, large
/// products and any `indirect=`).
fn argname_routings(decorators: &[Expr], scope: ParametrizeScope<'_>) -> Vec<(String, Routing)> {
    read_marks(decorators, scope).0
}

/// Whether the test's own pytest parametrize decorators fail collection
/// (#10867): pytest then runs none of its cases, so the test lends no
/// credit at all, through its assertions or any fixture.
pub(super) fn collection_error(decorators: &[Expr], scope: ParametrizeScope<'_>) -> bool {
    read_marks(decorators, scope).1
}

/// The argname routings and whether any mark fails collection. One refused
/// mark makes every argname of the test unsettled, a healthy sibling mark's
/// routed fixture included: the test never runs.
fn read_marks(decorators: &[Expr], scope: ParametrizeScope<'_>) -> (Vec<(String, Routing)>, bool) {
    let mut found = Vec::new();
    let mut refused_any = false;
    for decorator in decorators {
        let Some((call, kind)) = scope.parametrize_call(decorator) else {
            continue;
        };
        let mut argnames = call.args.first();
        // `indirect` is the third positional parameter of
        // `parametrize(argnames, argvalues, indirect, ids, scope)`.
        let mut indirect = call.args.get(2);
        // A `*args` or `**kwargs` splat may carry `indirect` unseen.
        let mut splat = call.args.iter().any(|arg| matches!(arg, Expr::Starred(_)));
        for keyword in &call.keywords {
            match keyword.arg.as_ref().map(|arg| arg.as_str()) {
                Some("argnames") => argnames = Some(&keyword.value),
                Some("indirect") => indirect = Some(&keyword.value),
                None => splat = true,
                _ => {}
            }
        }
        let Some((names, _)) = argnames.and_then(|expr| argnames_list(scope.resolve_value(expr)))
        else {
            continue;
        };
        let indirect = indirect.map(|value| scope.resolve_value(value));
        // pytest refuses `indirect=None` and a literal list naming a name
        // the mark does not parametrize.
        let refused = indirect.is_some_and(|value| {
            matches!(value, Expr::Constant(constant) if constant.value == ast::Constant::None)
                || names_unknown_indirect(value, &names)
        });
        refused_any |= refused;
        for name in &names {
            let routing = if splat || refused || kind == MarkKind::Unsettled {
                Routing::Unsettled
            } else {
                indirect.map_or(Routing::Direct, |value| indirect_routing(value, name))
            };
            found.push((name.clone(), routing));
        }
    }
    // "duplicate parametrization of 'q'" is a collection error too.
    refused_any |= has_duplicate(found.iter().map(|(name, _)| name.as_str()));
    if refused_any {
        for (_, routing) in &mut found {
            *routing = Routing::Unsettled;
        }
    }
    (found, refused_any)
}

/// Every argname the test's pytest parametrize decorators name, repeats
/// kept, so a caller combining class and method marks can see a name
/// parametrized twice (#10867).
pub(super) fn parametrize_argnames(
    decorators: &[Expr],
    scope: ParametrizeScope<'_>,
) -> Vec<String> {
    let mut names = Vec::new();
    for decorator in decorators {
        let Some((call, _)) = scope.parametrize_call(decorator) else {
            continue;
        };
        let argnames = call
            .keywords
            .iter()
            .find(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|arg| arg.as_str() == "argnames")
            })
            .map(|keyword| &keyword.value)
            .or_else(|| call.args.first());
        if let Some((found, _)) = argnames.and_then(|expr| argnames_list(scope.resolve_value(expr)))
        {
            names.extend(found);
        }
    }
    names
}

/// Whether any name repeats.
pub(super) fn has_duplicate<'n>(names: impl IntoIterator<Item = &'n str>) -> bool {
    let mut seen = BTreeSet::new();
    names.into_iter().any(|name| !seen.insert(name))
}

/// A literal `indirect` list naming something the mark does not
/// parametrize: pytest stops with "indirect fixture 'x' doesn't exist".
fn names_unknown_indirect(value: &Expr, names: &[String]) -> bool {
    let elements = match value {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return false,
    };
    string_elements(elements).is_some_and(|listed| listed.iter().any(|item| !names.contains(item)))
}

/// Argnames not definitely routed to a fixture. They are never resolved as
/// fixtures, so a fixture of the same name makes no `fixture_owner_call`
/// edge unless `indirect=` literally routes to it (RIPR-SPEC-0028).
pub(super) fn direct_argnames(decorators: &[Expr], scope: ParametrizeScope<'_>) -> Vec<String> {
    argname_routings(decorators, scope)
        .into_iter()
        .filter(|(_, routing)| *routing != Routing::Routed)
        .map(|(name, _)| name)
        .collect()
}

/// Argnames whose `indirect=` value the read cannot settle. pytest may pass
/// them through a fixture that changes the value, so they still count
/// toward `unresolved_pytest_fixture` (RIPR-SPEC-0238 L8, #10685).
pub(super) fn unsettled_indirect_argnames(
    decorators: &[Expr],
    scope: ParametrizeScope<'_>,
) -> Vec<String> {
    argname_routings(decorators, scope)
        .into_iter()
        .filter(|(_, routing)| *routing == Routing::Unsettled)
        .map(|(name, _)| name)
        .collect()
}

fn indirect_routing(value: &Expr, name: &str) -> Routing {
    let listed = |elements: &[Expr]| match string_elements(elements) {
        Some(names) if names.iter().any(|listed| listed == name) => Routing::Routed,
        Some(_) => Routing::Direct,
        None => Routing::Unsettled,
    };
    match value {
        Expr::Constant(constant) => match constant.value {
            ast::Constant::Bool(true) => Routing::Routed,
            ast::Constant::Bool(false) => Routing::Direct,
            // "expected Sequence or boolean for indirect, got NoneType".
            _ => Routing::Unsettled,
        },
        Expr::List(list) => listed(&list.elts),
        Expr::Tuple(tuple) => listed(&tuple.elts),
        _ => Routing::Unsettled,
    }
}

/// pytest's own decorator only (`@pytest.mark.parametrize`, or
/// `@mark.parametrize` after `from pytest import mark`). Another
/// `.parametrize` (a plugin or project helper) may supply other values, so
/// its cases stay unresolved.
fn is_parametrize_name(name: &str) -> bool {
    name == "pytest.mark.parametrize" || name == "mark.parametrize"
}

/// The cases of one `parametrize(argnames, argvalues, ...)` call.
fn decorator_cases(source: &str, call: &ast::ExprCall) -> Option<Vec<Vec<(String, String)>>> {
    // A third positional argument is `indirect`; a splat may hide one.
    if call.args.len() > 2 || call.args.iter().any(|arg| matches!(arg, Expr::Starred(_))) {
        return None;
    }
    let mut argnames = call.args.first();
    let mut argvalues = call.args.get(1);
    for keyword in &call.keywords {
        match keyword.arg.as_ref().map(|arg| arg.as_str()) {
            Some("argnames") => argnames = Some(&keyword.value),
            Some("argvalues") => argvalues = Some(&keyword.value),
            // `indirect=` routes values to fixtures, not to the test's
            // parameters.
            Some("indirect") | None => return None,
            Some(_) => {}
        }
    }
    let (names, bare_single) = argnames_list(argnames?)?;
    let rows = match argvalues? {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    rows.iter()
        .map(|row| {
            let values = row_values(row, names.len(), bare_single)?;
            Some(
                names
                    .iter()
                    .cloned()
                    .zip(
                        values
                            .iter()
                            .map(|value| text_for_range(source, value.range()).trim().to_string()),
                    )
                    .collect(),
            )
        })
        .collect()
}

/// The argnames, and whether a single-name row is a bare value. pytest
/// unpacks rows as tuples unless argnames is a string naming exactly one
/// parameter (`"x"` takes `[1, 2]`; `("x",)` takes `[(1,), (2,)]`).
fn argnames_list(expr: &Expr) -> Option<(Vec<String>, bool)> {
    let is_string =
        matches!(expr, Expr::Constant(constant) if matches!(constant.value, ast::Constant::Str(_)));
    let names: Vec<String> = match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Str(text) => text
                .split(',')
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect(),
            _ => return None,
        },
        Expr::List(list) => string_elements(&list.elts)?,
        Expr::Tuple(tuple) => string_elements(&tuple.elts)?,
        _ => return None,
    };
    let bare_single = is_string && names.len() == 1;
    (!names.is_empty()).then_some((names, bare_single))
}

fn string_elements(elements: &[Expr]) -> Option<Vec<String>> {
    elements
        .iter()
        .map(|element| match element {
            Expr::Constant(constant) => match &constant.value {
                ast::Constant::Str(text) => Some(text.trim().to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The values of one argvalues row, unwrapping `pytest.param(...)`. A
/// `pytest.param` with any keyword but `id=` (`marks=` skip or xfail) may
/// never run as a passing case, so it records nothing.
fn row_values(row: &Expr, arity: usize, bare_single: bool) -> Option<Vec<&Expr>> {
    if let Expr::Call(call) = row
        && expr_full_name(&call.func)
            .is_some_and(|name| name == "param" || name.ends_with(".param"))
    {
        if call.args.iter().any(|arg| matches!(arg, Expr::Starred(_)))
            || call
                .keywords
                .iter()
                .any(|keyword| keyword.arg.as_ref().is_none_or(|arg| arg.as_str() != "id"))
        {
            return None;
        }
        return (call.args.len() == arity).then(|| call.args.iter().collect());
    }
    if bare_single {
        return (!matches!(row, Expr::Starred(_))).then(|| vec![row]);
    }
    let elements = match row {
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::List(list) => &list.elts,
        _ => return None,
    };
    if elements
        .iter()
        .any(|element| matches!(element, Expr::Starred(_)))
    {
        return None;
    }
    (elements.len() == arity).then(|| elements.iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpython_parser::ast::Stmt;

    fn cases(source: &str) -> Option<Vec<Vec<(String, String)>>> {
        let module =
            super::super::source_facts::parse_module(std::path::Path::new("t.py"), source)?;
        let body = match module {
            ast::Mod::Module(module) => module.body,
            _ => return None,
        };
        let Some(Stmt::FunctionDef(function)) = body.into_iter().last() else {
            return None;
        };
        let module_values = PythonModuleNames::default();
        let scope = ParametrizeScope {
            imports: &[],
            module_values: &module_values,
            class_names: &[],
            class_opaque: false,
        };
        parametrize_cases(source, &function.decorator_list, scope).map(|found| {
            found
                .cases
                .into_iter()
                .map(|case| case.into_iter().collect())
                .collect()
        })
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn records_string_list_and_tuple_argnames_with_param_rows() {
        let expected = Some(vec![
            pairs(&[("a", "1"), ("b", "'x'")]),
            pairs(&[("a", "-2"), ("b", "None")]),
        ]);
        for decorator in [
            "@pytest.mark.parametrize(\"a,b\", [(1, 'x'), pytest.param(-2, None, id=\"n\")])",
            "@pytest.mark.parametrize([\"a\", \"b\"], ((1, 'x'), [-2, None]))",
            "@mark.parametrize(argnames=(\"a\", \"b\"), argvalues=[(1, 'x'), (-2, None)], ids=str)",
        ] {
            let source = format!("{decorator}\ndef test_x(a, b):\n    pass\n");
            assert_eq!(cases(&source), expected, "{decorator}");
        }
    }

    /// Only pytest's own decorator is expanded: a plugin or project
    /// `.parametrize` may supply other values.
    #[test]
    fn non_pytest_parametrize_decorators_record_nothing() {
        for decorator in [
            "@cases.parametrize(\"a\", [1])",
            "@parametrize(\"a\", [1])",
            "@my.mark.parametrize(\"a\", [1])",
        ] {
            let source = format!("{decorator}\ndef test_x(a):\n    pass\n");
            assert_eq!(cases(&source), None, "{decorator}");
        }
    }

    #[test]
    fn single_name_sequence_argnames_unpack_their_rows() {
        let source = "@pytest.mark.parametrize((\"a\",), [(1,), [2]])\ndef test_x(a):\n    pass\n";
        assert_eq!(
            cases(source),
            Some(vec![pairs(&[("a", "1")]), pairs(&[("a", "2")])])
        );
    }

    #[test]
    fn stacked_decorators_form_a_product() {
        let source = "@pytest.mark.parametrize(\"a\", [1, 2])\n@pytest.mark.parametrize(\"b\", [3])\ndef test_x(a, b):\n    pass\n";
        assert_eq!(
            cases(source),
            Some(vec![
                pairs(&[("a", "1"), ("b", "3")]),
                pairs(&[("a", "2"), ("b", "3")]),
            ])
        );
    }

    #[test]
    fn uncertain_shapes_record_nothing() {
        for decorator in [
            "@pytest.mark.parametrize(\"a\", CASES)",
            "@pytest.mark.parametrize(\"a\", [1], indirect=True)",
            "@pytest.mark.parametrize(\"a,b\", [(1, 2), (3,)])",
            "@pytest.mark.parametrize(\"a,b\", [(1, *rest)])",
            "@pytest.mark.parametrize(\"a\", [1], **options)",
            "@pytest.mark.parametrize(\"a\", [1])\n@pytest.mark.parametrize(\"a\", [2])",
            "@pytest.mark.slow",
            // A skipped or xfail case may never run as a passing case.
            "@pytest.mark.parametrize(\"a\", [1, pytest.param(0, marks=pytest.mark.skip)])",
            "@pytest.mark.parametrize(\"a\", [pytest.param(0, marks=[pytest.mark.xfail])])",
            // pytest unpacks rows for a list/tuple argnames, even of one name.
            "@pytest.mark.parametrize([\"a\"], [1, 2])",
        ] {
            let source = format!("{decorator}\ndef test_x(a, b=0):\n    pass\n");
            assert_eq!(cases(&source), None, "{decorator}");
        }
    }

    #[test]
    fn oversized_product_records_nothing() {
        let values = (0..20)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "@pytest.mark.parametrize(\"a\", [{values}])\n@pytest.mark.parametrize(\"b\", [{values}])\ndef test_x(a, b):\n    pass\n"
        );
        assert_eq!(cases(&source), None);
    }
}
