//! Static evaluation of a related test's exact-value assertion against the
//! seam owner's body (#6026).
//!
//! A related test that asserts `assert_eq!(owner(literal, ..), literal)` is
//! credited as a strong `exact_value` oracle even when the asserted literal
//! is statically wrong: both the arguments and the expected value are
//! literals, and the owner is frequently one pure integer expression that a
//! constant fold can evaluate. The wrong-value assert is a discriminator
//! pointing the wrong way — it fails at baseline and would pass under the
//! mutation the seam carries — yet static evidence closed the gap on it and
//! reported nothing weak or unknown. Per the repository invariant, a wrong
//! actionable repair signal is worse than a missed advisory finding, so the
//! fold's contradiction must be typed instead of credited.
//!
//! The existing integer entrypoints are fail-closed toward the current credit:
//! when the owner is not a free function of pure literal arithmetic, when any
//! argument or expected expression does not fold, or when anything about the
//! shape is unexpected, the verdict is [`ExactValueVerdict::NotEvaluable`]
//! and every consumer keeps today's behavior. Only a fold that completes and
//! contradicts the asserted relation downgrades the oracle, states the
//! contradiction in the receipt, and stops closing the gap. The downgrade
//! direction is always weaker for those entrypoints, so a fold bug can only
//! withhold their evidence.
//!
//! #8610 also reads a boolean result for a narrowly admitted direct assertion:
//! a whole comparison of primitive integer parameters/literals, called with
//! literal integer arguments. Only a completed, consistent boolean fold may
//! pass that new pin's consistency gate; unknown is never promotion evidence.

use crate::analysis::classify::oracle_owner_call_is_shadowed;
use crate::analysis::extract::{equality_assertion_arguments, outer_assertion_condition};
use crate::analysis::facts::TestFact as TestSummary;
use crate::analysis::facts::{FunctionSummary, SourceText};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, ast, ast::HasAttrs, ast::HasName};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The fixed disclosure a contradicted oracle carries in every evidence
/// summary: stage summaries, related-test records, and the outcome
/// receipt's contradiction detection all key on this exact prefix, so the
/// analysis producer and the receipt consumer share one contract string
/// instead of a token coincidence (#6026, #7007).
pub(crate) const CONTRADICTION_DISCLOSURE: &str =
    "assertion expected value contradicts static evaluation";

/// What static evaluation can say about one related test's equality
/// assertion against the seam owner's body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExactValueVerdict {
    /// The assertion's operand relation is statically false: the owner folds
    /// to `evaluated` at the asserted literal arguments while the assertion
    /// pins `expected` (equality) or excludes `expected` (inequality). The
    /// test fails at baseline and is not evidence for the seam's behavior.
    Contradicted { expected: String, evaluated: i128 },
    /// The fold completed and agrees with the asserted relation.
    Consistent,
    /// Static evaluation is not possible for this pair. No claim either way;
    /// the current behavior (and its limitation) is retained.
    NotEvaluable,
}

/// Statically evaluate one oracle's equality assertion against `owner`'s
/// body. `owner` is the seam's owning function; `oracle_text` is the
/// assertion fact's text (for example
/// `assert_eq!(discounted_total(5_000, 5_000), 5_000)`).
pub(super) fn exact_value_assertion_verdict(
    owner: Option<&FunctionSummary>,
    oracle_text: &str,
    shadowed: bool,
) -> ExactValueVerdict {
    if shadowed {
        // A `let` binding of the owner's name in the test body shadows the
        // crate function; the bare call then folds against a body the
        // assertion never called.
        return ExactValueVerdict::NotEvaluable;
    }
    verdict_in(owner, oracle_text, &BTreeMap::new())
}

/// [`exact_value_assertion_verdict`] for an assertion of `test`. An rstest
/// case table runs its body once per `#[case(..)]` row, so each row's
/// literals are bound to the `#[case]` parameters and folded on their own:
/// one contradicted row fails at baseline and contradicts the whole test,
/// as a wrong literal does in a plain `#[test]`. Every row consistent is
/// consistent; anything else is not evaluable.
pub(super) fn exact_value_assertion_verdict_for_test(
    owner: Option<&FunctionSummary>,
    oracle_text: &str,
    test: &TestSummary,
    index: &RustIndex,
) -> ExactValueVerdict {
    let shadowed =
        owner.is_some_and(|owner| bare_owner_call_is_shadowed(test, owner, oracle_text, index));
    let Some(rows) = case_row_bindings(test) else {
        return exact_value_assertion_verdict(owner, oracle_text, shadowed);
    };
    if shadowed {
        return ExactValueVerdict::NotEvaluable;
    }
    let mut verdict = ExactValueVerdict::Consistent;
    for row in &rows {
        match verdict_in(owner, oracle_text, row) {
            contradicted @ ExactValueVerdict::Contradicted { .. } => return contradicted,
            ExactValueVerdict::NotEvaluable => verdict = ExactValueVerdict::NotEvaluable,
            ExactValueVerdict::Consistent => {}
        }
    }
    verdict
}

/// Consistency for #8610's already owner-admitted `assert!(owner(literal))`
/// or `assert!(!owner(literal))`. This never invents equality assertion text.
/// Qualified calls, let-bound results, parameterized tests and unread owner
/// shapes stay not evaluable; the caller must not raise their strength.
pub(super) fn boolean_assertion_verdict_for_test(
    owner: &FunctionSummary,
    oracle_text: &str,
    test: &TestSummary,
    index: &RustIndex,
) -> ExactValueVerdict {
    if bare_owner_call_is_shadowed(test, owner, oracle_text, index) {
        return ExactValueVerdict::NotEvaluable;
    }
    let Some(condition) = outer_assertion_condition(oracle_text) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let (operand, expected) = match condition.trim().strip_prefix('!') {
        Some(negated) => (negated.trim_start(), false),
        None => (condition.trim(), true),
    };
    let Some(arguments) = owner_call_arguments(operand, &owner.name) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some(actual) = fold_boolean_owner_call(owner, &arguments) else {
        return ExactValueVerdict::NotEvaluable;
    };
    if actual == expected {
        ExactValueVerdict::Consistent
    } else {
        ExactValueVerdict::Contradicted {
            expected: expected.to_string(),
            evaluated: i128::from(actual),
        }
    }
}

fn whole_integer_literal(text: &str) -> Option<i128> {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let tokens = tokenize(&masked)?;
    match tokens.as_slice() {
        [Token::Int(value)] => Some(*value),
        [Token::Symbol("-"), Token::Int(value)] => value.checked_neg(),
        _ => None,
    }
}

fn integer_literal_in_type(text: &str, ty: &str) -> Option<i128> {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let literal = masked.trim();
    if INTEGER_SUFFIXES
        .iter()
        .find(|suffix| literal.ends_with(**suffix))
        .is_some_and(|suffix| *suffix != ty)
    {
        return None;
    }
    let value = whole_integer_literal(text)?;
    let (low, high) = declared_integer_range(ty)?;
    (low..=high).contains(&value).then_some(value)
}

/// Reuse the fold only where integer comparison has primitive Rust semantics.
/// No arithmetic, overloadable operand, pointer-sized type or hidden statement
/// can obtain the new boolean credit from the fold's integer approximation.
fn fold_boolean_owner_call(owner: &FunctionSummary, arguments: &[String]) -> Option<bool> {
    let parse = parse_clean_source_file(owner.body.as_str())?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if function
        .ret_type()?
        .syntax()
        .text()
        .to_string()
        .replace(' ', "")
        != "->bool"
    {
        return None;
    }
    let parameters = function.param_list()?;
    if parameters.self_param().is_some() || parameters.params().count() != arguments.len() {
        return None;
    }
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut values = Vec::new();
    for (parameter, argument) in parameters.params().zip(arguments) {
        let ty = parameter.ty()?.syntax().text().to_string();
        values.push(integer_literal_in_type(argument, &ty)?);
        let ast::Pat::IdentPat(name) = parameter.pat()? else {
            return None;
        };
        if name.pat().is_some() || name.ref_token().is_some() {
            return None;
        }
        names.push(name.name()?.text().to_string());
        types.push(ty);
    }
    let body = function.body()?;
    if body.stmt_list()?.statements().next().is_some() {
        return None;
    }
    let ast::Expr::BinExpr(comparison) = body.tail_expr()? else {
        return None;
    };
    if !matches!(comparison.op_kind(), Some(ast::BinaryOp::CmpOp(_))) {
        return None;
    }
    let left = comparison.lhs()?.syntax().text().to_string();
    let right = comparison.rhs()?.syntax().text().to_string();
    let parameter_type = |operand: &str| {
        let masked = crate::analysis::extract::mask_comments_and_strings(operand);
        let tokens = tokenize(&masked)?;
        match tokens.as_slice() {
            [Token::Ident(name)] => names
                .iter()
                .position(|parameter| parameter == name)
                .map(|position| types[position].as_str()),
            _ => None,
        }
    };
    match (parameter_type(&left), parameter_type(&right)) {
        (Some(left_type), Some(right_type)) if left_type == right_type => {}
        (Some(ty), None) => {
            integer_literal_in_type(&right, ty)?;
        }
        (None, Some(ty)) => {
            integer_literal_in_type(&left, ty)?;
        }
        _ => return None,
    }
    bool_value(fold_function_value(owner.body.as_str(), &values)?)
}

/// The `#[case]` parameter bindings of each `#[case(..)]` row of an rstest
/// test, each row value folded to an integer (a value that does not fold
/// binds nothing, so an operand naming it stays not evaluable). `None` when
/// the test has no case rows.
fn case_row_bindings(test: &TestSummary) -> Option<Vec<BTreeMap<String, i128>>> {
    if !test
        .attrs
        .iter()
        .any(|attr| attr.trim_start().starts_with("#[case"))
    {
        return None;
    }
    let source = format!("{}\n{}", test.attrs.join("\n"), test.body.as_str());
    let parse = parse_clean_source_file(&source)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let parameters: Vec<Option<String>> = function
        .param_list()?
        .params()
        .filter(|param| {
            param
                .attrs()
                .any(|attr| attr.simple_name().as_deref() == Some("case"))
        })
        .map(|param| match param.pat() {
            Some(ast::Pat::IdentPat(pat)) => pat.name().map(|name| name.text().to_string()),
            _ => None,
        })
        .collect();
    let empty = BTreeMap::new();
    let mut rows = Vec::new();
    for attr in function.attrs() {
        let Some(path) = attr.path() else {
            continue;
        };
        let path = path.syntax().text().to_string().replace(' ', "");
        if path != "case" && !path.starts_with("case::") {
            continue;
        }
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            continue;
        };
        let tree = meta.token_tree()?.syntax().text().to_string();
        let inside = tree.strip_prefix('(')?.strip_suffix(')')?;
        let values = super::split_top_level_commas(inside);
        if values.len() != parameters.len() {
            return None;
        }
        let mut row = BTreeMap::new();
        for (parameter, value) in parameters.iter().zip(&values) {
            if let (Some(parameter), Some(value)) = (parameter, fold_expression(value, &empty)) {
                row.insert(parameter.clone(), value);
            }
        }
        rows.push(row);
    }
    (!rows.is_empty()).then_some(rows)
}

/// The fold of one equality assertion with `environment` binding names the
/// operands may use (an rstest row's parameters).
fn verdict_in(
    owner: Option<&FunctionSummary>,
    oracle_text: &str,
    environment: &BTreeMap<String, i128>,
) -> ExactValueVerdict {
    let Some(owner) = owner else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some((asserts_inequality, arguments)) = equality_assertion_shape(oracle_text) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let [actual, expected, ..] = arguments.as_slice() else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some(call_arguments) = owner_call_arguments(actual, &owner.name) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let mut values = Vec::with_capacity(call_arguments.len());
    for argument in &call_arguments {
        let Some(value) = fold_expression(argument, environment) else {
            return ExactValueVerdict::NotEvaluable;
        };
        values.push(value);
    }
    let Some(expected_value) = fold_expression(expected, environment) else {
        return ExactValueVerdict::NotEvaluable;
    };
    let Some(actual_value) = fold_owner_call(owner, &values) else {
        return ExactValueVerdict::NotEvaluable;
    };
    // `assert_eq!` fails at baseline exactly when the folded values differ;
    // `assert_ne!` fails exactly when they are equal.
    let contradicted = if asserts_inequality {
        actual_value == expected_value
    } else {
        actual_value != expected_value
    };
    if contradicted {
        ExactValueVerdict::Contradicted {
            expected: if environment.is_empty() {
                expected.trim().to_string()
            } else {
                expected_value.to_string()
            },
            evaluated: actual_value,
        }
    } else {
        ExactValueVerdict::Consistent
    }
}

/// The before/after comparison for a selected typed-pointer call. Preparing
/// the separating-input witness is lazy and shared by this owner pin, so
/// ordinary direct calls pay no grid scan and later assertions reuse it.
/// Call identity and assertion admission remain the caller's responsibility.
pub(in crate::analysis) struct PointerReturnChange {
    current: SourceText,
    before: String,
    after: String,
    baseline: OnceLock<Option<String>>,
}

impl PointerReturnChange {
    pub(in crate::analysis) fn new(
        owner: &FunctionSummary,
        before: Option<&str>,
        after: &str,
    ) -> Option<Self> {
        let before = before?;
        if before == after || owner.body.matches(after).count() != 1 {
            return None;
        }
        Some(Self {
            current: owner.body.clone(),
            before: before.to_string(),
            after: after.to_string(),
            baseline: OnceLock::new(),
        })
    }

    /// Refuse only a completed comparison that proves this call misses the
    /// change. Unknown folds and equivalent rewrites keep today's reading.
    pub(in crate::analysis) fn is_invisible_at_call(&self, call: &str) -> bool {
        let Some(baseline) = self.baseline.get_or_init(|| {
            let baseline = self.current.replacen(&self.after, &self.before, 1);
            versions_differ_on_small_inputs(&self.current, &baseline).then_some(baseline)
        }) else {
            return false;
        };
        let Some(open) = call.find('(') else {
            return false;
        };
        let Some(inside) = super::delimited_contents_at(call, open) else {
            return false;
        };
        let environment = BTreeMap::new();
        let mut values = Vec::new();
        for argument in super::split_top_level_commas(&inside) {
            let Some(value) = fold_expression(&argument, &environment) else {
                return false;
            };
            values.push(value);
        }
        match (
            fold_function_text(&self.current, &values),
            fold_function_text(baseline, &values),
        ) {
            (Some(changed), Some(unchanged)) => changed == unchanged,
            _ => false,
        }
    }
}

/// Whether the changed expression is invisible to every call the related
/// tests make (#8659): each test is a plain `#[test]` whose assertions are all
/// equality assertions over one bare call of `owner` with literal arguments,
/// and the owner folds to the same value at each call whether it holds
/// `after` (the current text) or `before`, while a small input grid does find
/// a call where the two differ (an equivalent rewrite is not reported). Such a test passes on the baseline
/// and on the change, so it cannot reveal the difference even though its
/// exact assertion shares the owner. Every other shape (a call outside an
/// assertion, a case table, a value that does not fold, a body where `after`
/// is not found exactly once) is `None` and keeps today's credit.
/// Why every related test failed to tell the changed expression from its
/// baseline. The honest gap statement differs between the two, so the caller
/// turns the reason into the matching infection summary (#8659, #10916).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum InvisibilityReason {
    /// Every equality assertion folds the owner to the same value at its
    /// asserted input on both versions: no asserted input reaches a
    /// difference.
    SameValueAtEveryAssertedInput,
    /// A relational assertion's comparison folds to the same truth value on
    /// both versions: the asserted inputs may evaluate differently, yet no
    /// assertion's outcome distinguishes them.
    RelationalTruthUnchanged,
}

pub(in crate::analysis) fn change_is_invisible_to_every_assertion(
    owner: &FunctionSummary,
    before: &str,
    after: &str,
    tests: &[&TestSummary],
) -> Option<InvisibilityReason> {
    let current = owner.body.as_str();
    if tests.is_empty() || before == after || current.matches(after).count() != 1 {
        return None;
    }
    let baseline = current.replacen(after, before, 1);
    // An equivalent rewrite (`x * 3` to `3 * x`) is invisible to every test
    // too, but "no input reaches a difference" would send the reader after a
    // test that cannot exist. The change must differ somewhere: a small input
    // grid has to find a call at which the two bodies fold to different values.
    if !versions_differ_on_small_inputs(current, &baseline) {
        return None;
    }
    // A related test that never names the owner exercises it through no call
    // the fold could miss (a helper that calls it makes the owner have other
    // callers, which the caller checks), so it neither blocks nor supports
    // the reading; at least one test must name the owner.
    let naming_tests = tests
        .iter()
        .filter(|test| count_name_tokens(test.body.as_str(), &owner.name) > 0);
    naming_tests.clone().next()?;
    // A mixture of equality and relational assertions reads as relational:
    // "the same truth value on both versions" holds for every member, while
    // "no input reaches a difference" does not survive a relational
    // assertion that calls the owner at two different inputs.
    let mut relational_seen = false;
    for test in naming_tests {
        if case_row_bindings(test).is_some()
            || test
                .let_bindings
                .iter()
                .any(|binding| binding.name == owner.name)
            || body_declares_same_name_fn(test, &owner.name)
            || test.assertions.is_empty()
        {
            return None;
        }
        // Every mention of the owner must be the one call an assertion folds:
        // a second call in the expected operand, or one outside an
        // assertion, may separate the versions at an input the fold never saw.
        // The call may sit in either operand, or in the `let` that binds the
        // operand, so the operands are resolved before the mentions are counted.
        // A relational assertion (`assert!(f(3) > f(1))`) passes on the
        // baseline exactly when its comparison folds the same way there, so
        // it is invisible when the comparison's truth value is unchanged.
        let mut relational_mentions = 0usize;
        let mut equality_assertions = Vec::new();
        for oracle in &test.assertions {
            match relational_assertion(&oracle.text, &owner.name) {
                Some(relational) => {
                    if !relational.truth_unchanged(current, &baseline) {
                        return None;
                    }
                    relational_seen = true;
                    relational_mentions += relational.owner_calls;
                }
                None => equality_assertions.push(oracle),
            }
        }
        let resolved: Vec<Vec<ResolvedOperand>> = equality_assertions
            .iter()
            .map(|oracle| assertion_operands(&oracle.text, test.body.as_str()))
            .collect();
        // A `let` read by several assertions is one call in the body, so each
        // binding's initializer counts once however many assertions use it.
        let mut counted_bindings = std::collections::BTreeSet::new();
        let resolved_mentions: usize = resolved
            .iter()
            .flatten()
            .filter(|operand| match &operand.binding {
                Some(name) => counted_bindings.insert(name.clone()),
                None => true,
            })
            .map(|operand| count_name_tokens(&operand.text, &owner.name))
            .sum();
        if count_name_tokens(test.body.as_str(), &owner.name)
            != resolved_mentions + relational_mentions
        {
            return None;
        }
        let all_fold_equal = resolved.iter().all(|operands| {
            let mut holding = operands
                .iter()
                .map(|operand| operand.text.as_str())
                .filter(|operand| count_name_tokens(operand, &owner.name) > 0);
            let (Some(actual), None) = (holding.next(), holding.next()) else {
                return false;
            };
            if count_name_tokens(actual, &owner.name) != 1 {
                return false;
            }
            // Invisibility only: equal owner values at the literal arguments
            // give equal results through any one-argument wrapper
            // (`block_on(owner(..))`), so reading through it can only
            // withhold credit, never grant it.
            let Some(call_arguments) = owner_call_arguments(actual, &owner.name)
                .or_else(|| wrapped_owner_call_arguments(actual, &owner.name))
            else {
                return false;
            };
            let environment = BTreeMap::new();
            let mut values = Vec::with_capacity(call_arguments.len());
            for argument in &call_arguments {
                let Some(value) = fold_expression(argument, &environment) else {
                    return false;
                };
                values.push(value);
            }
            match (
                fold_function_text(current, &values),
                fold_function_text(&baseline, &values),
            ) {
                (Some(changed), Some(unchanged)) => changed == unchanged,
                _ => false,
            }
        });
        if !all_fold_equal {
            return None;
        }
    }
    Some(if relational_seen {
        InvisibilityReason::RelationalTruthUnchanged
    } else {
        InvisibilityReason::SameValueAtEveryAssertedInput
    })
}

/// An `assert!(left OP right)` whose comparison is `<`, `<=`, `>` , `>=` or
/// `!=` and whose sides are each an integer literal expression or one bare
/// call of the owner with literal arguments.
struct RelationalAssertion {
    operator: &'static str,
    left: String,
    right: String,
    owner_name: String,
    owner_calls: usize,
}

fn relational_assertion(oracle_text: &str, owner_name: &str) -> Option<RelationalAssertion> {
    let trimmed = oracle_text.trim_start();
    let body = trimmed
        .strip_prefix("assert!")
        .or_else(|| trimmed.strip_prefix("debug_assert!"))?
        .trim_start();
    let inside = super::delimited_contents_at(body, body.find(['(', '['])?)?;
    let condition = super::split_top_level_commas(&inside).into_iter().next()?;
    let bytes = condition.as_bytes();
    let mut depth = 0i32;
    let mut found = None;
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'"' | b'\'' | b'&' | b'|' | b'!' if depth == 0 => {
                if byte == b'!' && bytes.get(at + 1) == Some(&b'=') {
                    if found.replace((at, "!=")).is_some() {
                        return None;
                    }
                    at += 1;
                } else {
                    return None;
                }
            }
            b'=' if depth == 0 => {
                // `==` is the equality family; a lone `=` is not an assertion.
                return None;
            }
            b'<' | b'>' if depth == 0 => {
                let two = bytes.get(at + 1) == Some(&b'=');
                let operator = match (byte, two) {
                    (b'<', true) => "<=",
                    (b'<', false) => "<",
                    (b'>', true) => ">=",
                    _ => ">",
                };
                if found.replace((at, operator)).is_some() {
                    return None;
                }
                at += usize::from(two);
            }
            _ => {}
        }
        at += 1;
    }
    let (index, operator) = found?;
    let width = operator.len();
    let left = condition[..index].trim().to_string();
    let right = condition[index + width..].trim().to_string();
    let calls = count_name_tokens(&left, owner_name) + count_name_tokens(&right, owner_name);
    if calls == 0 || left.is_empty() || right.is_empty() {
        return None;
    }
    Some(RelationalAssertion {
        operator,
        left,
        right,
        owner_name: owner_name.to_string(),
        owner_calls: calls,
    })
}

impl RelationalAssertion {
    fn side(&self, text: &str, body: &str) -> Option<i128> {
        let environment = BTreeMap::new();
        match count_name_tokens(text, &self.owner_name) {
            0 => fold_expression(text, &environment),
            1 => {
                let arguments = owner_call_arguments(text, &self.owner_name)?;
                let mut values = Vec::with_capacity(arguments.len());
                for argument in &arguments {
                    values.push(fold_expression(argument, &environment)?);
                }
                fold_function_text(body, &values)
            }
            _ => None,
        }
    }

    fn holds(&self, body: &str) -> Option<bool> {
        let left = self.side(&self.left, body)?;
        let right = self.side(&self.right, body)?;
        Some(match self.operator {
            "<" => left < right,
            "<=" => left <= right,
            ">" => left > right,
            ">=" => left >= right,
            _ => left != right,
        })
    }

    /// Only a completed fold on both versions that gives the same truth
    /// value counts; anything unfolded keeps today's credit.
    fn truth_unchanged(&self, current: &str, baseline: &str) -> bool {
        matches!(
            (self.holds(current), self.holds(baseline)),
            (Some(changed), Some(unchanged)) if changed == unchanged
        )
    }
}

/// The compared operands of an equality assertion (either order, or the two
/// sides of `assert!(a == b)`), each bare identifier replaced by the one
/// `let` initializer that binds it in `test_body`. Empty when the assertion
/// is not an equality shape.
fn assertion_operands(oracle_text: &str, test_body: &str) -> Vec<ResolvedOperand> {
    let operands: Vec<String> = match equality_assertion_shape(oracle_text) {
        Some((_, arguments)) => arguments.into_iter().take(2).collect(),
        None => crate::analysis::classify::equality_condition_operands(oracle_text)
            .map(Vec::from)
            .unwrap_or_default(),
    };
    operands
        .into_iter()
        .map(|operand| {
            let name = operand.trim().to_string();
            match let_initializer(test_body, &name) {
                Some(text) => ResolvedOperand {
                    text,
                    binding: Some(name),
                },
                None => ResolvedOperand {
                    text: operand,
                    binding: None,
                },
            }
        })
        .collect()
}

/// A compared operand after `let` resolution; `binding` names the `let` it
/// was read through so a shared binding is counted once.
struct ResolvedOperand {
    text: String,
    binding: Option<String>,
}

/// The initializer of the single `let [mut] name = init;` in `body`.
fn let_initializer(body: &str, name: &str) -> Option<String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    // A later `name = ..`, `name += ..` or `&mut name` changes the compared
    // value after the initializer, so the initializer alone is not it.
    let mutated = body.match_indices(name).any(|(at, _)| {
        let before = body[..at].chars().next_back();
        let after = body[at + name.len()..].trim_start();
        let whole = !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !after.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_');
        whole
            && (body[..at].trim_end().ends_with("&mut")
                || (after.starts_with('=')
                    && !after.starts_with("==")
                    && !body[..at].trim_end().ends_with("let")
                    && !body[..at].trim_end().ends_with("mut"))
                || after.starts_with("+=")
                || after.starts_with("-=")
                || after.starts_with("*=")
                || after.starts_with("/="))
    });
    if mutated {
        return None;
    }
    let mut found = None;
    for (at, _) in body.match_indices("let ") {
        if body[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            continue;
        }
        let rest = body[at + 4..].trim_start();
        let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
        let Some(after) = rest.strip_prefix(name) else {
            continue;
        };
        // A type annotation (`let name: u32 = ..`) binds the same value.
        let after = after.trim_start();
        let after = match after.strip_prefix(':') {
            Some(annotated) => match annotated.find(['=', ';']) {
                Some(at) if annotated[at..].starts_with('=') => &annotated[at..],
                _ => continue,
            },
            None => after,
        };
        let Some(init) = after.strip_prefix('=') else {
            continue;
        };
        let end = init.find(';')?;
        if found.replace(init[..end].trim().to_string()).is_some() {
            return None;
        }
    }
    found
}

/// How many whole-identifier occurrences of `name` the `text` holds.
fn count_name_tokens(text: &str, name: &str) -> usize {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| *token == name)
        .count()
}

/// Whether `current` and `baseline` fold to different values at some call
/// over a grid of integer arguments (every parameter takes each of
/// [`SEPARATION_SMALL_GRID`]; a single parameter also takes the wider
/// [`SEPARATION_WIDE_GRID`], so a change a divisor or a saturation hides below
/// the small values still counts as a real difference). More than
/// [`SEPARATION_MAX_PARAMETERS`] parameters are not searched.
fn versions_differ_on_small_inputs(current: &str, baseline: &str) -> bool {
    const SEPARATION_SMALL_GRID: [i128; 8] = [0, 1, 2, 3, 5, 6, 7, 10];
    const SEPARATION_WIDE_GRID: [i128; 16] = [
        0, 1, 2, 3, 5, 6, 7, 10, 16, 31, 64, 100, 255, 1000, 4096, 65536,
    ];
    const SEPARATION_MAX_PARAMETERS: usize = 3;
    let Some(parameters) = parse_clean_source_file(current)
        .and_then(|parse| {
            parse
                .tree()
                .syntax()
                .descendants()
                .find_map(ast::Fn::cast)
                .and_then(|function| function.param_list())
        })
        .map(|list| list.params().count())
    else {
        return false;
    };
    if parameters == 0 || parameters > SEPARATION_MAX_PARAMETERS {
        return false;
    }
    let grid: &[i128] = if parameters == 1 {
        &SEPARATION_WIDE_GRID
    } else {
        &SEPARATION_SMALL_GRID
    };
    let mut arguments = vec![grid[0]; parameters];
    let mut counters = vec![0usize; parameters];
    loop {
        for (argument, counter) in arguments.iter_mut().zip(&counters) {
            *argument = grid[*counter];
        }
        if let (Some(changed), Some(unchanged)) = (
            fold_function_text(current, &arguments),
            fold_function_text(baseline, &arguments),
        ) && changed != unchanged
        {
            return true;
        }
        // Advance the odometer.
        let mut position = 0;
        loop {
            let Some(counter) = counters.get_mut(position) else {
                return false;
            };
            *counter += 1;
            if *counter < grid.len() {
                break;
            }
            *counter = 0;
            position += 1;
        }
    }
}

/// The one-line disclosure a receipt carries when the oracle is downgraded.
pub(super) fn contradiction_summary(expected: &str, evaluated: i128) -> String {
    format!("{CONTRADICTION_DISCLOSURE} (asserts {expected}, owner folds to {evaluated})")
}

/// Whether the owner's name in `test`'s body is not the owner as a value for
/// `oracle_text`: a `let` binding of the name (a local shadow the bare-name
/// fold cannot see through, #6701 review), or — scoped to the assertion's own
/// position — a same-name `fn` block or an import the shared call-identity
/// resolver reads as another definition (RIPR-SPEC-0240, #10162 review). The
/// per-assertion item reading lives in
/// [`crate::analysis::classify::indirect_owner::oracle_owner_call_is_shadowed`];
/// a body the shared parser refuses proves no item there, so only the `let`
/// answer stands.
pub(super) fn bare_owner_call_is_shadowed(
    test: &crate::analysis::facts::TestFact,
    owner: &FunctionSummary,
    oracle_text: &str,
    index: &RustIndex,
) -> bool {
    if owner.name.is_empty() {
        return false;
    }
    if test
        .let_bindings
        .iter()
        .any(|binding| binding.name == owner.name)
    {
        return true;
    }
    oracle_owner_call_is_shadowed(test.body.as_str(), oracle_text, owner, test, index) == Some(true)
}

/// Whether the test body itself declares a same-name `fn` item: by definition
/// another item, wherever its block sits. Read through the shared parser this
/// crate already trusts for item scope (#10162).
fn body_declares_same_name_fn(test: &TestSummary, owner_name: &str) -> bool {
    use ra_ap_syntax::{ast, ast::HasName};
    let Some(parse) = parse_clean_source_file(test.body.as_str()) else {
        return false;
    };
    parse.tree().syntax().descendants().any(|node| {
        ast::Fn::cast(node)
            .and_then(|function| function.name())
            .is_some_and(|declared| declared.text() == owner_name)
    })
}

/// Which equality macro family the assertion uses and its first operands.
/// `Some((true, ..))` is the `assert_ne!` family (the asserted relation is
/// inequality); `None` is not an equality assertion at all.
fn equality_assertion_shape(oracle_text: &str) -> Option<(bool, Vec<String>)> {
    EQUALITY_SHAPE_MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        if let Some(shape) = memo.get(oracle_text) {
            return shape.clone();
        }
        let shape = equality_assertion_shape_uncached(oracle_text);
        if memo.len() >= EQUALITY_SHAPE_MEMO_ENTRIES {
            memo.clear();
        }
        memo.insert(oracle_text.to_string(), shape.clone());
        shape
    })
}

/// Per-thread memo of [`equality_assertion_shape`], keyed by the full oracle
/// text (equality, never a hash), so a hit is exactly the shape a fresh call
/// returns. The shape reads only the assertion text, but the verdict asks it
/// once per (seam, related test, oracle) and again per rstest row; masking and
/// splitting the same assertion each time was a quarter of cold repo-exposure
/// evidence on regex (#10932).
const EQUALITY_SHAPE_MEMO_ENTRIES: usize = 4096;

type EqualityShape = Option<(bool, Vec<String>)>;

thread_local! {
    static EQUALITY_SHAPE_MEMO: std::cell::RefCell<std::collections::HashMap<String, EqualityShape>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn equality_assertion_shape_uncached(oracle_text: &str) -> Option<(bool, Vec<String>)> {
    let trimmed = oracle_text.trim_start();
    let starts_with_any = |names: &[&str]| {
        names.iter().any(|name| {
            trimmed.starts_with(name) || trimmed.starts_with(format!("::{name}").as_str())
        })
    };
    let asserts_inequality = starts_with_any(&["assert_ne!", "debug_assert_ne!"]);
    let asserts_equality = starts_with_any(&["assert_eq!", "debug_assert_eq!"]);
    if !asserts_equality && !asserts_inequality {
        return None;
    }
    let arguments = equality_assertion_arguments(oracle_text)?;
    Some((asserts_inequality, arguments))
}

/// The call arguments when `operand` is a bare-name call to `owner_name`
/// (`owner(..)`). See the in-function guard for why receivers and
/// qualified paths stay not evaluable.
fn owner_call_arguments(operand: &str, owner_name: &str) -> Option<Vec<String>> {
    let trimmed = operand.trim();
    let open = super::named_call_open_paren_index(trimmed, owner_name)?;
    let before = trimmed[..open - owner_name.len()].trim_end();
    // A bare name only: a method receiver names a different entity than the
    // free function the seam owns, and a qualified path cannot be resolved
    // to the owner's module from the assertion alone, so both stay not
    // evaluable rather than folding against a body the assertion never
    // called (#6701 review).
    if !before.is_empty() {
        return None;
    }
    let inside = super::delimited_contents_at(trimmed, open)?;
    // The call must consume the whole operand: `owner(1).abs()` or
    // `owner(1) + 1` asserts a value the fold of `owner(1)` never computed,
    // so a trailing expression leaves the pair not evaluable instead of
    // contradicting against a transformed result.
    let bytes = trimmed.as_bytes();
    let mut depth = 0i32;
    let mut close = None;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    if !trimmed[close + 1..].trim().is_empty() {
        return None;
    }
    Some(super::split_top_level_commas(&inside))
}

/// The owner call's arguments when `operand` is `owner(..).await` or
/// `wrapper(owner(..))` with a plain identifier wrapper and nothing else
/// around the call.
fn wrapped_owner_call_arguments(operand: &str, owner_name: &str) -> Option<Vec<String>> {
    let trimmed = operand.trim();
    // `owner(..).await` is the same call read through the executor.
    if let Some(call) = trimmed.strip_suffix(".await") {
        return owner_call_arguments(call, owner_name);
    }
    let name_end = trimmed.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))?;
    if name_end == 0 || trimmed[..name_end] == *owner_name {
        return None;
    }
    let rest = trimmed[name_end..].trim_start();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    owner_call_arguments(inner, owner_name)
}

/// Evaluate the owner's body as a pure integer function of `arguments`.
/// Free functions only: a method or nested fn is reached through a receiver
/// or closure that the bare-path assertion does not name.
fn fold_owner_call(owner: &FunctionSummary, arguments: &[i128]) -> Option<i128> {
    fold_function_text(owner.body.as_str(), arguments)
}

/// [`fold_owner_call`] over the function's source text, so a baseline body
/// can be folded beside the current one.
fn fold_function_text(body: &str, arguments: &[i128]) -> Option<i128> {
    match fold_function_value(body, arguments)? {
        Value::Int(value) => Some(value),
        Value::Bool(_) => None,
    }
}

/// Keep the parser's result type so the narrow boolean caller can inspect it.
/// Existing integer callers still reject a boolean through `fold_function_text`.
fn fold_function_value(body: &str, arguments: &[i128]) -> Option<Value> {
    let parse = parse_clean_source_file(body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    for ancestor in function.syntax().ancestors().skip(1) {
        if ast::Impl::can_cast(ancestor.kind())
            || ast::Trait::can_cast(ancestor.kind())
            || ast::Fn::can_cast(ancestor.kind())
        {
            return None;
        }
    }
    let param_list = function.param_list()?;
    if param_list.self_param().is_some() {
        return None;
    }
    let mut names = Vec::new();
    let mut widths = Vec::new();
    for param in param_list.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        // A subpattern (`x @ (a, b)`) does not bind the written name alone.
        if ident.pat().is_some() {
            return None;
        }
        names.push(ident.name()?.text().to_string());
        // A declared-width parameter bounds the inputs the real function
        // accepts; an argument outside it names a call that cannot compile.
        let width = param
            .ty()
            .and_then(|ty| declared_integer_range(&ty.syntax().text().to_string()));
        widths.push(width);
    }
    if names.len() != arguments.len() {
        return None;
    }
    for (width, value) in widths.iter().zip(arguments.iter().copied()) {
        if let Some((low, high)) = width
            && !(*low..=*high).contains(&value)
        {
            return None;
        }
    }
    // The fold uses the return type's declared width when it is a
    // recognized integer type: release builds wrap at that width, so a
    // mathematical result outside it does not describe a value the owner
    // can return and the pair stays not evaluable instead of contradicting
    // a passing wrapping assertion (#6701 review).
    // No recognized return type: the fold keeps exact i128 arithmetic,
    // which can only withhold credit.
    let fold_range = function
        .ret_type()
        .and_then(|ret| declared_integer_range(&ret.syntax().text().to_string()));
    let block = function.body()?;
    let environment: BTreeMap<String, i128> =
        names.into_iter().zip(arguments.iter().copied()).collect();
    // Comments and strings carry no arithmetic; masking them first keeps the
    // tokenizer away from brace-bearing content inside either.
    let masked =
        crate::analysis::extract::mask_comments_and_strings(&block.syntax().text().to_string());
    let tokens = tokenize(&masked)?;
    let uniform = fold_range.filter(|range| widths.iter().all(|width| *width == Some(*range)));
    fold_block(&tokens, &environment, fold_range, uniform)
}

/// The inclusive value range a recognized Rust integer type admits, or
/// `None` for anything else: `u128`/`i128` exceed the fold's i128
/// arithmetic, and pointer-sized types depend on the analysis target's
/// width, which no established input names here — a fold at the wrong
/// width could contradict a passing wrapping assertion, so they stay not
/// evaluable (#6701 review).
fn declared_integer_range(ty: &str) -> Option<(i128, i128)> {
    // The parser's return-type node carries its `->` arrow; parameter types
    // are bare.
    let compact = ty.replace(' ', "").trim_start_matches("->").to_string();
    match compact.as_str() {
        "u8" => Some((0, i128::from(u8::MAX))),
        "u16" => Some((0, i128::from(u16::MAX))),
        "u32" => Some((0, i128::from(u32::MAX))),
        "u64" => Some((0, i128::from(u64::MAX))),
        "i8" => Some((i128::from(i8::MIN), i128::from(i8::MAX))),
        "i16" => Some((i128::from(i16::MIN), i128::from(i16::MAX))),
        "i32" => Some((i128::from(i32::MIN), i128::from(i32::MAX))),
        "i64" => Some((i128::from(i64::MIN), i128::from(i64::MAX))),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Int(i128),
    Ident(String),
    Symbol(&'static str),
}

const INTEGER_SUFFIXES: [&str; 12] = [
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// Tokenize masked Rust expression text. Any byte the fold grammar cannot
/// mean (`?`, `|`, `.`, `:`), a non-suffix identifier glued to a number, or
/// a non-ASCII byte fails the whole fold.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        // `0xF0`, `0b1010`: radix literals, for the masks and shifts the fold
        // evaluates. A type suffix after one is left to fail the fold.
        if byte == b'0'
            && let Some(radix) = match bytes.get(index + 1) {
                Some(b'x') => Some(16),
                Some(b'b') => Some(2),
                _ => None,
            }
        {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let digits = text[start..end].replace('_', "");
            let value = i128::from_str_radix(&digits, radix).ok()?;
            tokens.push(Token::Int(value));
            index = end;
            continue;
        }
        if byte.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
                index += 1;
            }
            let digits_end = index;
            if index < bytes.len() && (bytes[index].is_ascii_alphabetic() || bytes[index] == b'_') {
                let suffix_start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                if !INTEGER_SUFFIXES.contains(&&text[suffix_start..index]) {
                    return None;
                }
            }
            let digits = text[start..digits_end].replace('_', "");
            let value = digits.parse::<i128>().ok()?;
            tokens.push(Token::Int(value));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token::Ident(text[start..index].to_string()));
            continue;
        }
        let two_byte_symbol = |pair: &[u8]| match pair {
            [b'=', b'='] => "==",
            [b'!', b'='] => "!=",
            [b'<', b'='] => "<=",
            [b'>', b'='] => ">=",
            [b'&', b'&'] => "&&",
            [b'|', b'|'] => "||",
            [b'<', b'<'] => "<<",
            [b'>', b'>'] => ">>",
            _ => "",
        };
        let symbol = if index + 2 <= bytes.len() {
            two_byte_symbol(&bytes[index..index + 2])
        } else {
            ""
        };
        let symbol = if !symbol.is_empty() {
            symbol
        } else if !byte.is_ascii() {
            return None;
        } else {
            match byte {
                b'<' => "<",
                b'>' => ">",
                b'+' => "+",
                b'-' => "-",
                b'*' => "*",
                b'/' => "/",
                b'%' => "%",
                b'&' => "&",
                b'|' => "|",
                b'^' => "^",
                b'!' => "!",
                b'(' => "(",
                b')' => ")",
                b'{' => "{",
                b'}' => "}",
                b',' => ",",
                b';' => ";",
                // A method call's dot (`x.min(5)`): it follows a value and
                // precedes a name. A float's dot (`0.85`) is followed by a
                // digit and still fails the whole fold.
                b'.' if matches!(
                    tokens.last(),
                    Some(Token::Int(_) | Token::Ident(_) | Token::Symbol(")"))
                ) && bytes
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_alphabetic()) =>
                {
                    "."
                }
                _ => return None,
            }
        };
        index += symbol.len();
        tokens.push(Token::Symbol(symbol));
    }
    Some(tokens)
}

/// A folded value: the only two types a pure integer fold produces.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Int(i128),
    Bool(bool),
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    environment: &'a BTreeMap<String, i128>,
    /// The owner return type's declared width, when recognized. Every
    /// integer produced inside the body must fit it: release builds wrap at
    /// the declared width, so a mathematical value outside the range does
    /// not describe a value the owner can return.
    range: Option<(i128, i128)>,
    /// The shared declared range when every parameter and the return type
    /// are the same recognized integer type. The shifts and the saturating
    /// methods depend on the operand's own width, which the return range
    /// stands for only then: a comparison or a differently typed parameter
    /// can hold a value of another width.
    uniform: Option<(i128, i128)>,
}

impl<'a> Parser<'a> {
    /// The bit width of the uniform integer type, when there is one.
    fn uniform_bits(&self) -> Option<u32> {
        let (low, high) = self.uniform?;
        let magnitude = 128 - u128::try_from(high).ok()?.leading_zeros();
        Some(if low < 0 { magnitude + 1 } else { magnitude })
    }

    fn check_range(&self, value: i128) -> Option<i128> {
        match self.range {
            Some((low, high)) if !(low..=high).contains(&value) => None,
            _ => Some(value),
        }
    }
}

impl<'a> Parser<'a> {
    fn peek_symbol(&self) -> Option<&'static str> {
        match self.tokens.get(self.position) {
            Some(Token::Symbol(symbol)) => Some(symbol),
            _ => None,
        }
    }

    fn peek_ident(&self) -> Option<&str> {
        match self.tokens.get(self.position) {
            Some(Token::Ident(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        if self.peek_symbol() == Some(symbol) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: &str) -> Option<()> {
        self.eat_symbol(symbol).then_some(())
    }

    /// A `{ .. }` block: a sequence of expression statements and one optional
    /// trailing expression. `let` bindings, `return`, and any statement shape
    /// outside the fold leave it unevaluated instead of being guessed at.
    fn parse_block_value(&mut self) -> Option<Value> {
        self.expect_symbol("{")?;
        let mut value = None;
        loop {
            match self.tokens.get(self.position) {
                None => return None,
                Some(Token::Symbol("}")) => {
                    self.position += 1;
                    return value;
                }
                _ => {}
            }
            if matches!(self.peek_ident(), Some("let" | "return")) {
                return None;
            }
            value = Some(self.parse_expression()?);
            match self.peek_symbol() {
                Some(";") => {
                    self.position += 1;
                    value = None;
                }
                Some("}") => {}
                _ => return None,
            }
        }
    }

    fn parse_expression(&mut self) -> Option<Value> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<Value> {
        let mut left = self.parse_and()?;
        while self.eat_symbol("||") {
            // Rust short-circuits: a deciding LHS never evaluates the RHS,
            // so an undefined RHS (a division by zero the run never reaches)
            // must not make the fold silent (#6701 review).
            if bool_value(left)? {
                self.skip_expression()?;
                left = Value::Bool(true);
            } else {
                left = Value::Bool(bool_value(self.parse_and()?)?);
            }
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Value> {
        let mut left = self.parse_comparison()?;
        while self.eat_symbol("&&") {
            if !bool_value(left)? {
                self.skip_expression()?;
                left = Value::Bool(false);
            } else {
                left = Value::Bool(bool_value(self.parse_comparison()?)?);
            }
        }
        Some(left)
    }

    /// Consumes one expression's tokens without evaluating it (the untaken
    /// branch of an `if` or the short-circuited side of `&&`/`||`): balanced
    /// delimiters, no semantics. `let`/`return`/calls stay un-parseable so
    /// a skipped region cannot smuggle new grammar in.
    fn skip_expression(&mut self) -> Option<()> {
        self.skip_or()
    }

    fn skip_or(&mut self) -> Option<()> {
        self.skip_and()?;
        while self.eat_symbol("||") {
            self.skip_and()?;
        }
        Some(())
    }

    fn skip_and(&mut self) -> Option<()> {
        self.skip_comparison()?;
        while self.eat_symbol("&&") {
            self.skip_comparison()?;
        }
        Some(())
    }

    fn skip_comparison(&mut self) -> Option<()> {
        self.skip_bitwise()?;
        if matches!(
            self.peek_symbol(),
            Some("==" | "!=" | "<" | "<=" | ">" | ">=")
        ) {
            self.position += 1;
            self.skip_bitwise()?;
        }
        Some(())
    }

    fn skip_bitwise(&mut self) -> Option<()> {
        self.skip_additive()?;
        while matches!(self.peek_symbol(), Some("|" | "^" | "&" | "<<" | ">>")) {
            self.position += 1;
            self.skip_additive()?;
        }
        Some(())
    }

    fn skip_additive(&mut self) -> Option<()> {
        self.skip_multiplicative()?;
        while matches!(self.peek_symbol(), Some("+" | "-")) {
            self.position += 1;
            self.skip_multiplicative()?;
        }
        Some(())
    }

    fn skip_multiplicative(&mut self) -> Option<()> {
        self.skip_unary()?;
        while matches!(self.peek_symbol(), Some("*" | "/" | "%")) {
            self.position += 1;
            self.skip_unary()?;
        }
        Some(())
    }

    fn skip_unary(&mut self) -> Option<()> {
        if matches!(self.peek_symbol(), Some("-" | "!")) {
            self.position += 1;
            return self.skip_unary();
        }
        self.skip_primary()
    }

    fn skip_primary(&mut self) -> Option<()> {
        match self.tokens.get(self.position) {
            Some(Token::Symbol("(")) => {
                self.position += 1;
                self.skip_or()?;
                self.expect_symbol(")")
            }
            Some(Token::Symbol("{")) => self.skip_balanced_braces(),
            Some(Token::Ident(name)) if name == "if" => {
                self.position += 1;
                self.skip_or()?;
                self.skip_balanced_braces()?;
                if self.peek_ident() == Some("else") {
                    self.position += 1;
                    if self.peek_ident() == Some("if") {
                        self.position += 1;
                        self.skip_or()?;
                        self.skip_balanced_braces()?;
                    } else {
                        self.skip_balanced_braces()?;
                    }
                }
                Some(())
            }
            Some(Token::Int(_) | Token::Ident(_)) => {
                self.position += 1;
                Some(())
            }
            _ => None,
        }
    }

    /// Consumes one balanced `{ .. }` region without evaluating it.
    fn skip_balanced_braces(&mut self) -> Option<()> {
        self.expect_symbol("{")?;
        let mut depth = 1i32;
        while depth > 0 {
            match self.tokens.get(self.position) {
                Some(Token::Symbol("{")) => depth += 1,
                Some(Token::Symbol("}")) => depth -= 1,
                None => return None,
                _ => {}
            }
            self.position += 1;
        }
        Some(())
    }

    fn parse_comparison(&mut self) -> Option<Value> {
        let left = self.parse_bit_or()?;
        let Some(op) = self.peek_symbol() else {
            return Some(left);
        };
        if !matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
            return Some(left);
        }
        self.position += 1;
        let right = self.parse_bit_or()?;
        match (left, right) {
            (Value::Int(left), Value::Int(right)) => {
                let result = match op {
                    "==" => left == right,
                    "!=" => left != right,
                    "<" => left < right,
                    "<=" => left <= right,
                    ">" => left > right,
                    _ => left >= right,
                };
                Some(Value::Bool(result))
            }
            (Value::Bool(left), Value::Bool(right)) if matches!(op, "==" | "!=") => {
                Some(Value::Bool(if op == "==" {
                    left == right
                } else {
                    left != right
                }))
            }
            _ => None,
        }
    }

    /// One left-associative integer level of the bitwise operators, between
    /// comparison and addition in Rust's precedence: `|` below `^` below `&`
    /// below the shifts. `next` parses the tighter level.
    fn parse_bit_level(
        &mut self,
        symbols: &[&'static str],
        next: fn(&mut Self) -> Option<Value>,
    ) -> Option<Value> {
        let mut left = next(self)?;
        loop {
            let Some(op) = self.peek_symbol().filter(|symbol| symbols.contains(symbol)) else {
                return Some(left);
            };
            self.position += 1;
            let right = next(self)?;
            let (Value::Int(left_value), Value::Int(right_value)) = (left, right) else {
                return None;
            };
            let value = match op {
                "|" => left_value | right_value,
                "^" => left_value ^ right_value,
                "&" => left_value & right_value,
                // A shift past the integer's width panics in a debug build
                // and loses bits in a release build; an amount at or past the
                // width, or a left shift whose result overflows the declared
                // width, is not a value the owner returns. The width is known
                // only for a uniform integer type.
                "<<" | ">>" => {
                    let bits = self.uniform_bits()?;
                    let amount = u32::try_from(right_value)
                        .ok()
                        .filter(|amount| *amount < bits)?;
                    if op == "<<" {
                        left_value.checked_mul(1i128.checked_shl(amount)?)?
                    } else {
                        left_value >> amount
                    }
                }
                _ => return None,
            };
            left = Value::Int(self.check_range(value)?);
        }
    }

    fn parse_bit_or(&mut self) -> Option<Value> {
        self.parse_bit_level(&["|"], Self::parse_bit_xor)
    }

    fn parse_bit_xor(&mut self) -> Option<Value> {
        self.parse_bit_level(&["^"], Self::parse_bit_and)
    }

    fn parse_bit_and(&mut self) -> Option<Value> {
        self.parse_bit_level(&["&"], Self::parse_shift)
    }

    fn parse_shift(&mut self) -> Option<Value> {
        self.parse_bit_level(&["<<", ">>"], Self::parse_additive)
    }

    fn parse_additive(&mut self) -> Option<Value> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_symbol() {
                Some("+") => "+",
                Some("-") => "-",
                _ => return Some(left),
            };
            self.position += 1;
            let right = self.parse_multiplicative()?;
            let Value::Int(left_value) = left else {
                return None;
            };
            let Value::Int(right_value) = right else {
                return None;
            };
            left = Value::Int(self.check_range(match op {
                "+" => left_value.checked_add(right_value)?,
                _ => left_value.checked_sub(right_value)?,
            })?);
        }
    }

    fn parse_multiplicative(&mut self) -> Option<Value> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_symbol() {
                Some("*") => "*",
                Some("/") => "/",
                Some("%") => "%",
                _ => return Some(left),
            };
            self.position += 1;
            let right = self.parse_unary()?;
            let Value::Int(left_value) = left else {
                return None;
            };
            let Value::Int(right_value) = right else {
                return None;
            };
            left = Value::Int(self.check_range(match op {
                "*" => left_value.checked_mul(right_value)?,
                "/" => left_value.checked_div(right_value)?,
                _ => left_value.checked_rem(right_value)?,
            })?);
        }
    }

    fn parse_unary(&mut self) -> Option<Value> {
        if self.eat_symbol("-") {
            let Value::Int(value) = self.parse_unary()? else {
                return None;
            };
            return Some(Value::Int(self.check_range(value.checked_neg()?)?));
        }
        if self.eat_symbol("!") {
            let value = self.parse_unary()?;
            return Some(Value::Bool(!bool_value(value)?));
        }
        let mut value = self.parse_primary()?;
        while self.eat_symbol(".") {
            value = self.parse_integer_method(value)?;
        }
        Some(value)
    }

    /// `.pow(n)`, `.min(n)` or `.max(n)` on an integer value: the only
    /// methods the fold evaluates. Their receivers are integers by
    /// construction (every bound name and literal is one), so the std
    /// integer methods are the ones that run.
    fn parse_integer_method(&mut self, receiver: Value) -> Option<Value> {
        let Value::Int(receiver) = receiver else {
            return None;
        };
        let name = self.peek_ident()?.to_string();
        self.position += 1;
        self.expect_symbol("(")?;
        let Value::Int(argument) = self.parse_expression()? else {
            return None;
        };
        let upper = if name == "clamp" {
            self.expect_symbol(",")?;
            let Value::Int(upper) = self.parse_expression()? else {
                return None;
            };
            Some(upper)
        } else {
            None
        };
        self.expect_symbol(")")?;
        // The saturating forms clamp at the receiver's own range, known only
        // when every parameter and the return type share one integer type.
        let value = match name.as_str() {
            "pow" => receiver.checked_pow(u32::try_from(argument).ok()?)?,
            "min" => receiver.min(argument),
            "max" => receiver.max(argument),
            // `clamp` panics when the bounds are inverted.
            "clamp" => {
                let upper = upper?;
                if argument > upper {
                    return None;
                }
                receiver.clamp(argument, upper)
            }
            "saturating_add" | "saturating_sub" => {
                let (low, high) = self.uniform?;
                let exact = if name == "saturating_add" {
                    receiver.checked_add(argument)?
                } else {
                    receiver.checked_sub(argument)?
                };
                exact.clamp(low, high)
            }
            _ => return None,
        };
        Some(Value::Int(self.check_range(value)?))
    }

    fn parse_primary(&mut self) -> Option<Value> {
        match self.tokens.get(self.position) {
            Some(Token::Symbol("(")) => {
                self.position += 1;
                let value = self.parse_expression()?;
                self.expect_symbol(")")?;
                Some(value)
            }
            Some(Token::Symbol("{")) => self.parse_block_value(),
            // Keywords lex as identifiers: `if` opens the fold's only
            // control form.
            Some(Token::Ident(name)) if name == "if" => {
                self.position += 1;
                self.parse_if_tail()
            }
            Some(Token::Int(value)) => {
                let value = *value;
                self.position += 1;
                Some(Value::Int(value))
            }
            Some(Token::Ident(name)) => {
                let name = name.clone();
                self.position += 1;
                match name.as_str() {
                    "true" => return Some(Value::Bool(true)),
                    "false" => return Some(Value::Bool(false)),
                    _ => {}
                }
                // Calls, method chains, paths and field access are outside
                // the fold's scope; only bound parameters evaluate.
                match self.peek_symbol() {
                    Some("(" | "::") => None,
                    _ => self.environment.get(&name).copied().map(Value::Int),
                }
            }
            _ => None,
        }
    }

    /// The `if` keyword has been consumed; parse the condition, the taken
    /// block, and (when present) the `else` arm. An `if` without `else` is
    /// evaluable only when the condition folds true.
    fn parse_if_tail(&mut self) -> Option<Value> {
        let condition = bool_value(self.parse_expression()?)?;
        if condition {
            let taken = self.parse_block_value()?;
            // The untaken arm is consumed syntactically but never folded: a
            // division by zero or overflow the run would never reach must
            // not silence the fold (#6701 review).
            if self.peek_ident() == Some("else") {
                self.position += 1;
                if self.peek_ident() == Some("if") {
                    self.position += 1;
                    self.skip_if_tail_syntax()?;
                } else {
                    self.skip_balanced_braces()?;
                }
            }
            Some(taken)
        } else {
            // Skip the taken block syntactically, then evaluate the else
            // arm. An `if` without `else` is not evaluable when false.
            self.skip_balanced_braces()?;
            if self.peek_ident() != Some("else") {
                return None;
            }
            self.position += 1;
            let alternative = if self.peek_ident() == Some("if") {
                self.position += 1;
                self.parse_if_tail()?
            } else {
                self.parse_block_value()?
            };
            Some(alternative)
        }
    }

    /// Skips an `else if` chain syntactically (the `if` already consumed).
    fn skip_if_tail_syntax(&mut self) -> Option<()> {
        self.skip_or()?;
        self.skip_balanced_braces()?;
        if self.peek_ident() == Some("else") {
            self.position += 1;
            if self.peek_ident() == Some("if") {
                self.position += 1;
                self.skip_if_tail_syntax()?;
            } else {
                self.skip_balanced_braces()?;
            }
        }
        Some(())
    }
}

fn bool_value(value: Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(value),
        Value::Int(_) => None,
    }
}

/// Fold a standalone expression (an assertion operand) with no bound names.
fn fold_expression(text: &str, environment: &BTreeMap<String, i128>) -> Option<i128> {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let tokens = tokenize(&masked)?;
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        environment,
        range: None,
        uniform: None,
    };
    let value = parser.parse_expression()?;
    if parser.position != tokens.len() {
        return None;
    }
    match value {
        Value::Int(value) => Some(value),
        Value::Bool(_) => None,
    }
}

/// Fold a `{ .. }` block with `environment` as the parameter bindings and
/// `range` as the return type's declared width (no width when `None`).
fn fold_block(
    tokens: &[Token],
    environment: &BTreeMap<String, i128>,
    range: Option<(i128, i128)>,
    uniform: Option<(i128, i128)>,
) -> Option<Value> {
    let mut parser = Parser {
        tokens,
        position: 0,
        environment,
        range,
        uniform,
    };
    let value = parser.parse_block_value()?;
    if parser.position != tokens.len() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};

    #[test]
    fn equality_shape_memo_returns_what_a_fresh_shape_would() {
        let texts = [
            "assert_eq!(discounted_total(1, 2)\n, 3\n)",
            "assert_ne!(discounted_total(1, 2)\n, 4\n)",
            "debug_assert_eq!(f(1)\n, 1\n)",
            "assert!(flag)",
            "assert_eq!(discounted_total(1, 2)\n, 3\n)",
        ];
        for text in texts {
            let fresh = equality_assertion_shape_uncached(text);
            assert_eq!(equality_assertion_shape(text), fresh, "first ask: {text}");
            assert_eq!(equality_assertion_shape(text), fresh, "memo hit: {text}");
        }
        assert!(
            equality_assertion_shape("assert!(flag)").is_none(),
            "a non-equality assertion stays not an equality shape"
        );
        assert_eq!(
            equality_assertion_shape("assert_ne!(f(1)\n, 2\n)").map(|shape| shape.0),
            Some(true),
            "the assert_ne family keeps its inequality flag through the memo"
        );
    }

    #[test]
    fn equality_shape_memo_answers_a_repeat_from_the_memo() {
        // A planted entry no fresh computation could produce proves the
        // second ask reads the memo instead of recomputing the shape.
        let text = "assert_eq!(memo_probe(1)\n, 2\n)";
        let planted: EqualityShape = Some((true, vec!["planted".to_string()]));
        assert_ne!(equality_assertion_shape_uncached(text), planted);
        EQUALITY_SHAPE_MEMO
            .with(|memo| memo.borrow_mut().insert(text.to_string(), planted.clone()));
        assert_eq!(equality_assertion_shape(text), planted);
        let entries = EQUALITY_SHAPE_MEMO.with(|memo| memo.borrow().len());
        assert_eq!(equality_assertion_shape(text), planted);
        assert_eq!(
            EQUALITY_SHAPE_MEMO.with(|memo| memo.borrow().len()),
            entries,
            "a hit must not add an entry"
        );
    }

    fn verdict_for_owner(owner: &str, assertion: &str) -> Result<ExactValueVerdict, String> {
        let adapter = RaRustSyntaxAdapter;
        let facts = adapter
            .summarize_file(std::path::Path::new("src/lib.rs"), &format!("{owner}\n"))
            .map_err(|error| format!("owner source summarizes: {error}"))?;
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "discounted_total")
            .ok_or("owner function is indexed")?;
        Ok(exact_value_assertion_verdict(Some(owner), assertion, false))
    }

    const BOUNDARY_OWNER: &str = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents * 70 / 100
    } else {
        amount_cents
    }
}
"#;

    #[test]
    fn wrongval_assertion_of_the_mutants_value_is_contradicted() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(5_000, 5_000), 5_000);",
        );
        assert_eq!(
            verdict?,
            ExactValueVerdict::Contradicted {
                expected: "5_000".to_string(),
                evaluated: 3_500,
            }
        );
        Ok(())
    }

    #[test]
    fn correct_boundary_literal_is_consistent() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(5_000, 5_000), 3_500);",
        );
        assert_eq!(verdict?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn non_boundary_literal_is_consistent() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(1_000, 5_000), 1_000);",
        );
        assert_eq!(verdict?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn non_literal_expected_value_is_not_evaluable() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(5_000, 5_000), expected_total());",
        );
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn non_literal_call_argument_is_not_evaluable() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(discounted_total(amount, 5_000), 3_500);",
        );
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn qualified_owner_path_call_is_not_evaluable() -> Result<(), String> {
        // A qualified path may name a same-named function in another module;
        // the assertion alone cannot resolve it to the seam owner, so the
        // fold stays silent instead of contradicting against the wrong body.
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(crate::discounted_total(5_000, 5_000), 5_000);",
        )?;
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn owner_that_calls_another_function_is_not_evaluable() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        discount_factor(amount_cents)
    } else {
        amount_cents
    }
}
fn discount_factor(amount_cents: u64) -> u64 { amount_cents * 70 / 100 }
"#;
        let verdict =
            verdict_for_owner(owner, "assert_eq!(discounted_total(5_000, 5_000), 5_000);")?;
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn method_owner_is_not_evaluable() -> Result<(), String> {
        let owner = r#"
pub struct Pricing { pub threshold: u64 }
impl Pricing {
    pub fn discounted_total(&self, amount_cents: u64) -> u64 {
        if amount_cents >= self.threshold {
            amount_cents * 70 / 100
        } else {
            amount_cents
        }
    }
}
"#;
        let verdict =
            verdict_for_owner(owner, "assert_eq!(discounted_total(5_000, 5_000), 5_000);")?;
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn transformed_owner_result_is_not_evaluable() -> Result<(), String> {
        // A method or arithmetic on the call's result asserts a value the
        // owner fold never computed; the fold must stay silent rather than
        // contradict a passing assertion.
        for assertion in [
            "assert_eq!(discounted_total(5_000, 5_000).max(3_500), 3_500);",
            "assert_eq!(discounted_total(5_000, 5_000) - 500, 3_000);",
        ] {
            assert_eq!(
                verdict_for_owner(BOUNDARY_OWNER, assertion)?,
                ExactValueVerdict::NotEvaluable,
                "{assertion}"
            );
        }
        Ok(())
    }

    #[test]
    fn method_call_operand_is_not_evaluable() -> Result<(), String> {
        let verdict = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_eq!(order.discounted_total(5_000, 5_000), 5_000);",
        );
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn wrong_assert_ne_is_contradicted_and_correct_ne_is_consistent() -> Result<(), String> {
        let wrong = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_ne!(discounted_total(5_000, 5_000), 3_500);",
        );
        assert_eq!(
            wrong?,
            ExactValueVerdict::Contradicted {
                expected: "3_500".to_string(),
                evaluated: 3_500,
            }
        );
        let correct = verdict_for_owner(
            BOUNDARY_OWNER,
            "assert_ne!(discounted_total(5_000, 5_000), 5_000);",
        );
        assert_eq!(correct?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn contradiction_summary_names_both_values() {
        let summary = contradiction_summary("5_000", 3_500);
        assert!(
            summary.contains("5_000") && summary.contains("3500"),
            "{summary}"
        );
        assert!(
            summary.contains("contradicts static evaluation"),
            "{summary}"
        );
    }

    #[test]
    fn else_if_chain_folds() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents * 70 / 100
    } else if amount_cents >= discount_threshold_cents / 2 {
        amount_cents * 90 / 100
    } else {
        amount_cents
    }
}
"#;
        // 3_000 < 5_000 takes the else-if arm (3_000 >= 5_000 / 2), which
        // folds to 3_000 * 90 / 100 = 2_700.
        let contradicted =
            verdict_for_owner(owner, "assert_eq!(discounted_total(3_000, 5_000), 3_000)?;");
        assert_eq!(
            contradicted?,
            ExactValueVerdict::Contradicted {
                expected: "3_000".to_string(),
                evaluated: 2_700,
            }
        );
        let consistent =
            verdict_for_owner(owner, "assert_eq!(discounted_total(3_000, 5_000), 2_700)?;");
        assert_eq!(consistent?, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn negative_and_suffixed_literals_fold() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: i64, discount_threshold_cents: i64) -> i64 {
    if amount_cents >= discount_threshold_cents {
        -amount_cents + 10i64
    } else {
        amount_cents
    }
}
"#;
        // -30 >= -100 takes the discounted arm: -(-30) + 10 = 40.
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(-30, -100), 40)?;");
        assert_eq!(verdict?, ExactValueVerdict::Consistent);
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(-30, -100), -40)?;");
        assert_eq!(
            verdict?,
            ExactValueVerdict::Contradicted {
                expected: "-40".to_string(),
                evaluated: 40,
            }
        );
        Ok(())
    }

    #[test]
    fn division_by_zero_is_not_evaluable() -> Result<(), String> {
        let owner = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents / discount_threshold_cents
    } else {
        amount_cents
    }
}
"#;
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(10, 0), 0)?;");
        assert_eq!(verdict?, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn debug_equality_macros_folds_like_plain_equality() -> Result<(), String> {
        let wrong = verdict_for_owner(
            BOUNDARY_OWNER,
            "debug_assert_eq!(discounted_total(5_000, 5_000), 5_000);",
        )?;
        assert!(matches!(wrong, ExactValueVerdict::Contradicted { .. }));
        let correct = verdict_for_owner(
            BOUNDARY_OWNER,
            "debug_assert_eq!(discounted_total(5_000, 5_000), 3_500);",
        )?;
        assert_eq!(correct, ExactValueVerdict::Consistent);
        let wrong_ne = verdict_for_owner(
            BOUNDARY_OWNER,
            "debug_assert_ne!(discounted_total(5_000, 5_000), 3_500);",
        )?;
        assert!(matches!(wrong_ne, ExactValueVerdict::Contradicted { .. }));
        Ok(())
    }

    #[test]
    fn declared_width_overflow_is_not_evaluable() -> Result<(), String> {
        // `fn next(x: u8) -> u8 { x + 1 }` at 255 wraps in release builds;
        // the mathematical fold (256) does not describe a value the owner
        // can return, so the pair stays not evaluable instead of
        // contradicting a passing wrapping assertion.
        let owner = "pub fn next(x: u8) -> u8 { x + 1 }";
        let adapter = RaRustSyntaxAdapter;
        let facts = adapter
            .summarize_file(
                std::path::Path::new("src/lib.rs"),
                &format!(
                    "{owner}
"
                ),
            )
            .map_err(|error| format!("owner source summarizes: {error}"))?;
        let owner_fn = facts
            .functions
            .iter()
            .find(|function| function.name == "next")
            .ok_or("owner function is indexed")?;
        let verdict =
            exact_value_assertion_verdict(Some(owner_fn), "assert_eq!(next(255), 0);", false);
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        // A value outside the declared width cannot be returned at all, so
        // an assertion pinning one stays not evaluable too.
        let verdict =
            exact_value_assertion_verdict(Some(owner_fn), "assert_eq!(next(255), 256);", false);
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        // Inside the declared width the same shape folds normally.
        let verdict =
            exact_value_assertion_verdict(Some(owner_fn), "assert_eq!(next(2), 3);", false);
        assert_eq!(verdict, ExactValueVerdict::Consistent);
        let verdict =
            exact_value_assertion_verdict(Some(owner_fn), "assert_eq!(next(2), 4);", false);
        assert!(matches!(verdict, ExactValueVerdict::Contradicted { .. }));
        Ok(())
    }

    #[test]
    fn untaken_branches_do_not_silence_the_fold() -> Result<(), String> {
        // `if x == 0 { 0 } else { 10 / x }` at 0 never reaches the
        // division; the fold evaluates only the selected branch, so the
        // baseline-failing assert is contradicted instead of keeping its
        // strong credit behind an undefined untaken arm (#6701 review).
        let owner = r#"
pub fn discounted_total(amount_cents: i64, discount_threshold_cents: i64) -> i64 {
    if amount_cents == 0 {
        0
    } else {
        10 / amount_cents
    }
}
"#;
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(0, 5), 1);")?;
        assert!(matches!(verdict, ExactValueVerdict::Contradicted { .. }));
        // The guard branch itself is honored: at 5 the division runs (2).
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(5, 5), 3);")?;
        assert!(matches!(verdict, ExactValueVerdict::Contradicted { .. }));
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(5, 5), 2);")?;
        assert_eq!(verdict, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn short_circuit_operands_skip_undefined_sides() -> Result<(), String> {
        // `x != 0 && 10 / x > 1` at 0 short-circuits before the division.
        let owner = r#"
pub fn discounted_total(amount_cents: i64, discount_threshold_cents: i64) -> i64 {
    if amount_cents != 0 && 10 / amount_cents > 1 {
        10 / amount_cents
    } else {
        0
    }
}
"#;
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(0, 5), 1);")?;
        assert!(matches!(verdict, ExactValueVerdict::Contradicted { .. }));
        let verdict = verdict_for_owner(owner, "assert_eq!(discounted_total(0, 5), 0);")?;
        assert_eq!(verdict, ExactValueVerdict::Consistent);
        Ok(())
    }

    #[test]
    fn local_shadow_of_the_owner_name_is_not_evaluable() -> Result<(), String> {
        // A `let` binding of the owner's name shadows the crate function;
        // the bare-name fold must stay silent instead of folding against a
        // body the assertion never called (#6701 review).
        let source = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents * 70 / 100
    } else {
        amount_cents
    }
}
#[cfg(test)]
mod tests {
    use super::discounted_total;
    #[test]
    fn boundary_uses_a_shadow() {
        let discounted_total = |a: u64, b: u64| a;
        assert_eq!(discounted_total(5_000, 5_000), 5_000);
    }
}
"#;
        let adapter = RaRustSyntaxAdapter;
        let facts = adapter
            .summarize_file(std::path::Path::new("src/lib.rs"), source)
            .map_err(|error| format!("source summarizes: {error}"))?;
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "discounted_total")
            .ok_or("owner function is indexed")?;
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "boundary_uses_a_shadow")
            .ok_or("shadowing test is indexed")?;
        let index = RustIndex::default();
        let assertion = test
            .assertions
            .iter()
            .find(|oracle| oracle.text.contains("assert_eq!"))
            .ok_or("the shadow test carries an equality assertion")?;
        assert!(bare_owner_call_is_shadowed(
            test,
            owner,
            &assertion.text,
            &index
        ));
        let verdict = exact_value_assertion_verdict(Some(owner), &assertion.text, true);
        assert_eq!(verdict, ExactValueVerdict::NotEvaluable);
        Ok(())
    }

    #[test]
    fn a_same_name_item_in_the_body_shadows_the_bare_name_fold() -> Result<(), String> {
        // A nested `fn` with the owner's name and an explicit same-name
        // import are other items (#10162): the bare-name fold must stay
        // silent instead of folding a rival's call against the owner's body.
        // A glob import binds nothing the fold names, so it stays readable.
        let source = r#"
pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
    if amount_cents >= discount_threshold_cents {
        amount_cents * 70 / 100
    } else {
        amount_cents
    }
}
pub mod other {
    pub fn discounted_total(amount_cents: u64, discount_threshold_cents: u64) -> u64 {
        amount_cents
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_fn_shadow() {
        {
            fn discounted_total(a: u64, b: u64) -> u64 {
                b
            }
            assert_eq!(discounted_total(5_000, 5_000), 5_000);
        }
    }

    #[test]
    fn rival_import_shadow() {
        use super::other::discounted_total;
        assert_eq!(discounted_total(5_000, 5_000), 5_000);
    }

    #[test]
    fn glob_import_keeps_the_owner_readable() {
        assert_eq!(discounted_total(5_000, 5_000), 5_000);
    }
}
"#;
        let adapter = RaRustSyntaxAdapter;
        let facts = adapter
            .summarize_file(std::path::Path::new("src/lib.rs"), source)
            .map_err(|error| format!("source summarizes: {error}"))?;
        let index = RustIndex::default();
        let owner = facts
            .functions
            .iter()
            .find(|function| {
                function.name == "discounted_total"
                    && function.file == std::path::Path::new("src/lib.rs")
            })
            .ok_or("owner function is indexed")?
            .clone();
        let shadowed = |name: &str, oracle: &str| {
            let test = facts
                .tests
                .iter()
                .find(|test| test.name == name)
                .ok_or_else(|| format!("{name} is indexed"))?;
            Ok::<bool, String>(bare_owner_call_is_shadowed(test, &owner, oracle, &index))
        };
        let oracle = "assert_eq!(discounted_total(5_000, 5_000), 5_000);";
        assert!(
            shadowed("nested_fn_shadow", oracle)?,
            "nested fn must shadow"
        );
        assert!(
            !shadowed("glob_import_keeps_the_owner_readable", oracle)?,
            "a glob import must not shadow"
        );
        // A rival import's other-definition reading runs through the shared
        // call-identity resolver; that pairing is pinned end-to-end by
        // `a_nested_scope_shadow_does_not_hide_an_outer_owner_value` in the
        // indirect_owner_limit public-API controls.
        Ok(())
    }

    #[test]
    fn non_equality_assertions_are_not_evaluable() -> Result<(), String> {
        for assertion in [
            "assert!(discounted_total(5_000, 5_000) > 0);",
            "assert!(matches!(discounted_total(5_000, 5_000), 3_500));",
            "assert_eq!(result, Ok(()));",
        ] {
            assert_eq!(
                verdict_for_owner(BOUNDARY_OWNER, assertion)?,
                ExactValueVerdict::NotEvaluable,
                "{assertion}"
            );
        }
        Ok(())
    }

    /// Each `#[case(..)]` row of an rstest table is folded on its own: one
    /// wrong row fails at baseline, so it contradicts the whole test.
    #[test]
    fn an_rstest_case_row_is_folded_with_its_own_literals() -> Result<(), String> {
        let verdict = |rows: &str| -> Result<ExactValueVerdict, String> {
            let source = format!(
                "pub fn weight(x: u32) -> u32 {{\n    x * 3\n}}\n#[cfg(test)]\nmod tests {{\n    use super::weight;\n    use rstest::rstest;\n    #[rstest]\n{rows}\n    fn weighs(#[case] x: u32, #[case] expected: u32) {{\n        assert_eq!(weight(x), expected);\n    }}\n}}\n"
            );
            let facts = RaRustSyntaxAdapter
                .summarize_file(std::path::Path::new("src/lib.rs"), &source)
                .map_err(|error| format!("source summarizes: {error}"))?;
            let owner = facts
                .functions
                .iter()
                .find(|function| function.name == "weight")
                .ok_or("owner function is indexed")?;
            let test = facts
                .tests
                .iter()
                .find(|test| test.name == "weighs")
                .ok_or("rstest test is indexed")?;
            let assertion = test
                .assertions
                .first()
                .ok_or("the rstest test carries its assertion")?;
            Ok(exact_value_assertion_verdict_for_test(
                Some(owner),
                &assertion.text,
                test,
                &RustIndex::default(),
            ))
        };
        assert_eq!(
            verdict("    #[case(4, 12)]\n    #[case::five(5, 15)]")?,
            ExactValueVerdict::Consistent
        );
        assert_eq!(
            verdict("    #[case(4, 12)]\n    #[case::five(5, 14)]")?,
            ExactValueVerdict::Contradicted {
                expected: "14".to_string(),
                evaluated: 15,
            }
        );
        // A row value the fold cannot read leaves the table not evaluable.
        assert_eq!(
            verdict("    #[case(4, 12)]\n    #[case(5, LIMIT)]")?,
            ExactValueVerdict::NotEvaluable
        );
        Ok(())
    }

    fn fold_text(body: &str, arguments: &[i128]) -> Option<i128> {
        fold_function_text(body, arguments)
    }

    #[test]
    fn integer_methods_fold_on_integer_receivers() {
        let pow = "fn f(a: u32, b: u64) -> u64 { b * 3u64.pow(a) }";
        assert_eq!(fold_text(pow, &[0, 100]), Some(100));
        assert_eq!(fold_text(pow, &[2, 10]), Some(90));
        let min = "fn f(a: u32) -> u32 { a.min(5) }";
        assert_eq!(fold_text(min, &[3]), Some(3));
        assert_eq!(fold_text(min, &[9]), Some(5));
        let max = "fn f(a: u32) -> u32 { a.max(5).min(8) }";
        assert_eq!(fold_text(max, &[1]), Some(5));
        assert_eq!(fold_text(max, &[9]), Some(8));
    }

    #[test]
    fn unsupported_methods_floats_and_overflow_stay_not_evaluable() {
        // Only pow/min/max fold; any other method, a float literal and a
        // `pow` that overflows the declared width fail the whole fold.
        assert_eq!(fold_text("fn f(a: u32) -> u32 { a.abs() }", &[1]), None);
        assert_eq!(fold_text("fn f(a: u32) -> u32 { a.pow(-1) }", &[1]), None);
        assert_eq!(fold_text("fn f(a: u32) -> u32 { a * 0.85 }", &[1]), None);
        assert_eq!(fold_text("fn f(a: u32) -> u8 { 3u8.pow(a) }", &[9]), None);
    }

    #[test]
    fn bitwise_shift_clamp_and_saturating_forms_fold_with_rust_precedence() {
        let fold = |expression: &str, argument: i128| {
            fold_text(
                &format!("fn f(a: u32) -> u32 {{ {expression} }}"),
                &[argument],
            )
        };
        // `&` binds tighter than `^`, which binds tighter than `|`.
        assert_eq!(fold("a | 4 ^ 6 & 3", 1), Some(1 | (4 ^ (6 & 3))));
        // Shifts bind tighter than `&` and looser than `+`.
        assert_eq!(fold("a + 1 << 2 & 0xF0", 7), Some(((7 + 1) << 2) & 0xF0));
        assert_eq!(fold("(a * 3) >> 4", 40), Some(7));
        assert_eq!(fold("a & 0b101", 7), Some(5));
        // Comparison is looser than the bitwise operators.
        assert_eq!(
            fold_text("fn f(a: u32) -> bool { a & 1 == 1 }", &[3]),
            None,
            "a bool-returning body is not an integer fold"
        );
        assert_eq!(fold("a.clamp(2, 5)", 9), Some(5));
        assert_eq!(fold("a.clamp(2, 5)", 0), Some(2));
        assert_eq!(fold("a.saturating_sub(10)", 4), Some(0));
        assert_eq!(fold("a.saturating_add(10)", 4), Some(14));
    }

    #[test]
    fn unsound_bitwise_shift_clamp_and_saturating_forms_stay_not_evaluable() {
        // A shift amount past the width, a left shift that overflows the
        // declared width, inverted clamp bounds, a saturating form with no
        // declared return width and a stray `|` (a closure bar) fail the fold.
        assert_eq!(fold_text("fn f(a: u32) -> u32 { a >> 200 }", &[1]), None);
        assert_eq!(fold_text("fn f(a: u32) -> u32 { a >> 32 }", &[1]), None);
        assert_eq!(
            fold_text("fn f(a: u32) -> u32 { a >> 31 }", &[1 << 31]),
            Some(1)
        );
        // The operand's width is unknown unless parameters and return agree.
        assert_eq!(fold_text("fn f(a: u8) -> u32 { a >> 1 }", &[4]), None);
        assert_eq!(fold_text("fn f(a: usize) -> usize { a << 63 }", &[2]), None);
        assert_eq!(
            fold_text(
                "fn f(a: u8) -> i32 { if a.saturating_sub(5) == 0 { 1 } else { 0 } }",
                &[3]
            ),
            None
        );
        assert_eq!(fold_text("fn f(a: u8) -> u8 { a << 7 }", &[3]), None);
        assert_eq!(
            fold_text("fn f(a: u32) -> u32 { a.clamp(5, 2) }", &[1]),
            None
        );
        assert_eq!(
            fold_text("fn f(a: u32) -> bool { a.saturating_sub(1) > 0 }", &[3]),
            None
        );
        assert_eq!(
            fold_text("fn f(a: u32) -> u32 { a.min(|b| b) }", &[3]),
            None
        );
    }

    fn invisible_reason(
        owner: &str,
        before: &str,
        after: &str,
        test: &str,
    ) -> Result<Option<InvisibilityReason>, String> {
        let adapter = RaRustSyntaxAdapter;
        let source = format!("{owner}\n\n{test}\n");
        let facts = adapter
            .summarize_file(std::path::Path::new("src/lib.rs"), &source)
            .map_err(|error| format!("source summarizes: {error}"))?;
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "owner")
            .ok_or("owner function is indexed")?;
        let tests = facts.tests.iter().collect::<Vec<_>>();
        Ok(change_is_invisible_to_every_assertion(
            owner, before, after, &tests,
        ))
    }

    fn invisible(owner: &str, before: &str, after: &str, test: &str) -> Result<bool, String> {
        Ok(invisible_reason(owner, before, after, test)?.is_some())
    }

    const CAP: &str = "pub fn owner(n: u32) -> u32 {\n    n.min(6)\n}";

    #[test]
    fn an_assertion_below_the_cap_cannot_see_the_raised_cap() -> Result<(), String> {
        let test = "#[test]\nfn low() {\n    assert_eq!(owner(3), 3);\n}";
        assert!(invisible(CAP, "n.min(5)", "n.min(6)", test)?);
        Ok(())
    }

    const POWER: &str =
        "pub fn owner(attempt: u32, base: u64) -> u64 {\n    base * 3u64.pow(attempt)\n}";

    #[test]
    fn a_relational_sibling_that_holds_on_both_versions_stays_invisible() -> Result<(), String> {
        // `owner(3,100) > owner(1,100)` is 2700 > 300 changed and 800 > 200 before.
        let test = "#[test]\nfn a() {\n    assert_eq!(owner(0, 100), 100);\n}\n#[test]\nfn grows() {\n    assert!(owner(3, 100) > owner(1, 100));\n}";
        assert!(invisible(
            POWER,
            "base * 2u64.pow(attempt)",
            "base * 3u64.pow(attempt)",
            test
        )?);
        // The relational member names the reason: the asserted inputs may
        // evaluate differently, so the caller owes the truth-value summary,
        // not "no input reaches a difference" (#10916).
        assert_eq!(
            invisible_reason(
                POWER,
                "base * 2u64.pow(attempt)",
                "base * 3u64.pow(attempt)",
                test
            )?,
            Some(InvisibilityReason::RelationalTruthUnchanged)
        );
        // The equality-only shape keeps the value-equality reason.
        let equality_only = "#[test]\nfn a() {\n    assert_eq!(owner(0, 100), 100);\n}";
        assert_eq!(
            invisible_reason(
                POWER,
                "base * 2u64.pow(attempt)",
                "base * 3u64.pow(attempt)",
                equality_only
            )?,
            Some(InvisibilityReason::SameValueAtEveryAssertedInput)
        );
        Ok(())
    }

    #[test]
    fn a_relational_assertion_that_flips_between_versions_is_visible() -> Result<(), String> {
        // 2700 > 1000 changed, 800 > 1000 before: this test can tell them apart.
        let test = "#[test]\nfn a() {\n    assert_eq!(owner(0, 100), 100);\n}\n#[test]\nfn big() {\n    assert!(owner(3, 100) > 1000);\n}";
        assert!(!invisible(
            POWER,
            "base * 2u64.pow(attempt)",
            "base * 3u64.pow(attempt)",
            test
        )?);
        Ok(())
    }

    #[test]
    fn a_relational_assertion_with_an_unfoldable_side_keeps_the_credit() -> Result<(), String> {
        let test =
            "#[test]\nfn a() {\n    let n = 3;\n    assert!(owner(n, 100) > owner(1, 100));\n}";
        assert!(!invisible(
            POWER,
            "base * 2u64.pow(attempt)",
            "base * 3u64.pow(attempt)",
            test
        )?);
        let test = "#[test]\nfn a() {\n    assert!(owner(3, 100) > owner(1, 100) && true);\n}";
        assert!(!invisible(
            POWER,
            "base * 2u64.pow(attempt)",
            "base * 3u64.pow(attempt)",
            test
        )?);
        Ok(())
    }

    #[test]
    fn an_equivalent_rewrite_is_not_reported_as_invisible() -> Result<(), String> {
        let owner = "pub fn owner(n: u32) -> u32 {\n    3 * n\n}";
        let test = "#[test]\nfn a() {\n    assert_eq!(owner(4), 12);\n}";
        assert!(!invisible(owner, "n * 3", "3 * n", test)?);
        Ok(())
    }

    #[test]
    fn one_separating_call_or_an_unreadable_call_is_not_invisible() -> Result<(), String> {
        let separating = "#[test]\nfn a() {\n    assert_eq!(owner(3), 3);\n}\n#[test]\nfn b() {\n    assert_eq!(owner(9), 6);\n}";
        assert!(!invisible(CAP, "n.min(5)", "n.min(6)", separating)?);
        let unreadable = "#[test]\nfn a() {\n    assert_eq!(owner(compute()), 3);\n}";
        assert!(!invisible(CAP, "n.min(5)", "n.min(6)", unreadable)?);
        // A call outside an equality assertion is not folded either.
        let outside = "#[test]\nfn a() {\n    let v = owner(3);\n    assert!(v > 0);\n}";
        assert!(!invisible(CAP, "n.min(5)", "n.min(6)", outside)?);
        // A second owner call in the expected operand, or beside the
        // assertion, can separate the versions at an input the fold never saw.
        let expected_side = "#[test]\nfn a() {\n    assert_eq!(owner(3), owner(9) - 3);\n}";
        assert!(!invisible(CAP, "n.min(5)", "n.min(6)", expected_side)?);
        let beside =
            "#[test]\nfn a() {\n    let wide = owner(9);\n    assert_eq!(owner(3), wide);\n}";
        assert!(!invisible(CAP, "n.min(5)", "n.min(6)", beside)?);
        // The changed text must occur exactly once in the owner.
        let low = "#[test]\nfn a() {\n    assert_eq!(owner(3), 3);\n}";
        assert!(!invisible(CAP, "n.min(5)", "n.max(6)", low)?);
        Ok(())
    }
}
