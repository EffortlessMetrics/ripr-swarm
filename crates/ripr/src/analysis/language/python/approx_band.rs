//! `approx(expected, rel=, abs=)` band evaluation for a changed arithmetic
//! return (RIPR-SPEC-0238 rule 21, #9546).
//!
//! An explicit band may admit the old value, so a numeric edit under such an
//! assertion used to abstain. For a narrow set of shapes the band is
//! decidable from the source alone. The owner's result is `+ - * /`
//! arithmetic over numeric literals and its own parameters, read from one of:
//!
//! * a one-statement function, `def f(..): return <expr>`;
//! * the same function under `functools.lru_cache` / `functools.cache`
//!   memoization only, which returns the decorated function's own result
//!   (#13723);
//! * a function whose body is single-line `name = <expr>` assignments then
//!   `return <expr>`, when the file's whole diff is the one changed line, so
//!   the old body is the new body with that line put back (#13723);
//! * a one-statement method of a plain, base-free class without dunders,
//!   called on a fresh `Class()` whose name the test imports from the
//!   owner's module (#13723).
//!
//! The selected assertion compares one literal-argument call to that owner
//! with `pytest.approx(<number>, <rel>, <abs>)`, the band given by keyword or
//! positionally, each value a numeric literal or a constant of the test's
//! own module that nothing can rebind (#13723). The old and new return
//! values are evaluated at the call's arguments and compared with the band
//! the way pytest does:
//!
//! * the old value inside the band and the new value outside it: the
//!   assertion fails after the edit, so it discriminates the change;
//! * both inside: the assertion passes either way, a weak gap;
//! * anything else (an old value already outside, a value too close to the
//!   band edge to trust under floating point, an unreadable shape) is
//!   unread and keeps the abstention.

use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::module_constants::ModuleBindingFacts;
use super::related_tests::{PythonRelatedCandidate, import_source_module_matches_owner};
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_end, line_for_range_start};
use super::static_limits::{
    is_imported_functools_memoization_decorator, is_simple_python_identifier,
};
use super::test_local_literal::test_module_constant;
use super::{PythonAssertion, PythonOwner, PythonTest, expr_full_name};
use crate::domain::{OracleStrength, OwnerKind, ValueContext};
use crate::value_origins::LocatedValueFact;
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::BTreeMap;
use std::path::Path;

/// The largest magnitude `f64` holds every integer up to exactly. Python
/// ints are exact, so a literal or intermediate result beyond this is not
/// evaluated.
const EXACT_INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0;

/// Relative distance to the band edge below which the outcome is not trusted:
/// evaluation runs in `f64`, pytest's runs in Python floats and ints.
const EDGE_MARGIN: f64 = 1e-9;

/// What the selected strong assertions' explicit bands say about the edit.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum PythonBandReading {
    /// No selected strong assertion passes an explicit band.
    NotBanded,
    /// A band is present but this bounded evaluation cannot read the edit.
    Unread,
    Read(PythonBandEvidence),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonBandVerdict {
    /// The new value leaves the band the old value was inside.
    Rejects,
    /// Old and new values both sit inside the band of every banded assertion.
    Absorbs,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PythonBandEvidence {
    pub(super) verdict: PythonBandVerdict,
    pub(super) discriminator: String,
    pub(super) observed_values: Vec<LocatedValueFact>,
    pub(super) reason: String,
}

/// The changed file's new-side source and whether the file's whole diff is
/// one line replaced in place. Empty source reads only the shapes that need
/// none (an undecorated one-statement function).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PythonBandSource<'a> {
    pub(super) text: &'a str,
    pub(super) sole_line_replacement: bool,
}

/// Read the explicit `approx` bands of the strong assertions the focus
/// selects against the edit from `old_line` to `new_line`.
pub(super) fn python_band_reading(
    old_line: Option<&str>,
    new_line: &str,
    owner: &PythonOwner,
    source: PythonBandSource<'_>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> PythonBandReading {
    let mut banded = Vec::new();
    let mut unbanded_strong = false;
    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let Some(assertion) =
            select_relevant_assertion(&candidate.test.assertions, Some(focus)).assertion()
        else {
            continue;
        };
        if assertion.oracle_strength.rank() < OracleStrength::Strong.rank() {
            continue;
        }
        if super::static_limits::approx_call_has_band(&assertion.gate_text) {
            banded.push((candidate.test, assertion));
        } else {
            unbanded_strong = true;
        }
    }
    if banded.is_empty() {
        return PythonBandReading::NotBanded;
    }
    let Some(old_line) = old_line else {
        return PythonBandReading::Unread;
    };
    let Some(edit) = ReturnEdit::read(old_line, new_line, owner, source) else {
        return PythonBandReading::Unread;
    };
    let mut outcomes = Vec::new();
    for (test, assertion) in banded {
        match edit.evaluate(test, assertion, owner) {
            Some(outcome) => outcomes.push(outcome),
            None => return PythonBandReading::Unread,
        }
    }
    let verdict = if outcomes.iter().any(|outcome| outcome.rejects) {
        PythonBandVerdict::Rejects
    } else if !unbanded_strong && outcomes.iter().all(|outcome| outcome.absorbs) {
        // Another strong assertion that is not banded could still catch the
        // edit, so only an all-banded set proves a gap.
        PythonBandVerdict::Absorbs
    } else {
        return PythonBandReading::Unread;
    };
    let matching: Vec<&Outcome> = outcomes
        .iter()
        .filter(|outcome| match verdict {
            PythonBandVerdict::Rejects => outcome.rejects,
            PythonBandVerdict::Absorbs => outcome.absorbs,
        })
        .collect();
    let observed_values = matching
        .iter()
        .map(|outcome| LocatedValueFact {
            file: None,
            line: outcome.line,
            text: outcome.assertion_text.clone(),
            value: outcome.call.clone(),
            context: ValueContext::FunctionArgument,
        })
        .collect();
    let detail = matching
        .iter()
        .map(|outcome| outcome.detail.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let (discriminator, reason) = match verdict {
        PythonBandVerdict::Rejects => (
            String::new(),
            format!("the edit moves the result outside the asserted `approx` band ({detail})"),
        ),
        PythonBandVerdict::Absorbs => (
            "a result outside the `approx` band".to_string(),
            format!(
                "the asserted `approx` band admits both the old and the new result ({detail}), so the assertion passes either way"
            ),
        ),
    };
    PythonBandReading::Read(PythonBandEvidence {
        verdict,
        discriminator,
        observed_values,
        reason,
    })
}

struct Outcome {
    line: usize,
    assertion_text: String,
    call: String,
    rejects: bool,
    absorbs: bool,
    detail: String,
}

/// The owner's result before and after the edit.
struct ReturnEdit {
    old: OwnerBody,
    new: OwnerBody,
    /// For a method owner, the class a test must construct (`Class()`) to
    /// call it; `None` for a function owner.
    receiver_class: Option<String>,
}

/// Straight-line `name = <expr>` assignments, in order, then the returned
/// expression.
struct OwnerBody {
    locals: Vec<(String, Expr)>,
    result: Expr,
}

impl OwnerBody {
    fn evaluate(&self, bindings: &BTreeMap<String, f64>) -> Option<f64> {
        let mut names = bindings.clone();
        for (name, value) in &self.locals {
            let value = evaluate(value, &names)?;
            names.insert(name.clone(), value);
        }
        evaluate(&self.result, &names)
    }
}

impl ReturnEdit {
    fn read(
        old_line: &str,
        new_line: &str,
        owner: &PythonOwner,
        source: PythonBandSource<'_>,
    ) -> Option<Self> {
        let method = match owner.owner_kind {
            Some(OwnerKind::Function) => false,
            Some(OwnerKind::Method) => true,
            _ => return None,
        };
        let one_statement = owner.end_line == owner.start_line + 1;
        // The undecorated one-statement function needs no source; every
        // other shape is checked against the owner's parsed definition.
        if !method && owner.decorators.is_empty() && one_statement {
            return Some(Self {
                old: return_body(old_line)?,
                new: return_body(new_line)?,
                receiver_class: None,
            });
        }
        // The decorators, the class and a longer body are read from the new
        // source, which shows the old owner only when this line is the
        // file's whole diff: a decorator, class member or other statement
        // changed beside it could have changed the old result.
        if !source.sole_line_replacement {
            return None;
        }
        let definition = OwnerDefinition::read(owner, source.text)?;
        let (old, new) = if one_statement {
            (return_body(old_line)?, return_body(new_line)?)
        } else if method {
            return None;
        } else {
            straight_line_bodies(&definition.function, source.text, old_line, new_line)?
        };
        Some(Self {
            old,
            new,
            receiver_class: definition.receiver_class,
        })
    }

    fn evaluate(
        &self,
        test: &PythonTest,
        assertion: &PythonAssertion,
        owner: &PythonOwner,
    ) -> Option<Outcome> {
        let comparison =
            parse_expression(assertion.gate_text.trim_start().strip_prefix("assert")?)?;
        let Expr::Compare(compare) = &comparison else {
            return None;
        };
        if compare.ops.len() != 1 || !matches!(compare.ops[0], ast::CmpOp::Eq) {
            return None;
        }
        let right = compare.comparators.first()?;
        let (call, band) = match (compare.left.as_ref(), right) {
            (call @ Expr::Call(_), band @ Expr::Call(_)) => {
                if approx_name(call) {
                    (band, call)
                } else {
                    (call, band)
                }
            }
            _ => return None,
        };
        let band = Band::read(band, test)?;
        let Expr::Call(call_expr) = call else {
            return None;
        };
        match &self.receiver_class {
            None => {
                if call_name(&call_expr.func)? != owner.name {
                    return None;
                }
            }
            Some(class) => {
                if !fresh_instance_call(&call_expr.func, class, &owner.name)
                    || !test_binds_owner_class(test, owner, class)
                {
                    return None;
                }
            }
        }
        let bindings = bind_arguments(owner, call_expr, self.receiver_class.is_some())?;
        let old = self.old.evaluate(&bindings)?;
        let new = self.new.evaluate(&bindings)?;
        let tolerance = band.tolerance();
        let old_distance = (old - band.expected).abs();
        let new_distance = (new - band.expected).abs();
        if near_edge(old_distance, tolerance) || near_edge(new_distance, tolerance) {
            return None;
        }
        // An old value outside the band means the assertion already failed
        // before the edit; whether it still does is not a statement about
        // this edit.
        if old_distance > tolerance {
            return None;
        }
        let call_text = format!(
            "{}({})",
            owner.name,
            owner
                .parameters
                .iter()
                .filter_map(|parameter| bindings
                    .get(&parameter.name)
                    .map(|value| format!("{}={value}", parameter.name)))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Some(Outcome {
            line: assertion.line,
            assertion_text: assertion.text.clone(),
            call: call_text,
            rejects: new_distance > tolerance,
            absorbs: new_distance <= tolerance,
            detail: format!(
                "expected {} with tolerance {tolerance}; old result {old}, new result {new}",
                band.expected
            ),
        })
    }
}

struct Band {
    expected: f64,
    rel: Option<f64>,
    abs: Option<f64>,
}

impl Band {
    /// `approx(<expected>, <rel>, <abs>, <nan_ok>)` with the band given by
    /// keyword or positionally (pytest's own parameter order, #13723). Each
    /// value is a numeric literal or a constant of the test's module that
    /// nothing can rebind (`test_module_constant`); a literal `None` band is
    /// absent. A spread, a repeated parameter or any other argument is not
    /// read.
    fn read(call: &Expr, test: &PythonTest) -> Option<Self> {
        let Expr::Call(call) = call else {
            return None;
        };
        if !approx_name(&Expr::Call(call.clone())) || call.args.is_empty() || call.args.len() > 4 {
            return None;
        }
        let expected = band_value(call.args.first()?, test)?;
        let (mut rel, mut abs) = (None, None);
        let mut given = [false; 3];
        for (index, argument) in call.args.iter().enumerate().skip(1) {
            let slot = index - 1;
            given[slot] = true;
            match slot {
                0 => rel = optional_band_value(argument, test)?,
                1 => abs = optional_band_value(argument, test)?,
                _ => nan_ok_flag(argument)?,
            }
        }
        for keyword in &call.keywords {
            let name = keyword.arg.as_ref()?.as_str();
            let slot = match name {
                "rel" => 0,
                "abs" => 1,
                "nan_ok" => 2,
                _ => return None,
            };
            // A parameter passed twice is a `TypeError`, not a band.
            if std::mem::replace(&mut given[slot], true) {
                return None;
            }
            match slot {
                0 => rel = optional_band_value(&keyword.value, test)?,
                1 => abs = optional_band_value(&keyword.value, test)?,
                _ => {}
            }
        }
        if rel.is_none() && abs.is_none() {
            return None;
        }
        if rel.is_some_and(|value| value < 0.0) || abs.is_some_and(|value| value < 0.0) {
            return None;
        }
        Some(Self { expected, rel, abs })
    }

    /// pytest's `ApproxScalar.tolerance`: `abs` alone is the tolerance, else
    /// the larger of `rel * |expected|` and `abs` (default `1e-12`).
    fn tolerance(&self) -> f64 {
        match (self.rel, self.abs) {
            (None, Some(abs)) => abs,
            (rel, abs) => {
                let relative = rel.unwrap_or(1e-6) * self.expected.abs();
                relative.max(abs.unwrap_or(1e-12))
            }
        }
    }
}

fn near_edge(distance: f64, tolerance: f64) -> bool {
    (distance - tolerance).abs() <= EDGE_MARGIN * tolerance.abs().max(1.0)
}

fn approx_name(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    expr_full_name(&call.func).is_some_and(|name| name == "approx" || name == "pytest.approx")
}

fn call_name(func: &Expr) -> Option<&str> {
    match func {
        Expr::Name(name) => Some(name.id.as_str()),
        _ => None,
    }
}

/// A band argument: a numeric literal, or a name the test's module binds
/// once to one that nothing can rebind before the assertion runs.
fn band_value(expr: &Expr, test: &PythonTest) -> Option<f64> {
    match expr {
        Expr::Name(name) => {
            let literal = test_module_constant(test, name.id.as_str())?;
            evaluate(&parse_expression(&literal)?, &BTreeMap::new())
        }
        other => evaluate(other, &BTreeMap::new()),
    }
}

/// A `rel` or `abs` band: a literal `None` is absent (`Some(None)`).
fn optional_band_value(expr: &Expr, test: &PythonTest) -> Option<Option<f64>> {
    match expr {
        Expr::Constant(constant) if matches!(constant.value, ast::Constant::None) => Some(None),
        other => band_value(other, test).map(Some),
    }
}

/// A positional `nan_ok` is read only as a `True` or `False` literal.
fn nan_ok_flag(expr: &Expr) -> Option<()> {
    match expr {
        Expr::Constant(constant) if matches!(constant.value, ast::Constant::Bool(_)) => Some(()),
        _ => None,
    }
}

/// `return <expr>` read as a body with no locals.
fn return_body(line: &str) -> Option<OwnerBody> {
    Some(OwnerBody {
        locals: Vec::new(),
        result: parse_expression(line.trim().strip_prefix("return ")?)?,
    })
}

/// The owner's `def`, found in the parsed new-side source, with every
/// condition its shape needs checked.
struct OwnerDefinition {
    function: ast::StmtFunctionDef,
    receiver_class: Option<String>,
}

impl OwnerDefinition {
    /// A function owner is a module-level `def` (not `async def`) whose
    /// decorators are only `functools` memoization; a method owner is an
    /// undecorated `def` in a [`plain_owner_class`]. The name must be
    /// defined once in its scope, so the definition read is the one called.
    fn read(owner: &PythonOwner, text: &str) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let Ok(Mod::Module(module)) = parse_module_result(Path::new("<owner>"), text) else {
            return None;
        };
        let source = SourceText::new(text);
        let facts = ModuleBindingFacts::of(text, &module.body);
        let (scope, receiver_class): (&[Stmt], Option<String>) =
            if owner.owner_kind == Some(OwnerKind::Method) {
                let class = plain_owner_class(owner, &module.body, &facts)?;
                (class.body.as_slice(), Some(class.name.to_string()))
            } else {
                if !owner.class_path.is_empty() || !facts.bound_once(&owner.name) {
                    return None;
                }
                (module.body.as_slice(), None)
            };
        let mut definitions = scope.iter().filter_map(|stmt| match stmt {
            Stmt::FunctionDef(function) if function.name.as_str() == owner.name => Some(function),
            _ => None,
        });
        let function = definitions.next()?;
        if definitions.next().is_some()
            || scope.iter().any(|stmt| {
                matches!(stmt, Stmt::AsyncFunctionDef(function) if function.name.as_str() == owner.name)
            })
        {
            return None;
        }
        let start = line_for_range_start(&source, function.range);
        let end = line_for_range_end(&source, function.range);
        if !(start..=end).contains(&owner.start_line) || end != owner.end_line {
            return None;
        }
        let decorators_read = if receiver_class.is_some() {
            function.decorator_list.is_empty()
        } else {
            function.decorator_list.len() == owner.decorators.len()
                && function
                    .decorator_list
                    .iter()
                    .all(|decorator| memoizing_decorator(decorator, owner, &facts))
        };
        decorators_read.then(|| Self {
            function: function.clone(),
            receiver_class,
        })
    }
}

/// `functools.lru_cache` / `functools.cache` return the decorated function's
/// own result for equal arguments, and this evaluation reads only arithmetic
/// over the arguments, so the cache cannot change the value (#13723). The
/// name must resolve to `functools` (the shared transparency check) and be
/// bound once in the owner's module; `lru_cache` may be called with only a
/// literal `maxsize` and `typed` (a callable first argument makes it wrap
/// that callable instead). `cached_property` turns the function into a
/// descriptor and is not read.
fn memoizing_decorator(decorator: &Expr, owner: &PythonOwner, facts: &ModuleBindingFacts) -> bool {
    let (target, arguments_read) = match decorator {
        Expr::Call(call) => (
            call.func.as_ref(),
            call.args.is_empty() && call.keywords.iter().all(|keyword| {
                match keyword.arg.as_ref().map(|name| name.as_str()) {
                    Some("maxsize") => matches!(
                        &keyword.value,
                        Expr::Constant(constant)
                            if matches!(constant.value, ast::Constant::Int(_) | ast::Constant::None)
                    ),
                    Some("typed") => matches!(
                        &keyword.value,
                        Expr::Constant(constant) if matches!(constant.value, ast::Constant::Bool(_))
                    ),
                    _ => false,
                }
            }),
        ),
        other => (other, true),
    };
    let Some(name) = expr_full_name(target).filter(|_| !matches!(target, Expr::Call(_))) else {
        return false;
    };
    let last = name.rsplit('.').next().unwrap_or_default();
    let first = name.split('.').next().unwrap_or_default();
    let called = matches!(decorator, Expr::Call(_));
    arguments_read
        && (last == "lru_cache" || (last == "cache" && !called))
        && owner.decorators.contains(&name)
        && is_imported_functools_memoization_decorator(&name, &owner.imports)
        && facts.bound_once(first)
}

/// The owner's class when a fresh instance of it calls the method's own
/// `def`: a module-level class bound once, with no decorator, base or class
/// keyword (any of which can replace the method or the instance), whose body
/// holds only undecorated non-dunder `def`s, a docstring or `pass` (no
/// `__init__`, `__new__` or `__getattr__` can rebind the method per
/// instance), and whose method name no attribute write in the module names.
fn plain_owner_class<'a>(
    owner: &PythonOwner,
    module: &'a [Stmt],
    facts: &ModuleBindingFacts,
) -> Option<&'a ast::StmtClassDef> {
    let class = owner.class_path.as_str();
    if !is_simple_python_identifier(class)
        || !facts.bound_once(class)
        || facts.may_assign_attribute(&owner.name)
    {
        return None;
    }
    let definition = module.iter().find_map(|stmt| match stmt {
        Stmt::ClassDef(definition) if definition.name.as_str() == class => Some(definition),
        _ => None,
    })?;
    let body_plain = definition.body.iter().all(|stmt| match stmt {
        Stmt::FunctionDef(function) => {
            function.decorator_list.is_empty()
                && !super::related_tests::is_dunder_name(function.name.as_str())
        }
        Stmt::Pass(_) => true,
        Stmt::Expr(expr) => matches!(
            expr.value.as_ref(),
            Expr::Constant(constant) if matches!(constant.value, ast::Constant::Str(_))
        ),
        _ => false,
    });
    (definition.decorator_list.is_empty()
        && definition.bases.is_empty()
        && definition.keywords.is_empty()
        && body_plain)
        .then_some(definition)
}

/// Single-line `name = <expr>` assignments then `return <expr>`, as the
/// new body and as the old one with `old_line` put back in place of the one
/// statement `new_line` spells.
fn straight_line_bodies(
    function: &ast::StmtFunctionDef,
    text: &str,
    old_line: &str,
    new_line: &str,
) -> Option<(OwnerBody, OwnerBody)> {
    let source = SourceText::new(text);
    let mut changed = None;
    for (index, stmt) in function.body.iter().enumerate() {
        let line = line_for_range_start(&source, stmt.range());
        if line != line_for_range_end(&source, stmt.range()) {
            return None;
        }
        let written = text.lines().nth(line.checked_sub(1)?)?;
        if written.trim() == new_line.trim() && changed.replace(index).is_some() {
            return None;
        }
    }
    let changed = changed?;
    let old_statement = parse_statement(old_line.trim())?;
    let mut old_statements: Vec<&Stmt> = function.body.iter().collect();
    *old_statements.get_mut(changed)? = &old_statement;
    let new_statements: Vec<&Stmt> = function.body.iter().collect();
    Some((
        straight_line_body(&old_statements)?,
        straight_line_body(&new_statements)?,
    ))
}

fn straight_line_body(statements: &[&Stmt]) -> Option<OwnerBody> {
    let (last, assignments) = statements.split_last()?;
    let Stmt::Return(ret) = last else {
        return None;
    };
    let result = ret.value.as_deref()?.clone();
    let locals = assignments
        .iter()
        .map(|stmt| match stmt {
            Stmt::Assign(assign) => match assign.targets.as_slice() {
                [Expr::Name(target)] => {
                    Some((target.id.to_string(), assign.value.as_ref().clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(OwnerBody { locals, result })
}

/// The one statement `text` spells.
fn parse_statement(text: &str) -> Option<Stmt> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("t.py"), &format!("{text}\n"))
    else {
        return None;
    };
    let mut body = module.body.into_iter();
    let statement = body.next()?;
    body.next().is_none().then_some(statement)
}

/// `Class().method` with no constructor arguments.
fn fresh_instance_call(func: &Expr, class: &str, method: &str) -> bool {
    let Expr::Attribute(attribute) = func else {
        return false;
    };
    let Expr::Call(construction) = attribute.value.as_ref() else {
        return false;
    };
    attribute.attr.as_str() == method
        && construction.args.is_empty()
        && construction.keywords.is_empty()
        && call_name(&construction.func) == Some(class)
}

/// The test's `class` is the owner's: imported under its own name from the
/// owner's module, by that import alone, and never rebound, attribute-written
/// or written through a namespace by the test or its module.
fn test_binds_owner_class(test: &PythonTest, owner: &PythonOwner, class: &str) -> bool {
    let mut bindings = test.imports.iter().filter(|import| import.alias == class);
    let from_owner_module = bindings.next().is_some_and(|import| {
        import.imported == class && import_source_module_matches_owner(import, owner, &test.file)
    });
    let rebinding = &test.constant_rebinding;
    from_owner_module
        && bindings.next().is_none()
        && !rebinding.rebinds(class)
        && !rebinding.module_opaque
        && !rebinding.assigns_attribute(class)
        && !rebinding.assigns_attribute(&owner.name)
}

/// The literal value each owner parameter takes at the call: a positional or
/// keyword numeric literal, else a numeric literal default. A method's
/// receiver (its first parameter) is bound by the instance, never by the
/// call's arguments, and stays unbound so a result that reads it is unread.
fn bind_arguments(
    owner: &PythonOwner,
    call: &ast::ExprCall,
    skip_receiver: bool,
) -> Option<BTreeMap<String, f64>> {
    let receiver = usize::from(skip_receiver);
    if skip_receiver
        && owner
            .parameters
            .first()
            .is_none_or(|parameter| parameter.keyword_only)
    {
        return None;
    }
    let parameters = owner.parameters.get(receiver..)?;
    let positional: Vec<&super::PythonParameter> = parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .collect();
    if call.args.len() > positional.len()
        || call
            .args
            .iter()
            .any(|argument| matches!(argument, Expr::Starred(_)))
    {
        return None;
    }
    let mut bindings = BTreeMap::new();
    for (parameter, argument) in positional.iter().zip(&call.args) {
        bindings.insert(
            parameter.name.clone(),
            evaluate(argument, &BTreeMap::new())?,
        );
    }
    for keyword in &call.keywords {
        let name = keyword.arg.as_ref()?.as_str();
        let parameter = parameters
            .iter()
            .find(|parameter| parameter.name == name && !parameter.positional_only)?;
        if bindings
            .insert(
                parameter.name.clone(),
                evaluate(&keyword.value, &BTreeMap::new())?,
            )
            .is_some()
        {
            return None;
        }
    }
    for parameter in parameters {
        if !bindings.contains_key(&parameter.name) {
            let default = parameter.default.as_deref()?;
            bindings.insert(
                parameter.name.clone(),
                evaluate(&parse_expression(default)?, &BTreeMap::new())?,
            );
        }
    }
    Some(bindings)
}

/// Evaluate `+ - * /` arithmetic over numeric literals and bound names.
fn evaluate(expr: &Expr, names: &BTreeMap<String, f64>) -> Option<f64> {
    let value = match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(number) => {
                let value = number.to_string().parse::<f64>().ok()?;
                if value.abs() > EXACT_INTEGER_LIMIT {
                    return None;
                }
                value
            }
            ast::Constant::Float(number) => *number,
            _ => return None,
        },
        Expr::Name(name) => *names.get(name.id.as_str())?,
        Expr::UnaryOp(unary) => {
            let operand = evaluate(&unary.operand, names)?;
            match unary.op {
                ast::UnaryOp::USub => -operand,
                ast::UnaryOp::UAdd => operand,
                _ => return None,
            }
        }
        Expr::BinOp(binary) => {
            let left = evaluate(&binary.left, names)?;
            let right = evaluate(&binary.right, names)?;
            match binary.op {
                ast::Operator::Add => left + right,
                ast::Operator::Sub => left - right,
                ast::Operator::Mult => left * right,
                ast::Operator::Div if right != 0.0 => left / right,
                _ => return None,
            }
        }
        _ => return None,
    };
    (value.is_finite() && value.abs() <= EXACT_INTEGER_LIMIT).then_some(value)
}

/// The expression of `text`, read the way the surrounding source would, so
/// spacing and parentheses do not matter.
pub(super) fn parse_expression(text: &str) -> Option<Expr> {
    let source = format!("_ = (\n{text}\n)\n");
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("t.py"), &source) else {
        return None;
    };
    match module.body.into_iter().next()? {
        Stmt::Assign(assign) => Some(*assign.value),
        _ => None,
    }
}
