use super::super::rust_index::{FunctionSummary, TestSummary};
use super::text::{
    delimited_contents_at, enum_variant_values, exact_error_variant, path_value_is_constant,
};
use crate::domain::*;

#[cfg(test)]
pub(in crate::analysis) fn activation_evidence(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
) -> ActivationEvidence {
    activation_evidence_with_value_facts(
        probe,
        owner_fn,
        related_tests,
        flow_sinks,
        helper_chain,
        index,
        workspace_complete,
        None,
    )
}

/// `activation_evidence`, reading each related test's owner-independent
/// value facts through `value_facts` when the classifier supplies its
/// run-scoped memo.
#[cfg(test)]
#[allow(
    clippy::too_many_arguments,
    reason = "activation_evidence's inputs plus the optional run-scoped memo"
)]
pub(in crate::analysis) fn activation_evidence_with_value_facts(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
    value_facts: Option<&TestValueFacts>,
) -> ActivationEvidence {
    activation_and_boundary_input(
        probe,
        owner_fn,
        related_tests,
        flow_sinks,
        helper_chain,
        index,
        workspace_complete,
        value_facts,
    )
    .activation
}

/// Activation evidence plus, for a changed boundary whose inputs ripr
/// cannot read, why (#6674, #6693, #6672). The infection stage reads the
/// reason to stay unknown instead of reporting a missing boundary input.
pub(in crate::analysis) struct ActivationWithBoundaryInput {
    pub(in crate::analysis) activation: ActivationEvidence,
    pub(in crate::analysis) unresolved_boundary: Option<String>,
}

/// `activation_evidence_with_value_facts` that also returns the
/// unresolved-boundary reason computed on the same pass.
#[allow(
    clippy::too_many_arguments,
    reason = "activation_evidence's inputs plus the optional run-scoped memo"
)]
pub(in crate::analysis) fn activation_and_boundary_input(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
    value_facts: Option<&TestValueFacts>,
) -> ActivationWithBoundaryInput {
    let mut observed_values = related_tests
        .iter()
        .flat_map(|test| match value_facts {
            Some(memo) => memo.facts_for(index, test, owner_fn),
            None => value_facts_for_test(test, owner_fn),
        })
        .collect::<Vec<_>>();
    observed_values.extend(observed_discriminator_values(
        probe,
        owner_fn,
        related_tests,
        helper_chain,
        index,
        workspace_complete,
    ));
    sort_value_facts(&mut observed_values);

    let mut unresolved_boundary = None;
    let mut missing_discriminators = missing_discriminator_facts(
        probe,
        owner_fn,
        related_tests,
        flow_sinks,
        &observed_values,
        helper_chain,
        index,
        workspace_complete,
        &mut unresolved_boundary,
    );
    missing_discriminators.sort_by(|left, right| {
        left.value
            .cmp(&right.value)
            .then(left.reason.cmp(&right.reason))
            .then(
                left.flow_sink
                    .as_ref()
                    .map(|sink| sink.kind.as_str())
                    .cmp(&right.flow_sink.as_ref().map(|sink| sink.kind.as_str())),
            )
    });
    missing_discriminators
        .dedup_by(|left, right| left.value == right.value && left.reason == right.reason);

    ActivationWithBoundaryInput {
        activation: ActivationEvidence {
            observed_values,
            missing_discriminators,
        },
        unresolved_boundary,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParameterValue {
    parameter: String,
    value: String,
    line: usize,
    text: String,
}

/// `value_facts_for_test` per (related test, owner), computed at most once
/// for as long as the memo lives.
///
/// The facts depend only on the test and the owner, never on the probe, and
/// every probe in an owner relates to largely the same tests, so a
/// classification run shares one memo across its probes (it rides the
/// run-scoped `RelatedTestCandidateIndex`). Entries are keyed by the test's
/// and owner's slots in the index the memo was first queried with; a test
/// or owner that is not an element of that index, or a query against
/// another index, is computed fresh and never cached, so a key can only
/// ever name the same fact values.
/// A related test's slot in `RustIndex::tests` and its owner's slot in
/// `RustIndex::functions` (`None` for an ownerless probe).
type TestOwnerSlot = (usize, Option<usize>);

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct TestValueFacts {
    index_identity: std::cell::Cell<Option<(usize, usize, u64)>>,
    by_slot: std::cell::RefCell<std::collections::BTreeMap<TestOwnerSlot, Vec<ValueFact>>>,
}

impl TestValueFacts {
    /// `value_facts_for_test(test, owner_fn)`.
    pub(in crate::analysis) fn facts_for(
        &self,
        index: &crate::analysis::rust_index::RustIndex,
        test: &TestSummary,
        owner_fn: Option<&FunctionSummary>,
    ) -> Vec<ValueFact> {
        let Some(key) = self.slot_key(index, test, owner_fn) else {
            return value_facts_for_test(test, owner_fn);
        };
        if let Some(facts) = self.by_slot.borrow().get(&key) {
            return facts.clone();
        }
        let facts = value_facts_for_test(test, owner_fn);
        self.by_slot.borrow_mut().insert(key, facts.clone());
        facts
    }

    fn slot_key(
        &self,
        index: &crate::analysis::rust_index::RustIndex,
        test: &TestSummary,
        owner_fn: Option<&FunctionSummary>,
    ) -> Option<TestOwnerSlot> {
        let identity = index.storage_identity();
        match self.index_identity.get() {
            None => self.index_identity.set(Some(identity)),
            Some(bound) if bound != identity => return None,
            Some(_) => {}
        }
        let test_slot = index.test_slot(test)?;
        let owner_slot = match owner_fn {
            Some(owner) => Some(index.function_slot(owner)?),
            None => None,
        };
        Some((test_slot, owner_slot))
    }
}

/// The value context of a qualified path token found in a test (#5357):
/// `Constant` when the path's spelling establishes a constant, otherwise the
/// historical `EnumVariant`.
fn path_value_context(path: &str) -> ValueContext {
    if path_value_is_constant(path) {
        ValueContext::Constant
    } else {
        ValueContext::EnumVariant
    }
}

fn value_facts_for_test(test: &TestSummary, owner_fn: Option<&FunctionSummary>) -> Vec<ValueFact> {
    let owner_name = owner_fn.map(|owner| owner.name.as_str()).unwrap_or("");
    let parameters = owner_fn.map(function_parameters).unwrap_or_default();
    let mut facts = Vec::new();

    for call in test.body_calls() {
        if !owner_name.is_empty() && call.name != owner_name {
            continue;
        }
        let Some(arguments) = call_arguments(&call.text, &call.name) else {
            continue;
        };
        for (idx, argument) in arguments.iter().enumerate() {
            for value in owner_argument_values(test, argument) {
                let value = parameters
                    .get(idx)
                    .map(|parameter| format!("{parameter} = {value}"))
                    .unwrap_or(value);
                facts.push(ValueFact {
                    line: call.line,
                    text: call.text.clone(),
                    value,
                    context: ValueContext::FunctionArgument,
                });
            }
            for value in enum_variant_values(argument) {
                facts.push(ValueFact {
                    line: call.line,
                    text: call.text.clone(),
                    context: path_value_context(&value),
                    value,
                });
            }
        }
    }

    for assertion in &test.assertions {
        let assertion_arguments = macro_arguments(&assertion.text).unwrap_or_default();
        for argument in assertion_arguments {
            if argument.contains(owner_name) && !owner_name.is_empty() {
                continue;
            }
            for value in scalar_values(&argument) {
                facts.push(ValueFact {
                    line: assertion.line,
                    text: assertion.text.clone(),
                    value,
                    context: ValueContext::AssertionArgument,
                });
            }
        }
        for value in enum_variant_values(&assertion.text) {
            facts.push(ValueFact {
                line: assertion.line,
                text: assertion.text.clone(),
                context: path_value_context(&value),
                value,
            });
        }
    }

    for (offset, line) in test.body.lines().enumerate() {
        let line_number = test.start_line + offset;
        let trimmed = line.trim();
        if looks_like_table_row(trimmed) {
            for value in spelled_scalar_values(trimmed) {
                facts.push(ValueFact {
                    line: line_number,
                    text: trimmed.to_string(),
                    value,
                    context: ValueContext::TableRow,
                });
            }
        }
        if looks_like_builder_method(trimmed) {
            for value in spelled_scalar_values(trimmed) {
                facts.push(ValueFact {
                    line: line_number,
                    text: trimmed.to_string(),
                    value,
                    context: ValueContext::BuilderMethod,
                });
            }
        }
    }

    sort_value_facts(&mut facts);
    facts
}

fn observed_discriminator_values(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
) -> Vec<ValueFact> {
    let Some(owner) = owner_fn else {
        return Vec::new();
    };
    let parameters = function_parameters(owner);
    let Some((left, right)) =
        oriented_comparison_operands(owner, &parameters, &probe.expression, probe.location.line)
    else {
        return Vec::new();
    };
    let call_values = call_values_for_owner(owner, &parameters, related_tests, helper_chain);
    let left_parameter = boundary_operand_parameter(owner, &parameters, &left);
    let right_parameter = boundary_operand_parameter(owner, &parameters, &right);
    // #3295: the operands resolve once per probe (initializer or
    // parameter); only the input row changes per call.
    let left_resolved = resolve_boundary_operand(
        owner,
        &left,
        probe.location.line,
        &parameters,
        index,
        workspace_complete,
    );
    let right_resolved = resolve_boundary_operand(
        owner,
        &right,
        probe.location.line,
        &parameters,
        index,
        workspace_complete,
    );
    let right_constant = (right_resolved.is_none() && right_parameter.is_none())
        .then(|| boundary_constant(owner, &right, index))
        .flatten();
    let mut facts = Vec::new();

    for row in call_values {
        let inputs: super::value_transfer::ExactInputs = row
            .iter()
            .map(|cell| (cell.parameter.clone(), cell.value.clone()))
            .collect();
        let left_exact = left_resolved.as_ref().and_then(|resolved| {
            exact_operand_for_row(resolved, &row, &inputs, index, workspace_complete)
        });
        let right_exact = right_resolved
            .as_ref()
            .and_then(|resolved| {
                exact_operand_for_row(resolved, &row, &inputs, index, workspace_complete)
            })
            .or_else(|| visible_operand(&right, right_constant.as_ref()));
        if let (Some(left_value), Some(right_value)) = (&left_exact, &right_exact)
            && left_value.value == right_value.value
        {
            facts.push(ValueFact {
                line: row.first().map(|cell| cell.line).unwrap_or_default(),
                text: format!(
                    "{} | {}; {}",
                    row.first()
                        .map(|cell| cell.text.clone())
                        .unwrap_or_default(),
                    left_value.provenance,
                    right_value.provenance,
                ),
                value: format!("{left} == {right}"),
                context: ValueContext::FunctionArgument,
            });
            continue;
        }
        let Some(left_parameter) = left_parameter.as_deref() else {
            continue;
        };
        let Some(left_value) = parameter_value(&row, left_parameter) else {
            continue;
        };
        let right_value = right_parameter
            .as_deref()
            .and_then(|parameter| parameter_value(&row, parameter))
            .map(|value| value.value)
            .or_else(|| {
                visible_operand(&right, right_constant.as_ref()).map(|operand| operand.value)
            });
        if right_value
            .as_deref()
            .is_some_and(|value| comparable_value(value) == comparable_value(&left_value.value))
        {
            facts.push(ValueFact {
                line: left_value.line,
                text: left_value.text.clone(),
                value: format!("{left} == {right}"),
                context: ValueContext::FunctionArgument,
            });
        }
    }
    if let (Some(constant), Some(left_parameter)) = (&right_constant, left_parameter.as_deref()) {
        for (line, text) in owner_calls_passing_constant(
            related_tests,
            owner,
            &parameters,
            left_parameter,
            constant,
            index,
        ) {
            facts.push(ValueFact {
                line,
                text: format!("{text} | argument names constant {right}"),
                value: format!("{left} == {right}"),
                context: ValueContext::FunctionArgument,
            });
        }
    }

    facts
}

/// The exact value of one comparison operand under one related-test
/// call row (#3295). A parameter resolves to the row's literal; a
/// local binding resolves through the #3294 binding relation (the
/// predicate must be a direct use in the binding's live span) and the
/// bounded value-transfer evaluator. `None` keeps the operand unknown.
/// One comparison operand resolved once per probe (#3295 review): a
/// shadowing local's initializer (evaluated per row) or the raw
/// parameter.
pub(crate) enum ResolvedOperand {
    Parameter(String),
    Local(String, String),
    /// A direct call to a unique helper (`is_word_start(input, 0)`)
    /// whose return value is evaluated over the row's bound inputs
    /// (#3296 boolean-predicate helper family).
    Call {
        callee: Box<crate::analysis::facts::FunctionSummary>,
        arguments: Vec<String>,
    },
}

fn resolve_boundary_operand(
    owner: &FunctionSummary,
    operand: &str,
    predicate_line: usize,
    parameters: &[String],
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
) -> Option<ResolvedOperand> {
    if let Some(initializer) = live_local_initializer(owner, operand, predicate_line) {
        // #3296 scanner/helper remainder: a local whose initializer is
        // itself a direct call to a unique helper (`let final_state =
        // scan_state(input)`) jumps to the helper authority — the value
        // evaluator fails closed on call expressions, so without the
        // jump the operand stays unknown whatever the callee's body.
        if let Some(call) = resolve_direct_call(&initializer, index, workspace_complete) {
            return Some(call);
        }
        return Some(ResolvedOperand::Local(operand.to_string(), initializer));
    }
    if let Some(parameter) = parameters
        .iter()
        .find(|parameter| parameter.as_str() == operand)
    {
        return Some(ResolvedOperand::Parameter(parameter.clone()));
    }
    resolve_direct_call(operand, index, workspace_complete)
}

/// Decompose a direct-call text (`is_word_start(input, 0)`) into the
/// unique callee and its statically splittable arguments. Method or
/// path-qualified call sites, non-unique callee names, and unsplittable
/// argument lists stay unresolved (fail closed).
pub(crate) fn resolve_direct_call(
    text: &str,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
) -> Option<ResolvedOperand> {
    let trimmed = text.trim().trim_end_matches(';').trim();
    let callee_name = trimmed
        .split('(')
        .next()
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })?
        .to_string();
    if !trimmed.contains('(') || !trimmed.ends_with(')') {
        return None;
    }
    if !workspace_complete || !super::helper_transfer::callee_is_unique(&callee_name, index) {
        return None;
    }
    let callee = index
        .functions()
        .iter()
        .find(|function| function.name == callee_name)?;
    let arguments = super::helper_transfer::split_call_arguments_text(trimmed, &callee_name)?;
    Some(ResolvedOperand::Call {
        callee: Box::new(callee.clone()),
        arguments,
    })
}

/// Evaluate one resolved operand against one related-test call row.
fn exact_operand_for_row(
    resolved: &ResolvedOperand,
    row: &[ParameterValue],
    inputs: &super::value_transfer::ExactInputs,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
) -> Option<ExactOperand> {
    match resolved {
        ResolvedOperand::Local(name, initializer) => {
            match super::value_transfer::evaluate_initializer(initializer, inputs) {
                super::value_transfer::EvalOutcome::Exact { value, provenance } => Some(
                    exact_operand_from_evaluation(name, &value, &provenance, inputs),
                ),
                _ => None,
            }
        }
        ResolvedOperand::Parameter(parameter) => {
            let cell = row
                .iter()
                .find(|cell| cell.parameter == *parameter)
                .cloned()?;
            Some(ExactOperand {
                value: cell.value.clone(),
                provenance: format!("exact input {parameter} = {}", cell.value),
            })
        }
        ResolvedOperand::Call { callee, arguments } => {
            // Bind the call-site arguments against the owner's row
            // (literal or the owner's parameter), then evaluate the
            // helper's return over the bound inputs. A computed
            // argument stops the operand (no guessed value).
            let owner_parameters = function_parameters_of_row(row);
            let mut bound = super::value_transfer::ExactInputs::new();
            for (index, argument) in arguments.iter().enumerate() {
                let parameter = function_parameters(callee)
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("arg{index}"));
                let value = if let Some(literal) = super::helper_transfer::strict_literal(argument)
                {
                    literal
                } else if let Some(cell) = owner_parameters
                    .iter()
                    .position(|parameter| parameter.as_str() == argument.trim())
                    .and_then(|position| row.get(position))
                {
                    cell.value.clone()
                } else {
                    return None;
                };
                bound.insert(parameter, value);
            }
            let eval = super::helper_transfer::HelperEval::root(index, workspace_complete);
            let value = super::helper_transfer::helper_return_value(callee, &bound, &eval)?;
            Some(ExactOperand {
                value: value.render(),
                provenance: format!(
                    "{} = {} via helper return of `{}` over bound inputs (1 hop)",
                    callee.name,
                    value.render(),
                    callee.name
                ),
            })
        }
    }
}

/// The parameter names implied by a row's cells, in binding order.
fn function_parameters_of_row(row: &[ParameterValue]) -> Vec<String> {
    row.iter().map(|cell| cell.parameter.clone()).collect()
}

/// Build the exact operand with its provenance chain: operation
/// families, source inputs, and chain depth (#3295 evidence contract).
fn exact_operand_from_evaluation(
    operand: &str,
    value: &super::value_transfer::TypedValue,
    provenance: &[super::value_transfer::EvalStep],
    inputs: &super::value_transfer::ExactInputs,
) -> ExactOperand {
    let chain = provenance
        .iter()
        .map(|step| step.operation.as_str())
        .collect::<Vec<_>>()
        .join(" -> ");
    let input_literals = inputs
        .iter()
        .map(|(parameter, literal)| format!("{parameter} = {literal}"))
        .collect::<Vec<_>>()
        .join(", ");
    if provenance.is_empty() {
        return ExactOperand {
            value: value.render(),
            provenance: format!("{operand} = {} (exact literal)", value.render()),
        };
    }
    ExactOperand {
        value: value.render(),
        provenance: format!(
            "{operand} = {} via {chain} over {input_literals} (chain depth {})",
            value.render(),
            provenance.len()
        ),
    }
}

/// One exact comparison operand: the rendered value plus the #3295
/// provenance chain retained on the fact text (source inputs,
/// operation families, chain depth).
struct ExactOperand {
    value: String,
    provenance: String,
}

/// What the owner says about a boundary operand that may be a local
/// binding (#4228).
pub(in crate::analysis) enum LocalBoundary {
    /// No `let` in the owner declares the operand.
    NotLocal,
    /// A live local whose initializer needs no test input
    /// (`let limit = 100;`), folded by the same binding relation and
    /// bounded evaluator `check` uses per row.
    Exact(String),
    /// A local whose value depends on test inputs or that the evaluator
    /// cannot fold. No input-free value exists to match a test against.
    Unresolved,
}

pub(in crate::analysis) fn local_boundary(
    owner: &FunctionSummary,
    operand: &str,
    predicate_line: usize,
) -> LocalBoundary {
    if let Some(initializer) = live_local_initializer(owner, operand, predicate_line)
        && let super::value_transfer::EvalOutcome::Exact { value, .. } =
            super::value_transfer::evaluate_initializer(
                &initializer,
                &super::value_transfer::ExactInputs::new(),
            )
    {
        return LocalBoundary::Exact(value.render());
    }
    if owner_declares_local(owner, operand, predicate_line) {
        LocalBoundary::Unresolved
    } else {
        LocalBoundary::NotLocal
    }
}

/// Whether the owner's body declares `operand` with `let` (or `let mut`)
/// on or before the predicate line. A declaration after the predicate
/// cannot be the compared binding (a same-named constant still is), so it
/// does not count. Comments and strings are masked first; an operand that
/// is not an identifier (a literal such as `100`) returns early.
fn owner_declares_local(owner: &FunctionSummary, operand: &str, predicate_line: usize) -> bool {
    let is_identifier = operand
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && operand
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !is_identifier {
        return false;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&owner.body);
    masked
        .lines()
        .enumerate()
        .take_while(|(offset, _)| owner.start_line + offset <= predicate_line)
        .flat_map(|(_, line)| line.split([';', '{', '}']))
        .any(|statement| {
            let statement = statement.trim();
            let Some(rest) = statement.strip_prefix("let ") else {
                return false;
            };
            let rest = rest.trim_start();
            let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
            rest.strip_prefix(operand).is_some_and(|after| {
                !after
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            })
        })
}

/// The initializer of a local binding whose live span (per the #3294
/// binding relation) covers the predicate line: the predicate must be
/// one of the binding's direct uses, so the initializer provably feeds
/// the compared operand. At most one declaration generation can hold
/// the predicate in its live span; the first that does wins.
fn live_local_initializer(
    owner: &FunctionSummary,
    operand: &str,
    predicate_line: usize,
) -> Option<String> {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&owner.body);
    // Detection runs on the masked line; the initializer is taken from
    // the raw line so string literals survive for evaluation (#3295).
    for (offset, (raw_line, masked_line)) in owner.body.lines().zip(masked.lines()).enumerate() {
        let absolute = owner.start_line + offset;
        let trimmed = masked_line.trim();
        // A trailing comment (`let x = …; // note`) survives on the
        // raw line: cut the raw statement at the masked line's first
        // semicolon, which string masking keeps honest.
        // Masking preserves byte offsets, so the masked semicolon's
        // byte index cuts the raw line directly.
        let raw_statement = masked_line
            .find(';')
            .map(|cut| &raw_line[..=cut])
            .unwrap_or(raw_line);
        if trimmed.starts_with("let ")
            && trimmed.contains(';')
            && let Some((_declared, _)) = crate::analysis::language::changed_let_binding(trimmed)
            && let Some((declared, initializer)) =
                crate::analysis::language::changed_let_binding(raw_statement.trim())
            && declared == operand
        {
            let initializer = initializer.to_string();
            let resolution = crate::analysis::probes::resolve_changed_binding_uses(
                operand,
                &initializer,
                &owner.body,
                owner.start_line,
                absolute,
            );
            if let crate::analysis::probes::BindingPredicateResolution::DirectUses(uses) =
                &resolution
                && uses
                    .iter()
                    .any(|use_site| use_site.predicate_line == predicate_line)
            {
                return Some(initializer);
            }
        }
    }
    None
}

#[allow(
    clippy::too_many_arguments,
    reason = "the shared evidence inputs flow through one authority"
)]
fn missing_discriminator_facts(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
    observed_values: &[ValueFact],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
    unresolved_boundary: &mut Option<String>,
) -> Vec<MissingDiscriminatorFact> {
    let mut missing = Vec::new();
    if matches!(probe.family, ProbeFamily::Predicate)
        && let Some(fact) = missing_boundary_discriminator(
            probe,
            owner_fn,
            related_tests,
            flow_sinks,
            helper_chain,
            index,
            workspace_complete,
            unresolved_boundary,
        )
    {
        missing.push(fact);
    }
    if (matches!(probe.family, ProbeFamily::ErrorPath)
        || flow_sinks
            .iter()
            .any(|sink| sink.kind == FlowSinkKind::ErrorVariant))
        && let Some(fact) = missing_error_variant_discriminator(probe, related_tests, flow_sinks)
    {
        missing.push(fact);
    }
    if matches!(probe.family, ProbeFamily::FieldConstruction)
        && let Some(fact) = missing_field_value_discriminator(
            probe,
            owner_fn.map(|owner| owner.name.as_str()),
            related_tests,
            flow_sinks,
        )
    {
        missing.push(fact);
    }
    if matches!(probe.family, ProbeFamily::MatchArm)
        && let Some(fact) = missing_match_arm_discriminator(
            probe,
            owner_fn,
            related_tests,
            flow_sinks,
            index,
            workspace_complete,
        )
    {
        missing.push(fact);
    }
    if missing.is_empty()
        && observed_values
            .iter()
            .any(|fact| fact.value.contains(" == "))
    {
        return Vec::new();
    }
    missing
}

#[allow(
    clippy::too_many_arguments,
    reason = "the shared evidence inputs plus the unresolved-boundary reason computed on the same pass"
)]
fn missing_boundary_discriminator(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
    unresolved: &mut Option<String>,
) -> Option<MissingDiscriminatorFact> {
    let owner = owner_fn?;
    let parameters = function_parameters(owner);
    let (left, right) =
        oriented_comparison_operands(owner, &parameters, &probe.expression, probe.location.line)?;
    let call_values = call_values_for_owner(owner, &parameters, related_tests, helper_chain);
    if call_values.is_empty() {
        // Every related call passes only computed arguments (#6672): the
        // inputs exist but none is readable, so the boundary is
        // unresolved rather than silently unmeasured.
        // Only a compared parameter is in scope; an unrelated computed
        // argument (an output buffer `&mut [0; 4]`) says nothing here.
        let compared = [
            boundary_operand_parameter(owner, &parameters, &left),
            boundary_operand_parameter(owner, &parameters, &right),
        ];
        *unresolved = computed_input_parameters(owner, &parameters, related_tests, helper_chain)
            .into_iter()
            .find(|parameter| compared.iter().flatten().any(|name| name == parameter))
            .map(|parameter| computed_argument_reason(&parameter))
            .or_else(|| {
                // The owner is called, but only with arguments ripr cannot
                // read (`score(&[b'f'; 16])`): a compared operand that is
                // neither a parameter, a literal, nor a constant (a local
                // counter `count`) has no row to evaluate it under, so it is
                // unreadable rather than missing (Devin review on #6796).
                let unknown_operands = [&left, &right]
                    .into_iter()
                    .filter(|operand| {
                        boundary_operand_parameter(owner, &parameters, operand).is_none()
                            && literal_operand_value(operand).is_none()
                            && crate::analysis::value_resolution::constant_operand_name(operand)
                                .is_none()
                            && crate::analysis::value_resolution::constant_offset_operand(operand)
                                .is_none()
                    })
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                (!unknown_operands.is_empty()
                    && owner_is_called(owner, related_tests, helper_chain))
                .then(|| {
                    unresolved_boundary_input(
                        owner,
                        &parameters,
                        related_tests,
                        helper_chain,
                        &unknown_operands,
                        [None, None],
                    )
                })
                .flatten()
            })
            .or_else(|| unreadable_spelled_line_reason(owner, related_tests, helper_chain));
        return None;
    }
    let left_parameter = boundary_operand_parameter(owner, &parameters, &left);
    let right_parameter = boundary_operand_parameter(owner, &parameters, &right);

    // #3295 exact path: when either operand is a computed local, the
    // bounded evaluator resolves its value from the row's exact
    // inputs; the boundary is observed when both sides compare equal
    // under any row.
    let left_resolved = resolve_boundary_operand(
        owner,
        &left,
        probe.location.line,
        &parameters,
        index,
        workspace_complete,
    );
    let right_resolved = resolve_boundary_operand(
        owner,
        &right,
        probe.location.line,
        &parameters,
        index,
        workspace_complete,
    );
    let right_constant = (right_resolved.is_none() && right_parameter.is_none())
        .then(|| boundary_constant(owner, &right, index))
        .flatten();
    let exact_rows: Vec<(Vec<ExactOperand>, Vec<ExactOperand>)> = call_values
        .iter()
        .map(|row| {
            let inputs: super::value_transfer::ExactInputs = row
                .iter()
                .map(|cell| (cell.parameter.clone(), cell.value.clone()))
                .collect();
            let lefts = left_resolved
                .as_ref()
                .and_then(|resolved| {
                    exact_operand_for_row(resolved, row, &inputs, index, workspace_complete)
                })
                .into_iter()
                .collect::<Vec<_>>();
            let rights = right_resolved
                .as_ref()
                .and_then(|resolved| {
                    exact_operand_for_row(resolved, row, &inputs, index, workspace_complete)
                })
                .or_else(|| visible_operand(&right, right_constant.as_ref()))
                .into_iter()
                .collect::<Vec<_>>();
            (lefts, rights)
        })
        .collect();
    // Exact operands compare by their canonical renderings directly:
    // the digit-oriented literal normalization (underscore/quote
    // stripping) would equate distinct strings (#3295 review).
    let exact_equality_observed = exact_rows.iter().any(|(lefts, rights)| {
        lefts.iter().any(|left_value| {
            rights
                .iter()
                .any(|right_value| right_value.value == left_value.value)
        })
    });

    let equality_observed = exact_equality_observed
        || left_parameter.as_deref().is_some_and(|left_parameter| {
            call_values.iter().any(|row| {
                let Some(left_value) = parameter_value(row, left_parameter) else {
                    return false;
                };
                let right_value = right_parameter
                    .as_deref()
                    .and_then(|parameter| parameter_value(row, parameter))
                    .map(|value| value.value)
                    .or_else(|| {
                        visible_operand(&right, right_constant.as_ref())
                            .map(|operand| operand.value)
                    });
                right_value.as_deref().is_some_and(|value| {
                    comparable_value(value) == comparable_value(&left_value.value)
                })
            })
        });
    let constant_named = right_constant
        .as_ref()
        .zip(left_parameter.as_deref())
        .is_some_and(|(constant, left_parameter)| {
            !owner_calls_passing_constant(
                related_tests,
                owner,
                &parameters,
                left_parameter,
                constant,
                index,
            )
            .is_empty()
        });
    if equality_observed || constant_named {
        return None;
    }
    // A deep table or builder line that may feed the owner hides its
    // inputs; only after the exact calls above found no boundary input.
    if let Some(reason) = unreadable_spelled_line_reason(owner, related_tests, helper_chain) {
        *unresolved = Some(reason);
        return None;
    }
    // #6674/#6693: an input ripr cannot read is not a missing input. When
    // the compared operand maps to no related-test input (a local
    // accumulator such as `count`, a computed value such as `s.len()` or
    // `name.split_whitespace().count()` that the bounded evaluator cannot
    // fold), or a related test feeds the compared parameter a computed
    // argument (#6672), no row says which side of the boundary a test
    // sits on, so naming a missing equality value would ask for a test
    // that may already exist. The boundary stays unresolved instead.
    let operand_evaluates = |resolved: Option<&ResolvedOperand>| {
        resolved.is_some_and(|resolved| {
            call_values.iter().any(|row| {
                let inputs: super::value_transfer::ExactInputs = row
                    .iter()
                    .map(|cell| (cell.parameter.clone(), cell.value.clone()))
                    .collect();
                exact_operand_for_row(resolved, row, &inputs, index, workspace_complete).is_some()
            })
        })
    };
    // A constant-shaped operand keeps its own named-constant route.
    let unknown = |operand: &str, parameter: Option<&str>, resolved: Option<&ResolvedOperand>| {
        parameter.is_none()
            && !operand_evaluates(resolved)
            && literal_operand_value(operand).is_none()
            && crate::analysis::value_resolution::constant_operand_name(operand).is_none()
            && crate::analysis::value_resolution::constant_offset_operand(operand).is_none()
    };
    // When neither side has a value (`out.len() != data.len() / 2`) no
    // row says which side of the boundary a test reaches either, so both
    // operands are named and the boundary stays unresolved.
    let unknown_operands = [
        (&left, left_parameter.as_deref(), left_resolved.as_ref()),
        (&right, right_parameter.as_deref(), right_resolved.as_ref()),
    ]
    .into_iter()
    .filter(|(operand, parameter, resolved)| unknown(operand, *parameter, *resolved))
    .map(|(operand, _, _)| operand.as_str())
    .collect::<Vec<_>>();
    if let Some(reason) = unresolved_boundary_input(
        owner,
        &parameters,
        related_tests,
        helper_chain,
        &unknown_operands,
        [left_parameter.as_deref(), right_parameter.as_deref()],
    ) {
        *unresolved = Some(reason);
        return None;
    }
    // An offset constant whose value ripr cannot see (`LIMIT - 2` with a
    // computed, suffixed, or non-decimal `LIMIT`) has no boundary value a
    // test can match, and a test naming `LIMIT` is not the offset boundary
    // by identity, so no repair would be recognized (#6671).
    if let Some(constant) = right_constant
        .as_ref()
        .filter(|constant| constant.offset != 0 && constant.lookup.value().is_none())
    {
        *unresolved = Some(format!(
            "boundary operand `{right}` offsets constant `{}`, whose value ripr cannot see statically ({}), so it cannot tell which side of the boundary a test reaches",
            constant.name,
            constant.lookup.limitation()
        ));
        return None;
    }
    // A local boundary that no row evaluates (a single-line `let`, or an
    // initializer the bounded evaluator cannot fold) has no value a test
    // can match, so naming it would ask for a repair ripr can never
    // confirm (#4228). The grip path routes the same local to its
    // unresolved-operand limitation.
    if right_parameter.is_none()
        && exact_rows.iter().all(|(_, rights)| rights.is_empty())
        && matches!(
            local_boundary(owner, &right, probe.location.line),
            LocalBoundary::Unresolved
        )
    {
        return None;
    }
    // A constant ripr cannot pin to one declaration in the owner's file
    // (imported, or declared twice) can never be matched by a test, so
    // naming it as the missing discriminator would ask for a repair ripr
    // cannot confirm. The stage stays unknown instead ("no literal
    // boundary was visible"), mirroring the repo-seam unresolved route.
    if right_constant
        .as_ref()
        .is_some_and(|constant| !constant.lookup.is_declared_once())
    {
        return None;
    }

    let mut left_values = left_parameter
        .as_deref()
        .map(|parameter| observed_parameter_values(&call_values, parameter))
        .unwrap_or_default();
    // Exact evaluated lefts join the observed listing so the reason
    // names real values instead of `unknown` (#3295).
    let exact_lefts: Vec<String> = exact_rows
        .iter()
        .flat_map(|(lefts, _)| lefts.iter().map(|operand| operand.value.clone()))
        .filter(|value| !left_values.contains(value))
        .collect();
    left_values.extend(exact_lefts);
    left_values.sort();
    left_values.dedup();
    let right_parameter_values = right_parameter
        .as_deref()
        .and_then(|parameter| parameter_value_set(&call_values, parameter));
    let right_literal =
        visible_operand(&right, right_constant.as_ref()).map(|operand| operand.value);
    let reason = if let Some(right_values) = right_parameter_values {
        format!(
            "No related test call uses {left} equal to {right}; observed {left} values: {}; observed {right} values: {}",
            list_or_unknown(&left_values),
            list_or_unknown(&right_values)
        )
    } else if let Some(right_value) = right_literal {
        format!(
            "No related test call uses {left} equal to {right}; observed {left} values: {}; target {right} value: {right_value}",
            list_or_unknown(&left_values)
        )
    } else if let Some(constant) = right_constant.as_ref() {
        let symbolic = if constant.lookup.is_declared_once() {
            format!("; a test that passes {right} itself is recognized")
        } else {
            String::new()
        };
        format!(
            "No related test call uses {left} equal to {right}; observed {left} values: {}; ripr cannot see the value of constant {right} statically ({}), so a test that already calls with that value is not recognized{symbolic}",
            list_or_unknown(&left_values),
            constant.lookup.limitation()
        )
    } else {
        format!(
            "No related test call uses {left} equal to {right}; observed {left} values: {}",
            list_or_unknown(&left_values)
        )
    };

    Some(MissingDiscriminatorFact {
        value: format!("{left} == {right}"),
        reason,
        flow_sink: first_visible_flow_sink(flow_sinks).cloned(),
    })
}

/// Why a changed boundary's inputs cannot be read, when they cannot
/// (#6674, #6693, #6672): an operand that is neither a parameter, a
/// value the bounded evaluator folds from a row, a literal, nor a named
/// constant; or a compared parameter some related test feeds a computed
/// argument. `None` when every operand is readable.
fn unresolved_boundary_input(
    owner: &FunctionSummary,
    parameters: &[String],
    related_tests: &[&TestSummary],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
    unknown_operands: &[&str],
    operand_parameters: [Option<&str>; 2],
) -> Option<String> {
    if !unknown_operands.is_empty() {
        // A predicate the operand splitter cannot cut cleanly (a `let`
        // statement, a closure with its own comparison) is not named.
        if !unknown_operands
            .iter()
            .all(|operand| is_simple_operand(operand))
        {
            return Some(
                "the changed comparison's operands are not ones ripr can map to the related tests' inputs, so it cannot tell which side of the boundary a test reaches"
                    .to_string(),
            );
        }
        return Some(match unknown_operands {
            [unmapped] => format!(
                "boundary operand `{unmapped}` is a local or computed value ripr cannot map to the related tests' inputs, so it cannot tell which side of the boundary a test reaches"
            ),
            _ => format!(
                "boundary operands {} are local or computed values ripr cannot map to the related tests' inputs, so it cannot tell which side of the boundary a test reaches",
                unknown_operands
                    .iter()
                    .map(|operand| format!("`{operand}`"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
        });
    }
    let computed = computed_input_parameters(owner, parameters, related_tests, helper_chain);
    let parameter = operand_parameters
        .into_iter()
        .flatten()
        .find(|parameter| computed.iter().any(|name| name == parameter))?;
    Some(computed_argument_reason(parameter))
}

/// Whether a related test calls the owner, or the resolved helper chain's
/// entry function.
fn owner_is_called(
    owner: &FunctionSummary,
    related_tests: &[&TestSummary],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
) -> bool {
    let entry_name = helper_chain
        .and_then(|chain| chain.hops.last())
        .map(|hop| hop.caller.name.as_str());
    related_tests.iter().any(|test| {
        test.body_calls()
            .any(|call| call.name == owner.name || Some(call.name.as_str()) == entry_name)
    })
}

/// An operand the comparison splitter cut cleanly: no statement keyword,
/// assignment, closure, or block, and balanced parentheses.
fn is_simple_operand(operand: &str) -> bool {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(operand);
    let opens = masked.matches('(').count();
    let closes = masked.matches(')').count();
    !masked.starts_with("let ")
        && !masked.contains(" if ")
        && !masked.starts_with("if ")
        && !masked.contains(['=', '|', '{', '}'])
        && opens == closes
}

fn computed_argument_reason(parameter: &str) -> String {
    format!(
        "a related test passes a computed argument for `{parameter}`, which is not an exact input value, so ripr cannot tell which side of the boundary that call reaches"
    )
}

fn missing_error_variant_discriminator(
    probe: &Probe,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
) -> Option<MissingDiscriminatorFact> {
    let variant = exact_error_variant(&probe.expression).or_else(|| {
        flow_sinks
            .iter()
            .find_map(|sink| exact_error_variant(&sink.text))
    })?;
    let exact_assertion_found = related_tests.iter().any(|test| {
        test.assertions.iter().any(|assertion| {
            assertion.kind == OracleKind::ExactErrorVariant && assertion.text.contains(&variant)
        })
    });
    if exact_assertion_found {
        return None;
    }

    Some(MissingDiscriminatorFact {
        value: variant.clone(),
        reason: format!("No exact error variant assertion for {variant}"),
        flow_sink: flow_sinks
            .iter()
            .find(|sink| sink.kind == FlowSinkKind::ErrorVariant)
            .or_else(|| first_visible_flow_sink(flow_sinks))
            .cloned(),
    })
}

/// Opens the reason of a match-arm missing discriminator; infection reads
/// it to keep an unselected arm from counting as activated.
pub(in crate::analysis) use crate::domain::ARM_UNSELECTED_REASON_PREFIX;

/// RIPR-SPEC-0229 (#5432): name a changed match arm as the missing
/// discriminator when every related test calls the owner directly and every
/// call's scrutinee input provably selects a different arm (`reason(Some(5))`
/// against `None => 0`). One unreadable use of the owner, one variable or
/// computed input, one wildcard or refutable alternative, or one related
/// test that never names the owner leaves the arm unnamed: such a test may
/// select the arm in a way this reading cannot see.
fn missing_match_arm_discriminator(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
    index: &crate::analysis::rust_index::RustIndex,
    workspace_complete: bool,
) -> Option<MissingDiscriminatorFact> {
    let owner = owner_fn?;
    // A same-named function elsewhere makes a direct call ambiguous, and a
    // partial index cannot show that the name is unique.
    if related_tests.is_empty()
        || !workspace_complete
        || !super::helper_transfer::callee_is_unique(&owner.name, index)
    {
        return None;
    }
    let selector = super::arm_selection::ArmSelector::establish(probe, owner)?
        .in_workspace(index, related_tests.iter().map(|test| test.file.as_path()));
    // Built once per probe: the helper walk below looks names up for every
    // related test.
    let mut functions_by_name = std::collections::BTreeMap::<&str, Vec<_>>::new();
    for function in index.functions() {
        functions_by_name
            .entry(function.name.as_str())
            .or_default()
            .push(function);
    }
    let mut inputs = Vec::new();
    for test in related_tests {
        if may_reach_owner_unread(test, &owner.name, &functions_by_name) {
            return None;
        }
        let observed = selector.observed_inputs(test)?;
        if observed.selection != super::arm_selection::ArmSelection::SelectsOther {
            return None;
        }
        inputs.extend(observed.inputs);
    }
    inputs.sort();
    inputs.dedup();
    let pattern = selector.pattern_text();
    Some(MissingDiscriminatorFact {
        value: pattern.to_string(),
        reason: format!(
            "{ARM_UNSELECTED_REASON_PREFIX} `{pattern} =>`; observed `{}` values: {}",
            selector.scrutinee(),
            inputs
                .iter()
                .map(|input| format!("`{input}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        flow_sink: flow_sinks
            .iter()
            .find(|sink| sink.kind == FlowSinkKind::MatchArm)
            .or_else(|| first_visible_flow_sink(flow_sinks))
            .cloned(),
    })
}

/// Whether the test may run the owner through something its own body does
/// not show: a call to an indexed function whose body names the owner (a
/// helper such as `check_none()`), or a macro other than the standard
/// assertion and formatting macros, whose expansion is not read.
fn may_reach_owner_unread(
    test: &TestSummary,
    owner: &str,
    functions_by_name: &std::collections::BTreeMap<&str, Vec<&FunctionSummary>>,
) -> bool {
    const READ_MACROS: &[&str] = &[
        "assert",
        "assert_eq",
        "assert_ne",
        "debug_assert",
        "debug_assert_eq",
        "debug_assert_ne",
        "vec",
        "format",
        "print",
        "println",
        "eprint",
        "eprintln",
        "dbg",
        "panic",
        "matches",
    ];
    // Follow helpers transitively: `check_none()` may call `inner(None)`,
    // which calls the owner. The test's own `fn name()` header reads as a
    // call to itself; only a different indexed function is a helper.
    let mut pending = test
        .body_calls()
        .map(|call| call.name.as_str())
        .filter(|name| *name != owner)
        .collect::<Vec<_>>();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name) {
            continue;
        }
        for function in functions_by_name
            .get(name)
            .into_iter()
            .flatten()
            .filter(|function| !(function.name == test.name && function.file == test.file))
        {
            if super::reveal::contains_as_whole_word(&function.body, owner) {
                return true;
            }
            pending.extend(
                function
                    .calls
                    .iter()
                    .map(|call| call.name.as_str())
                    .filter(|callee| *callee != owner),
            );
        }
    }
    let masked = crate::analysis::extract::mask_comments_and_strings(&test.body);
    let bytes = masked.as_bytes();
    // Rust allows whitespace on both sides of the `!` (`exercise! ()`,
    // `exercise !()`), so both are skipped before reading the delimiter
    // and the name.
    masked.match_indices('!').any(|(offset, _)| {
        let next = bytes[offset + 1..]
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace());
        if !matches!(next, Some(b'(' | b'[' | b'{')) {
            return false;
        }
        let name_end = masked[..offset].trim_end().len();
        let name_start = masked[..name_end]
            .char_indices()
            .rev()
            .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_'))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let name = &masked[name_start..name_end];
        // `if !(done)` is a negation after a keyword, not a macro.
        !name.is_empty()
            && !READ_MACROS.contains(&name)
            && !matches!(
                name,
                "if" | "while" | "match" | "return" | "in" | "else" | "break"
            )
    })
}

/// Produce a missing-discriminator fact for a `FieldConstruction` seam whose
/// `RequiredDiscriminator::FieldValue { field }` has no matching producer-owned
/// discriminator in the test evidence.
///
/// Mirrors the Predicate (`missing_boundary_discriminator`) and ErrorPath
/// (`missing_error_variant_discriminator`) arms. The probe expression is the
/// field value the readiness authority (`repair_route.rs`) compares via
/// `exact_key("field_value", field)`. When a related test already asserts the
/// field via an ExactValue / WholeObjectEquality / RelationalCheck / Snapshot
/// oracle, the discriminator is NOT missing and this returns `None`.
fn missing_field_value_discriminator(
    probe: &Probe,
    owner_name: Option<&str>,
    related_tests: &[&TestSummary],
    flow_sinks: &[FlowSinkFact],
) -> Option<MissingDiscriminatorFact> {
    // Only meaningful when there is a StructField flow sink.
    let has_struct_field_sink = flow_sinks
        .iter()
        .any(|sink| sink.kind == FlowSinkKind::StructField);
    if !has_struct_field_sink {
        return None;
    }

    // If any related test observes this field via an accepted oracle kind
    // (ExactValue, WholeObjectEquality, RelationalCheck, Snapshot), the
    // discriminator is already covered — do not emit a missing fact.
    //
    // The match uses word-boundary semantics via `contains_as_whole_word` to
    // avoid token coincidence (e.g. `id` matching inside `provider`), the
    // recurring false-observation family. See reveal.rs:413 for the same guard.
    // A read of the field by name observes it as well as the whole
    // initializer text does, but only on the owner's result (`cfg.retries`
    // after `let cfg = default_config()`, or `default_config().retries`):
    // `fallback.retries` on an unrelated value would clear the fact and let
    // the shared field token promote the initializer (#4428 review).
    let field_read = constructed_field_name(&probe.expression).map(|name| format!(".{name}"));
    let field_already_observed = related_tests.iter().any(|test| {
        test.assertions.iter().any(|assertion| {
            matches!(
                assertion.kind,
                OracleKind::ExactValue
                    | OracleKind::WholeObjectEquality
                    | OracleKind::RelationalCheck
                    | OracleKind::Snapshot
            ) && (super::reveal::contains_as_whole_word(&assertion.text, &probe.expression)
                || field_read
                    .as_deref()
                    .zip(owner_name)
                    .is_some_and(|(read, owner)| {
                        reads_owner_result_field(&test.body, &assertion.text, read, owner)
                    }))
        })
    });
    if field_already_observed {
        return None;
    }

    Some(MissingDiscriminatorFact {
        value: probe.expression.clone(),
        reason: format!(
            "No field-value assertion observes the constructed field: {}",
            probe.expression
        ),
        flow_sink: flow_sinks
            .iter()
            .find(|sink| sink.kind == FlowSinkKind::StructField)
            .cloned(),
    })
}

/// The field a struct-literal initializer line constructs: `retries: 1,` and
/// the shorthand `retries,` both name `retries`. `None` for anything that is
/// not a plain `ident: value` or `ident` initializer.
fn constructed_field_name(expression: &str) -> Option<&str> {
    let trimmed = expression.trim().trim_end_matches(',').trim();
    let name = match trimmed.find(':') {
        Some(colon) if trimmed[colon..].starts_with("::") => return None,
        Some(colon) => trimmed[..colon].trim(),
        None => trimmed,
    };
    let mut chars = name.chars();
    let starts_ident = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    (starts_ident && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')).then_some(name)
}

/// Whether `assertion` reads `read` (`.field`) on a value the test got from
/// calling `owner`: a direct `owner(..).field` chain, or a receiver the test
/// body binds with `let [mut] recv = ..owner(..)..;`.
fn reads_owner_result_field(body: &str, assertion: &str, read: &str, owner: &str) -> bool {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let owner_call = format!("{owner}(");
    assertion.match_indices(read).any(|(start, matched)| {
        if assertion[start + matched.len()..]
            .chars()
            .next()
            .is_some_and(is_ident)
        {
            return false;
        }
        let before = &assertion[..start];
        if before.ends_with(')') {
            return call_before_is_owner(before, owner);
        }
        let receiver_start = before
            .rfind(|ch: char| !is_ident(ch))
            .map_or(0, |index| index + 1);
        let receiver = &before[receiver_start..];
        !receiver.is_empty()
            && binds_from_owner_call(body, receiver, &owner_call)
            && !field_overwritten_before(body, assertion, receiver, &read[1..], &owner_call)
    })
}

/// Whether the owner's value of `field` no longer sits in `receiver` where
/// `assertion` reads it (RIPR-SPEC-0005: field credit holds only before
/// shadow or field overwrite). Comments and strings are masked, and only a
/// `let` or assignment in the assertion's own block or an enclosing one
/// counts. Walking back from the assertion, the nearest visible
/// `let receiver` that sets `field` in a struct literal to a value not read
/// from the receiver (`let q = Quote { total: 99, ..q };`) overwrites it; a
/// struct update that leaves `field` to its `..receiver` base passes it
/// through; and a visible `receiver.field = value` after the owner binding
/// overwrites it. Anything else, including an assertion that is not found
/// exactly once in the body, keeps the credit.
fn field_overwritten_before(
    body: &str,
    assertion: &str,
    receiver: &str,
    field: &str,
    owner_call: &str,
) -> bool {
    let mut found = body.match_indices(assertion).map(|(index, _)| index);
    let (Some(position), None) = (found.next(), found.next()) else {
        return false;
    };
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let Some(prefix) = masked.get(..position) else {
        return false;
    };
    let tainted = receiver_derived_bindings(prefix, receiver, owner_call);
    let fresh = |value: &str| {
        !mentions_ident(value, receiver)
            && !tainted.iter().any(|name| mentions_ident(value, name))
            && !value.contains(owner_call)
    };
    let visible = |start: usize| visible_at(prefix, start);
    for (start, initializer) in receiver_lets(prefix, receiver).into_iter().rev() {
        if !visible(start) {
            continue;
        }
        if let Some((fields, base)) = struct_literal_fields(initializer) {
            match fields.iter().find(|(name, _)| *name == field) {
                Some((_, value)) => return value.is_some_and(fresh),
                None if base == Some(receiver) => continue,
                None => return false,
            }
        }
        if initializer.contains(owner_call) {
            return field_assigned(prefix, start, receiver, field, &fresh);
        }
        return false;
    }
    false
}

/// Names bound in `body` from the receiver or another owner call, directly
/// or through another such name (`let t = q.total; let u = t;`,
/// `t = q.total;`, `let base = bundle(3);`): a value built from them may be
/// the owner's own field value.
fn receiver_derived_bindings(body: &str, receiver: &str, owner_call: &str) -> Vec<String> {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    // `let pattern = init;` and plain `name = value;` statements.
    let bindings: Vec<(&str, &str)> = body
        .split([';', '{', '}'])
        .filter_map(|statement| {
            let statement = statement.trim();
            let statement = statement.strip_prefix("let ").unwrap_or(statement);
            let (pattern, initializer) = statement.split_once('=')?;
            if initializer.starts_with('=')
                || pattern.ends_with(['!', '<', '>', '+', '-', '*', '/', '|', '&', '^', '%'])
            {
                return None;
            }
            Some((pattern, initializer))
        })
        .collect();
    let mut tainted = vec![receiver.to_string()];
    loop {
        let before = tainted.len();
        for (pattern, initializer) in &bindings {
            if initializer.contains(owner_call)
                || tainted.iter().any(|name| mentions_ident(initializer, name))
            {
                for name in pattern.split(|ch: char| !is_ident(ch)) {
                    if !name.is_empty()
                        && name != "mut"
                        && name.starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
                        && !tainted.iter().any(|known| known == name)
                    {
                        tainted.push(name.to_string());
                    }
                }
            }
        }
        if tainted.len() == before {
            break;
        }
    }
    tainted.remove(0);
    tainted
}

/// Whether a statement starting at `start` is still in scope at the end of
/// `prefix`: no block it sits in has closed since.
fn visible_at(prefix: &str, start: usize) -> bool {
    let mut depth = 0isize;
    for ch in prefix[start..].chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn mentions_ident(text: &str, name: &str) -> bool {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(name).any(|(start, matched)| {
        !text[..start].chars().next_back().is_some_and(is_ident)
            && !text[start + matched.len()..].starts_with(is_ident)
    })
}

/// The `let [mut] receiver [: Type] = initializer;` statements in `body`, in
/// order, as (statement start, initializer).
fn receiver_lets<'a>(body: &'a str, receiver: &str) -> Vec<(usize, &'a str)> {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    body.match_indices("let ")
        .filter(|(start, _)| !body[..*start].chars().next_back().is_some_and(is_ident))
        .filter_map(|(start, _)| {
            let statement = body[start + 4..].split(';').next().unwrap_or_default();
            let rest = statement.trim_start();
            let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
            let rest = rest.strip_prefix(receiver)?;
            if rest.starts_with(is_ident) {
                return None;
            }
            let (_, initializer) = rest.split_once('=')?;
            let initializer = initializer.trim();
            (!initializer.starts_with('=')).then_some((start, initializer))
        })
        .collect()
}

/// The fields (with their value, `None` for shorthand) and `..base` of a
/// struct literal `Path { a: x, b, ..base }`; `None` for any other
/// initializer.
type StructLiteral<'a> = (Vec<(&'a str, Option<&'a str>)>, Option<&'a str>);

fn struct_literal_fields(initializer: &str) -> Option<StructLiteral<'_>> {
    let open = initializer.find('{')?;
    let path = initializer[..open].trim();
    if path.is_empty()
        || !path.starts_with(|ch: char| ch.is_ascii_uppercase())
        || !path
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':')
    {
        return None;
    }
    let inner = initializer[open + 1..].strip_suffix('}')?;
    let mut fields = Vec::new();
    let mut base = None;
    let mut depth = 0usize;
    let mut entry_start = 0;
    for (index, ch) in inner.char_indices().chain([(inner.len(), ',')]) {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                let entry = inner[entry_start..index].trim();
                entry_start = index + 1;
                if let Some(rest) = entry.strip_prefix("..") {
                    base = Some(rest.trim());
                } else if !entry.is_empty() {
                    match entry.split_once(':') {
                        Some((name, value)) => fields.push((name.trim(), Some(value.trim()))),
                        None => fields.push((entry, None)),
                    }
                }
            }
            _ => {}
        }
    }
    Some((fields, base))
}

/// Whether `prefix`, after `from`, assigns `receiver.field = value` (not
/// `==`, not a value read from the receiver) in a statement still in scope
/// at its end.
fn field_assigned(
    prefix: &str,
    from: usize,
    receiver: &str,
    field: &str,
    fresh: &dyn Fn(&str) -> bool,
) -> bool {
    let target = format!("{receiver}.{field}");
    prefix[from..]
        .match_indices(&target)
        .any(|(offset, matched)| {
            let start = from + offset;
            let before_ok = !prefix[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
            let rest = &prefix[start + matched.len()..];
            if rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
                return false;
            }
            let rest = rest.trim_start();
            let Some(value) = rest
                .strip_prefix('=')
                .filter(|value| !value.starts_with('='))
            else {
                return false;
            };
            let value = value.split(';').next().unwrap_or_default();
            before_ok && fresh(value) && visible_at(prefix, start)
        })
}

/// Whether the call whose `)` ends `before` is a call of `owner`: the
/// matching `(` is preceded by `owner` as a whole identifier
/// (`default_config()`, `Config::default_config(..)`), not merely an owner
/// call somewhere else in the assertion (`other().retries` beside
/// `default_config()`).
fn call_before_is_owner(before: &str, owner: &str) -> bool {
    let mut depth = 0usize;
    let mut open = None;
    for (index, ch) in before.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    open = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(callee) = open.map(|index| &before[..index]) else {
        return false;
    };
    callee.strip_suffix(owner).is_some_and(|prefix| {
        !prefix
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

/// Whether `body` has `let [mut] receiver = ...;` whose initializer calls
/// the owner.
fn binds_from_owner_call(body: &str, receiver: &str, owner_call: &str) -> bool {
    body.match_indices("let ").any(|(start, _)| {
        if body[..start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return false;
        }
        let statement = body[start + 4..].split(';').next().unwrap_or_default();
        let rest = statement.trim_start();
        let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
        let Some(rest) = rest.strip_prefix(receiver) else {
            return false;
        };
        let rest = rest.trim_start();
        (rest.starts_with('=') || rest.starts_with(':')) && rest.contains(owner_call)
    })
}

fn owner_call_parameter_values(
    related_tests: &[&TestSummary],
    owner_name: &str,
    parameters: &[String],
) -> Vec<Vec<ParameterValue>> {
    let mut rows = Vec::new();
    if owner_name.is_empty() || parameters.is_empty() {
        return rows;
    }
    for test in related_tests {
        for call in test.body_calls() {
            if call.name != owner_name {
                continue;
            }
            let Some(arguments) = call_arguments(&call.text, &call.name) else {
                continue;
            };
            let row = arguments
                .iter()
                .enumerate()
                .filter_map(|(idx, argument)| {
                    let parameter = parameters.get(idx)?;
                    let value = owner_argument_values(test, argument).into_iter().next()?;
                    Some(ParameterValue {
                        parameter: parameter.clone(),
                        value,
                        line: call.line,
                        text: call.text.clone(),
                    })
                })
                .collect::<Vec<_>>();
            if !row.is_empty() {
                rows.push(row);
            }
        }
    }
    rows
}

/// #3296: the owner's exact input rows. Direct test-call rows first
/// (the pre-existing source); when the owner has none and the bounded
/// helper chain resolves, the entry function's rows bind down the
/// chain to the owner's parameters. A chain that stops, or a computed
/// argument at any hop, yields no rows — the stop edge stays with the
/// chain the classifier already recorded.
fn call_values_for_owner(
    owner: &FunctionSummary,
    parameters: &[String],
    related_tests: &[&TestSummary],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
) -> Vec<Vec<ParameterValue>> {
    let parameters = if parameters.is_empty() {
        function_parameters(owner)
    } else {
        parameters.to_vec()
    };
    let direct = owner_call_parameter_values(related_tests, &owner.name, &parameters);
    if !direct.is_empty() {
        return direct;
    }
    let Some(chain) = helper_chain else {
        return direct;
    };
    helper_transferred_rows(&parameters, chain, related_tests)
}

/// Whether `argument` computes one deterministic value ripr cannot read
/// (#6672): a computed expression (`base + 1`, `[b'f'; 16]`) whose free
/// variables are all bound to exact values (`is_exact`), constants
/// (`LIMIT`), or none at all. Such a call sits at one definite input that
/// may be the boundary, so the boundary is unresolved. A computation over
/// a value ripr cannot bind (a loop variable in `can_retire(age + 1)`) is
/// no more readable than that bare variable, which already yields no
/// input row without unresolving the boundary, so it is not counted.
fn deterministic_computed_argument(argument: &str, is_exact: impl Fn(&str) -> bool) -> bool {
    is_computed_value_expression(argument)
        && free_identifiers(argument)
            .iter()
            .all(|name| is_constant_like(name) || is_exact(name))
}

fn is_constant_like(name: &str) -> bool {
    matches!(name, "true" | "false")
        || (name.chars().any(|ch| ch.is_ascii_uppercase())
            && name
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
}

/// The variables an expression reads: identifiers that are not a call or
/// macro name, a method or field after `.`, a path segment beside `::`, a
/// type after `as`, a numeric literal or its suffix, or a char/byte
/// literal's contents. Strings and comments are masked first.
fn free_identifiers(text: &str) -> Vec<String> {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(text);
    let chars: Vec<char> = masked.chars().collect();
    let mut names = Vec::new();
    let mut previous: Option<char> = None;
    let mut after_as = false;
    let mut idx = 0usize;
    while idx < chars.len() {
        let ch = chars[idx];
        if ch == '\'' {
            let close = if chars.get(idx + 1) == Some(&'\\') {
                idx + 3
            } else {
                idx + 2
            };
            if chars.get(close) == Some(&'\'') {
                previous = Some('\'');
                idx = close + 1;
                continue;
            }
        }
        if ch.is_ascii_digit() {
            while idx < chars.len() && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '_') {
                idx += 1;
            }
            previous = Some('0');
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = idx;
            while idx < chars.len() && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '_') {
                idx += 1;
            }
            let name: String = chars[start..idx].iter().collect();
            let next = chars[idx..].iter().find(|ch| !ch.is_whitespace()).copied();
            let path_next = chars.get(idx) == Some(&':') && chars.get(idx + 1) == Some(&':');
            let byte_literal = name == "b" && chars.get(idx) == Some(&'\'');
            let skip = after_as
                || byte_literal
                || name == "as"
                || matches!(next, Some('(' | '!'))
                || matches!(previous, Some('.' | ':'))
                || path_next;
            after_as = name == "as";
            if !skip && !names.contains(&name) {
                names.push(name);
            }
            previous = Some('a');
            continue;
        }
        if !ch.is_whitespace() {
            previous = Some(ch);
        }
        idx += 1;
    }
    names
}

/// The owner parameters some related test feeds a computed argument
/// (`order_discount(base + 1)`), read from the same source rows
/// `call_values_for_owner` uses: direct owner calls when a test calls the
/// owner, otherwise the entry calls of the resolved helper chain carried
/// down the hops where a hop passes its caller's parameter straight
/// through, plus the target of any hop argument that is itself computed
/// (`owner(x + 1)`). Such an argument yields no input row (it is not an exact
/// value), yet the call may still sit on the boundary, so a boundary over
/// one of these parameters is unresolved rather than missing (#6672).
fn computed_input_parameters(
    owner: &FunctionSummary,
    parameters: &[String],
    related_tests: &[&TestSummary],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
) -> Vec<String> {
    let computed_for = |name: &str, names: &[String]| -> (bool, Vec<String>) {
        let mut called = false;
        let mut computed = Vec::new();
        for test in related_tests {
            for call in test.body_calls() {
                if call.name != name {
                    continue;
                }
                let Some(arguments) = call_arguments(&call.text, &call.name) else {
                    continue;
                };
                called = true;
                for (idx, argument) in arguments.iter().enumerate() {
                    if deterministic_computed_argument(argument, |name| {
                        !owner_argument_values(test, name).is_empty()
                    }) && let Some(parameter) = names.get(idx)
                        && !computed.contains(parameter)
                    {
                        computed.push(parameter.clone());
                    }
                }
            }
        }
        (called, computed)
    };
    let (called, direct) = computed_for(&owner.name, parameters);
    if called {
        return direct;
    }
    let Some(chain) = helper_chain else {
        return Vec::new();
    };
    let Some(entry) = chain.hops.last() else {
        return Vec::new();
    };
    let entry_parameters = function_parameters(&entry.caller);
    let (entry_called, mut computed) = computed_for(&entry.caller.name, &entry_parameters);
    // A hop argument computed from the caller's parameters is one exact
    // value per row only when the entry calls carry exact rows at all.
    if !entry_called
        || owner_call_parameter_values(related_tests, &entry.caller.name, &entry_parameters)
            .is_empty()
    {
        return Vec::new();
    }
    for step in (0..chain.hops.len()).rev() {
        let hop = &chain.hops[step];
        let target_parameters: Vec<String> = if step == 0 {
            parameters.to_vec()
        } else {
            function_parameters(&chain.hops[step - 1].caller)
        };
        // A hop argument is computed when it carries a computed caller
        // parameter straight through, or computes its value itself
        // (`owner(x + 1)`): `bind_helper_argument` stops the transfer at
        // either, so the target parameter gets no exact row.
        computed = hop
            .arguments
            .iter()
            .enumerate()
            .filter(|(_, argument)| {
                let caller_parameters = function_parameters(&hop.caller);
                deterministic_computed_argument(argument, |name| {
                    caller_parameters.iter().any(|parameter| parameter == name)
                }) || computed.iter().any(|name| name == argument.trim())
            })
            .filter_map(|(idx, _)| target_parameters.get(idx).cloned())
            .collect();
    }
    computed
}

/// Bind the entry function's direct test rows down the resolved chain
/// to the owner's parameters. Each hop's call-site arguments bind
/// positionally: a literal binds directly, the caller's own parameter
/// resolves through that caller's row, and anything else stops the row.
fn helper_transferred_rows(
    owner_parameters: &[String],
    chain: &super::helper_transfer::HelperChain,
    related_tests: &[&TestSummary],
) -> Vec<Vec<ParameterValue>> {
    let Some(entry) = chain.hops.last() else {
        return Vec::new();
    };
    let entry_parameters = function_parameters(&entry.caller);
    let mut rows =
        owner_call_parameter_values(related_tests, &entry.caller.name, &entry_parameters);
    if rows.is_empty() {
        return Vec::new();
    }
    // hops[0] is the owner's direct caller; hops[len-1] is the entry a
    // test can call. Bind from the entry downward.
    for step in (0..chain.hops.len()).rev() {
        let hop = &chain.hops[step];
        let target_parameters: Vec<String> = if step == 0 {
            owner_parameters.to_vec()
        } else {
            function_parameters(&chain.hops[step - 1].caller)
        };
        // The hop's call site lives in the hop's own caller: its
        // arguments reference THAT function's parameters (#3296 review
        // M3 — the previous off-by-one silently dropped every row when
        // parameter names differed between hops).
        let caller_parameters = function_parameters(&hop.caller);
        let mut bound_rows = Vec::new();
        for row in &rows {
            let mut bound = Vec::new();
            for (index, argument) in hop.arguments.iter().enumerate() {
                let Some(parameter) = target_parameters.get(index) else {
                    return Vec::new();
                };
                let Some(value) = bind_helper_argument(argument, &caller_parameters, row) else {
                    return Vec::new();
                };
                bound.push(ParameterValue {
                    parameter: parameter.clone(),
                    value,
                    line: row.first().map(|cell| cell.line).unwrap_or_default(),
                    text: row
                        .first()
                        .map(|cell| cell.text.clone())
                        .unwrap_or_default(),
                });
            }
            bound_rows.push(bound);
        }
        rows = bound_rows;
    }
    rows
}

/// One bound argument: a literal, or the caller's own parameter
/// resolved through the caller's row. A computed argument stops the
/// transfer for that chain (#3296: named edge, no guessed value).
fn bind_helper_argument(
    argument: &str,
    caller_parameters: &[String],
    row: &[ParameterValue],
) -> Option<String> {
    // #3296 review B2: only a strict whole-token literal binds; the
    // substring scanner must never fabricate a value from an
    // identifier like `a2`.
    if let Some(literal) = super::helper_transfer::strict_literal(argument) {
        return Some(literal);
    }
    let trimmed = argument.trim();
    caller_parameters
        .iter()
        .find(|parameter| parameter.as_str() == trimmed)
        .and_then(|parameter| {
            row.iter()
                .find(|cell| cell.parameter == *parameter)
                .map(|cell| cell.value.clone())
        })
}

fn parameter_value(row: &[ParameterValue], parameter: &str) -> Option<ParameterValue> {
    row.iter()
        .find(|value| value.parameter == parameter)
        .cloned()
}

fn parameter_value_set(rows: &[Vec<ParameterValue>], parameter: &str) -> Option<Vec<String>> {
    let mut values = observed_parameter_values(rows, parameter);
    if values.is_empty() {
        None
    } else {
        values.sort();
        values.dedup();
        Some(values)
    }
}

fn observed_parameter_values(rows: &[Vec<ParameterValue>], parameter: &str) -> Vec<String> {
    let mut values = rows
        .iter()
        .flat_map(|row| {
            row.iter()
                .filter(|value| value.parameter == parameter)
                .map(|value| value.value.clone())
        })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

pub(crate) fn function_parameters(function: &FunctionSummary) -> Vec<String> {
    let signature = function
        .body
        .lines()
        .next()
        .unwrap_or(function.body.as_str());
    parameters_in_signature(signature)
}

/// Parameter names of `function` read from its whole signature, so a
/// parameter list formatted across several lines is still seen. The
/// signature ends at the body's first `{` (#6970 review).
/// Names `function` binds as parameters, read from its whole signature:
/// a parameter list formatted across several lines is still seen, and a
/// destructuring pattern (`Input { subtotal }: Input`) yields the names it
/// binds. Field names in a pattern are kept too, which only widens the
/// owner-scoped set (#6970 review).
pub(crate) fn signature_parameters(function: &FunctionSummary) -> Vec<String> {
    let body = function.body.as_str();
    let Some(arguments) =
        parameter_list_open(body).and_then(|open| delimited_contents_at(body, open))
    else {
        return Vec::new();
    };
    split_top_level_args(&arguments)
        .iter()
        .flat_map(|argument| pattern_bindings(top_level_pattern(argument)))
        .collect()
}

/// The `(` opening the parameter list: the first one after `fn` that sits
/// outside the generic parameter list (`fn apply<F: Fn(u8)>(f: F)`).
fn parameter_list_open(body: &str) -> Option<usize> {
    let start = body.find("fn ").map_or(0, |at| at + 3);
    let mut angle = 0i32;
    let mut previous = ' ';
    for (offset, ch) in body[start..].char_indices() {
        let arrow = previous == '-';
        previous = ch;
        match ch {
            '<' => angle += 1,
            '>' if !arrow => angle -= 1,
            '(' if angle == 0 => return Some(start + offset),
            '{' | ';' => return None,
            _ => {}
        }
    }
    None
}

/// The pattern of one parameter: the text before its top-level `:`.
fn top_level_pattern(argument: &str) -> &str {
    let bytes = argument.as_bytes();
    let mut depth = 0i32;
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b':' if depth == 0 => {
                let path = bytes.get(index + 1) == Some(&b':')
                    || index.checked_sub(1).and_then(|at| bytes.get(at)) == Some(&b':');
                if !path {
                    return &argument[..index];
                }
            }
            _ => {}
        }
    }
    ""
}

/// Lowercase identifiers a parameter pattern names, without `ref`, `mut`,
/// `self` or path roots.
fn pattern_bindings(pattern: &str) -> Vec<String> {
    pattern
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| {
            word.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                && !matches!(*word, "_" | "ref" | "mut" | "self" | "crate" | "super")
        })
        .map(str::to_string)
        .collect()
}

fn parameters_in_signature(signature: &str) -> Vec<String> {
    let Some(arguments) = delimited_contents_after(signature, '(') else {
        return Vec::new();
    };
    split_top_level_args(&arguments)
        .into_iter()
        .filter_map(|argument| {
            argument
                .split_once(':')
                .map(|(name, _)| name.trim().to_string())
        })
        .filter(|name| !name.is_empty() && name != "self" && name != "&self" && name != "mut self")
        .collect()
}

fn boundary_operand_parameter(
    function: &FunctionSummary,
    parameters: &[String],
    operand: &str,
) -> Option<String> {
    parameters
        .iter()
        .find(|parameter| parameter.as_str() == operand)
        .cloned()
        .or_else(|| boundary_local_operand_parameter(function, parameters, operand))
}

fn boundary_local_operand_parameter(
    function: &FunctionSummary,
    parameters: &[String],
    operand: &str,
) -> Option<String> {
    if operand.is_empty() {
        return None;
    }
    for parameter in parameters {
        if body_contains_wrapped_local_alias(&function.body, "Some", operand, parameter)
            || body_contains_wrapped_local_alias(&function.body, "Ok", operand, parameter)
            || body_contains_direct_local_alias(&function.body, operand, parameter)
        {
            return Some(parameter.clone());
        }
    }
    None
}

fn body_contains_wrapped_local_alias(
    body: &str,
    wrapper: &str,
    operand: &str,
    parameter: &str,
) -> bool {
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        let prefix = format!("if let {wrapper}({operand}) = ");
        line.strip_prefix(&prefix)
            .is_some_and(|rest| starts_with_identifier_token(rest, parameter))
    }) || (body_contains_match_parameter(body, parameter)
        && body_contains_wrapper_pattern(body, wrapper, operand))
}

fn body_contains_match_parameter(body: &str, parameter: &str) -> bool {
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        if is_comment_line(line) {
            return false;
        }
        line.find("match ")
            .map(|index| &line[index + "match ".len()..])
            .is_some_and(|rest| starts_with_identifier_token(rest, parameter))
    })
}

fn body_contains_wrapper_pattern(body: &str, wrapper: &str, operand: &str) -> bool {
    let pattern = format!("{wrapper}({operand})");
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        !is_comment_line(line) && line.contains(&pattern)
    })
}

fn code_line_before_comment(line: &str) -> &str {
    let line = line.trim();
    let line = line.split_once("//").map_or(line, |(code, _comment)| code);
    line.split_once("/*")
        .map_or(line, |(code, _comment)| code)
        .trim()
}

fn is_comment_line(line: &str) -> bool {
    line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
}

fn starts_with_identifier_token(text: &str, token: &str) -> bool {
    let text = text.trim_start();
    let end = text
        .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .unwrap_or(text.len());
    end > 0 && &text[..end] == token
}

fn body_contains_direct_local_alias(body: &str, operand: &str, parameter: &str) -> bool {
    body.lines().any(|line| {
        let line = line.trim().trim_end_matches(';').trim();
        let Some(binding) = line.strip_prefix("let ") else {
            return false;
        };
        let Some((left, right)) = binding.split_once('=') else {
            return false;
        };
        let local_name = left.split_once(':').map(|(name, _)| name).unwrap_or(left);
        local_name.trim() == operand && right.trim() == parameter
    })
}

pub(in crate::analysis) fn comparison_operands(expression: &str) -> Option<(String, String)> {
    for operator in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some((left, right)) = expression.split_once(operator) {
            let left = clean_operand(left);
            let right = clean_operand(right);
            if !left.is_empty() && !right.is_empty() {
                return Some((left, right));
            }
        }
    }
    None
}

/// The comparison operands with the owner-bound side first. A reversed
/// predicate (`100 < amount`) compares the parameter on the right, but
/// every boundary resolver reads the left operand as the tested input and
/// the right one as the boundary, so the operands swap when only the right
/// side binds to a parameter or a local (#4228). Equality is symmetric,
/// so the swap changes which side is looked up, not what is compared.
fn oriented_comparison_operands(
    owner: &FunctionSummary,
    parameters: &[String],
    expression: &str,
    predicate_line: usize,
) -> Option<(String, String)> {
    let (left, right) = comparison_operands(expression)?;
    let owner_bound = |operand: &str| {
        boundary_operand_parameter(owner, parameters, operand).is_some()
            || live_local_initializer(owner, operand, predicate_line).is_some()
            // Any declared local keeps the left side, as before #4228:
            // the swap exists for literals, not to reinterpret locals.
            || owner_declares_local(owner, operand, usize::MAX)
    };
    if !owner_bound(&left) && owner_bound(&right) {
        return Some((right, left));
    }
    Some((left, right))
}

fn clean_operand(operand: &str) -> String {
    let cleaned = operand
        .trim()
        .trim_start_matches("if ")
        .trim_end_matches('{')
        .trim_end_matches(';')
        .trim();
    let cleaned = cleaned
        .split_once('{')
        .map(|(before, _)| before.trim())
        .unwrap_or(cleaned);
    cleaned.to_string()
}

/// The constant a changed comparison's right-hand operand names
/// (`amount >= DISCOUNT_THRESHOLD` -> `DISCOUNT_THRESHOLD`), if any.
pub(in crate::analysis) fn boundary_constant_operand_name(expression: &str) -> Option<String> {
    let (_, right) = comparison_operands(expression)?;
    crate::analysis::value_resolution::constant_operand_name(&right).map(str::to_string)
}

/// A boundary operand that names a constant (`DISCOUNT_THRESHOLD`,
/// `Self::LIMIT`), with what the owner's source file says about it through
/// the shared named-constant lookup in `analysis::value_resolution`.
struct BoundaryConstant {
    name: String,
    lookup: crate::analysis::value_resolution::NamedConstant,
    /// A bounded integer offset (`CURRENT - 2` -> `-2`, #6671); zero for
    /// the bare constant. Only an offset-free operand is the boundary by
    /// identity when a test names the constant.
    offset: i128,
}

/// Resolve a comparison operand that names a constant. Only a
/// constant-shaped operand is looked up, and only in the owner's own
/// source file.
fn boundary_constant(
    owner: &FunctionSummary,
    operand: &str,
    index: &crate::analysis::rust_index::RustIndex,
) -> Option<BoundaryConstant> {
    let (name, offset) = crate::analysis::value_resolution::constant_operand_name(operand)
        .map(|name| (name, 0))
        .or_else(|| crate::analysis::value_resolution::constant_offset_operand(operand))?;
    let lookup = index.files().get(&owner.file).map_or(
        crate::analysis::value_resolution::NamedConstant::Undeclared,
        |facts| crate::analysis::value_resolution::named_constant(&facts.source, name),
    );
    Some(BoundaryConstant {
        name: name.to_string(),
        lookup,
        offset,
    })
}

/// The statically visible value of the right-hand boundary operand: its
/// literal, or the literal value of the same-file constant it names.
fn visible_operand(operand: &str, constant: Option<&BoundaryConstant>) -> Option<ExactOperand> {
    if let Some(value) = literal_operand_value(operand) {
        return Some(ExactOperand {
            provenance: format!("literal operand {operand} = {value}"),
            value,
        });
    }
    let constant = constant?;
    let declared = constant.lookup.value()?;
    let value = if constant.offset == 0 {
        declared.to_string()
    } else {
        // RIPR-SPEC-0001 named-constant rule plus a bounded integer
        // offset: `CURRENT - 2` with `const CURRENT: u32 = 7;` is 5,
        // never the offset literal 2 (#6671).
        // `NamedConstant::Value` holds only a plain decimal literal; a
        // suffixed, hex, octal, or binary initializer (`10u32`, `0x10`)
        // is `Opaque`, never reaches here, and the offset boundary then
        // stays unresolved rather than parsed from a guess.
        crate::analysis::value_resolution::offset_integer_value(declared, constant.offset)?
    };
    Some(ExactOperand {
        provenance: format!("constant {operand} = {value} (same-file const)"),
        value,
    })
}

/// Direct owner calls in related tests whose argument for the compared
/// parameter names the boundary constant itself (`discounted_total(
/// DISCOUNT_THRESHOLD)`): that argument is the boundary value by
/// identity, whatever its value. Requires the owner's file to declare the
/// constant exactly once, and skips a test whose own file may declare a
/// constant of the same name.
fn owner_calls_passing_constant(
    related_tests: &[&TestSummary],
    owner: &FunctionSummary,
    parameters: &[String],
    left_parameter: &str,
    constant: &BoundaryConstant,
    index: &crate::analysis::rust_index::RustIndex,
) -> Vec<(usize, String)> {
    let Some(position) = parameters
        .iter()
        .position(|parameter| parameter == left_parameter)
    else {
        return Vec::new();
    };
    if !constant.lookup.is_declared_once() || constant.offset != 0 {
        return Vec::new();
    }
    related_tests
        .iter()
        .filter(|test| {
            !crate::analysis::value_resolution::test_file_may_shadow_constant(
                &owner.file,
                &test.file,
                index
                    .files()
                    .get(&test.file)
                    .map(|facts| facts.data().source.as_ref()),
                &constant.name,
            )
        })
        .flat_map(|test| test.calls.iter())
        .filter(|call| call.name == owner.name)
        .filter_map(|call| {
            let arguments = call_arguments(&call.text, &call.name)?;
            let argument = arguments.get(position)?;
            crate::analysis::value_resolution::argument_names_constant(argument, &constant.name)
                .then(|| (call.line, call.text.clone()))
        })
        .collect()
}

pub(in crate::analysis) fn literal_operand_value(operand: &str) -> Option<String> {
    // A computed operand (`2 + 2`, `CURRENT - 2`) is not the literal it
    // happens to contain: reading `CURRENT - 2` as `2` names a boundary
    // the code never compares against (#6671). It stays unresolved.
    if is_computed_value_expression(operand) {
        return None;
    }
    scalar_values(operand).into_iter().next()
}

/// Whether `text` computes its value rather than spelling it: a binary
/// arithmetic or bitwise operator (`base + 1`, `CURRENT - 2`, `n * 2`,
/// `flags | 1`) or an array-repeat length (`[b'f'; 16]`, whose `16` is a
/// length, not an element value). The literals inside such an expression
/// are its operands, not its value, so RIPR-SPEC-0001's rule that a
/// computed binding is not an exact input value applies (#6672, #6671).
/// An operand may end in `)`, `]`, `}` (`{ x } + 1`) or `?`
/// (`parse(s)? - 1`).
/// String and char literal contents are skipped; a unary minus (`-5`,
/// `f(-5)`), a reference (`&x`), a dereference (`*x`), and an
/// exponent sign (`1e-5`) are not binary operators.
pub(in crate::analysis) fn is_computed_value_expression(text: &str) -> bool {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(text);
    let chars: Vec<char> = masked.chars().collect();
    let mut bracket_depth = 0usize;
    // The last non-space character.
    let mut previous: Option<char> = None;
    let mut idx = 0usize;
    while idx < chars.len() {
        let ch = chars[idx];
        // A char literal (`'+'`, `'\n'`) is one operand; its contents are
        // never an operator. A lifetime (`'a`) has no closing quote.
        if ch == '\'' {
            let close = if chars.get(idx + 1) == Some(&'\\') {
                idx + 3
            } else {
                idx + 2
            };
            if chars.get(close) == Some(&'\'') {
                previous = Some('\'');
                idx = close + 1;
                continue;
            }
        }
        let after = chars.get(idx + 1).copied();
        let operand_before = previous.is_some_and(|prev| {
            prev.is_ascii_alphanumeric() || matches!(prev, '_' | ')' | ']' | '}' | '?' | '\'')
        });
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ';' if bracket_depth > 0 => return true,
            // `1 << 4`, `x >> 1`: shifts compute a value too.
            '<' | '>' if operand_before && after == Some(ch) => return true,
            '+' | '-' | '*' | '/' | '%' | '^' | '&' | '|' if operand_before => {
                // `->` (return type), `&&`/`||` (boolean, not a value).
                let not_value_operator = matches!(
                    (ch, after),
                    ('-', Some('>')) | ('&', Some('&')) | ('|', Some('|'))
                );
                let exponent_sign = matches!(ch, '+' | '-') && is_float_exponent_sign(&chars, idx);
                if !not_value_operator && !exponent_sign {
                    return true;
                }
            }
            _ => {}
        }
        if !ch.is_whitespace() {
            previous = Some(ch);
        }
        idx += 1;
    }
    false
}

/// Whether the sign at `sign` is a decimal float exponent's (`1e-5`,
/// `2.5E+3`): it immediately follows the `e`/`E` of a token that is a
/// decimal mantissa. `16usize - 1`, `2isize + 1` and `0xFE - 1` end in an
/// `e`/`E` that is a suffix or a hex digit, so their signs are operators.
fn is_float_exponent_sign(chars: &[char], sign: usize) -> bool {
    let Some(exponent) = sign.checked_sub(1) else {
        return false;
    };
    if !matches!(chars[exponent], 'e' | 'E') {
        return false;
    }
    let mut start = exponent;
    while start > 0
        && (chars[start - 1].is_ascii_alphanumeric() || matches!(chars[start - 1], '_' | '.'))
    {
        start -= 1;
    }
    let mantissa = &chars[start..exponent];
    mantissa.first().is_some_and(char::is_ascii_digit)
        && mantissa
            .iter()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '_' | '.'))
}

fn comparable_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .chars()
        .filter(|ch| *ch != '_')
        .collect()
}

fn first_visible_flow_sink(flow_sinks: &[FlowSinkFact]) -> Option<&FlowSinkFact> {
    flow_sinks
        .iter()
        .find(|sink| sink.kind != FlowSinkKind::Unknown)
}

fn list_or_unknown(values: &[String]) -> String {
    if values.is_empty() {
        "unknown".to_string()
    } else {
        values.join(", ")
    }
}

/// The values a related test feeds into the changed owner's inputs:
/// arguments of a direct owner call (literal or let-bound, per
/// [`owner_argument_values`]), table-row cells, and builder-method
/// arguments. Assertion arguments are oracle values (the expected side of
/// `assert_eq!(other(x), 2000)`), not inputs, and are excluded, as are the
/// derived `left == right` boundary facts. A `parameter = value` fact
/// yields its bare value.
pub(in crate::analysis) fn owner_input_values(activation: &ActivationEvidence) -> Vec<&str> {
    activation
        .observed_values
        .iter()
        .filter(|fact| {
            matches!(
                fact.context,
                ValueContext::FunctionArgument
                    | ValueContext::TableRow
                    | ValueContext::BuilderMethod
            ) && !fact.value.contains(" == ")
        })
        .map(|fact| {
            fact.value
                .split_once(" = ")
                .filter(|(parameter, _)| is_plain_identifier(parameter))
                .map_or(fact.value.as_str(), |(_, value)| value)
        })
        .collect()
}

/// The scalar values one owner-call argument carries. A literal argument
/// yields its literals; a bare identifier yields the scalar it is bound to
/// by a `let IDENT = LITERAL;` in the same test body, read through the
/// shared value-extraction authority
/// ([`crate::analysis::value_resolution::test_let_bound_literal`]) rather
/// than a second let scanner. Only a bound number or boolean counts: the
/// shared scan strips string contents, so a string binding is not an exact
/// value here. An identifier with no `let` binding may be an rstest
/// `#[case]` parameter, which yields one value per case row
/// ([`crate::analysis::value_resolution::test_case_bound_literals`]).
/// Anything else (a computed expression, a non-literal initializer)
/// yields nothing.
pub(in crate::analysis) fn owner_argument_values(
    test: &TestSummary,
    argument: &str,
) -> Vec<String> {
    // `order_discount(base + 1)` passes neither `1` nor `base`: a
    // computed argument is not an exact input value (RIPR-SPEC-0001,
    // #6672), so it yields nothing rather than one of its operands.
    if is_computed_value_expression(argument) {
        return Vec::new();
    }
    let direct = scalar_values(argument);
    if !direct.is_empty() {
        return direct;
    }
    let name = argument.trim();
    if !is_plain_identifier(name) {
        return Vec::new();
    }
    let let_bound: Vec<String> =
        crate::analysis::value_resolution::test_let_bound_literal(&test.body, name)
            .filter(|value| !value.starts_with(['"', '\'']))
            .filter(|value| scalar_values(value).as_slice() == std::slice::from_ref(value))
            .into_iter()
            .collect();
    if !let_bound.is_empty() {
        return let_bound;
    }
    // An rstest `#[case]` parameter carries one value per case row.
    crate::analysis::value_resolution::test_case_bound_literals(test, name)
        .into_iter()
        .filter(|value| !value.starts_with(['"', '\'']))
        .filter(|value| scalar_values(value).as_slice() == std::slice::from_ref(value))
        .collect()
}

fn is_plain_identifier(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(in crate::analysis) fn has_observed_boundary_equality(activation: &ActivationEvidence) -> bool {
    activation
        .observed_values
        .iter()
        .any(|fact| fact.value.contains(" == "))
}

fn sort_value_facts(facts: &mut Vec<ValueFact>) {
    facts.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.context.as_str().cmp(right.context.as_str()))
            .then(left.value.cmp(&right.value))
            .then(left.text.cmp(&right.text))
    });
    facts.dedup_by(|left, right| {
        left.line == right.line
            && left.text == right.text
            && left.value == right.value
            && left.context == right.context
    });
}

pub(in crate::analysis) fn call_arguments(text: &str, name: &str) -> Option<Vec<String>> {
    let needle = format!("{name}(");
    let start = text.find(&needle)? + name.len();
    let contents = delimited_contents_at(text, start)?;
    Some(split_top_level_args(&contents))
}

fn macro_arguments(text: &str) -> Option<Vec<String>> {
    let start = text.find("!(")? + 1;
    let contents = delimited_contents_at(text, start)?;
    Some(split_top_level_args(&contents))
}

fn delimited_contents_after(text: &str, delimiter: char) -> Option<String> {
    let start = text.find(delimiter)?;
    delimited_contents_at(text, start)
}

fn split_top_level_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                if let Some(arg) = text.get(start..idx).map(str::trim)
                    && !arg.is_empty()
                {
                    args.push(arg.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    if let Some(arg) = text.get(start..).map(str::trim)
        && !arg.is_empty()
    {
        args.push(arg.to_string());
    }
    args
}

/// `scalar_values` of a table-row or builder line, skipping every
/// argument that computes its value, the same RIPR-SPEC-0001 rule
/// owner-call arguments follow (#6672). The line is split at top-level
/// commas with full `()`/`[]`/`{}` depth; a char or byte literal (`','`,
/// `b'('`) and a string are one unit, never a delimiter. An argument is
/// computed when the text outside its groups is (`.amount(base.len() +
/// 10)` passes neither 10 nor `base`, `(a + 1) * 2` passes neither 1 nor
/// 2) or when it is an array repeat (`[b'f'; 16]`'s 16 is a length);
/// otherwise its own scalar is read and its groups (`Some(5)`, `(',',
/// true)`, `Case { n: 1 }`) are read as argument lists in turn.
/// A line nested deeper than [`MAX_SPELLED_DEPTH`] yields no values; see
/// [`spelled_line_is_unreadable`].
fn spelled_scalar_values(line: &str) -> Vec<String> {
    if spelled_line_is_unreadable(line) {
        return Vec::new();
    }
    let mut values = Vec::new();
    collect_spelled_values(line, &mut values, 0);
    values.sort();
    values.dedup();
    values
}

/// The deepest `()`/`[]`/`{}` nesting a table-row or builder line may
/// have and still be read. Real rows nest a few levels; the bound keeps a
/// hostile line (`(1, ` repeated thousands of times) from overflowing the
/// stack or re-masking each level.
const MAX_SPELLED_DEPTH: usize = 32;

/// Whether a table-row or builder line nests deeper than
/// [`MAX_SPELLED_DEPTH`]. Its values cannot be read, so a related test
/// holding one leaves the changed boundary unresolved rather than missing
/// an input it may well pass.
fn spelled_line_is_unreadable(line: &str) -> bool {
    // Too few openers to nest that deep: skip the masking scan.
    if line
        .bytes()
        .filter(|byte| matches!(byte, b'(' | b'[' | b'{'))
        .count()
        <= MAX_SPELLED_DEPTH
    {
        return false;
    }
    let mut depth = 0usize;
    for byte in structural_bytes(line) {
        match byte {
            b'(' | b'[' | b'{' => {
                depth += 1;
                if depth > MAX_SPELLED_DEPTH {
                    return true;
                }
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// The reason a changed boundary is unresolved when a related test that
/// may feed the owner from its rows holds a table-row or builder line too
/// deeply nested to read. Only a test that calls the owner (or the helper
/// chain's entry) with an argument that is not an exact value (`score(n)`
/// over a row, `score(req.amount)`) reads its inputs from such lines; a
/// test whose owner calls are all exact (`score(20)`) is read from those
/// calls, so a deep row it feeds elsewhere does not hide its input.
fn unreadable_spelled_line_reason(
    owner: &FunctionSummary,
    related_tests: &[&TestSummary],
    helper_chain: Option<&super::helper_transfer::HelperChain>,
) -> Option<String> {
    let entry_name = helper_chain
        .and_then(|chain| chain.hops.last())
        .map(|hop| hop.caller.name.as_str());
    related_tests
        .iter()
        .filter(|test| {
            test.body_calls().any(|call| {
                (call.name == owner.name || Some(call.name.as_str()) == entry_name)
                    && call_arguments(&call.text, &call.name).is_some_and(|arguments| {
                        arguments
                            .iter()
                            .any(|argument| owner_argument_values(test, argument).is_empty())
                    })
            })
        })
        .flat_map(|test| test.body.lines())
        .map(str::trim)
        .any(|line| {
            (looks_like_table_row(line) || looks_like_builder_method(line))
                && spelled_line_is_unreadable(line)
        })
        .then(|| {
            format!(
                "a related test's table row or builder line nests deeper than {MAX_SPELLED_DEPTH} levels, so ripr cannot read the inputs it passes"
            )
        })
}

fn collect_spelled_values(text: &str, values: &mut Vec<String>, depth: usize) {
    if depth > MAX_SPELLED_DEPTH {
        return;
    }
    let structure = structural_bytes(text);
    for (start, end) in top_level_comma_ranges(&structure) {
        let (Some(argument), Some(argument_structure)) =
            (text.get(start..end), structure.get(start..end))
        else {
            continue;
        };
        if argument.trim().is_empty() {
            continue;
        }
        let shape = argument_shape(argument, argument_structure);
        // Below the line itself, a group read through a call (`x.min(10)`,
        // `parse(10)`, `rows[0]`, a block) computes the argument's value;
        // only a constructor or a bare tuple/array passes its contents
        // through. The line's own groups are its setters and rows.
        if shape.array_repeat
            || (depth > 0 && !shape.all_groups_transparent)
            || is_computed_value_expression(&shape.outside)
        {
            continue;
        }
        values.extend(scalar_values(&shape.outside));
        for group in shape.groups {
            collect_spelled_values(group, values, depth + 1);
        }
    }
}

/// The bytes of `text` with comments, string contents and char/byte
/// literals blanked, so only real delimiters remain. Byte offsets match
/// `text`.
fn structural_bytes(text: &str) -> Vec<u8> {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(text);
    let chars = masked.char_indices().collect::<Vec<_>>();
    let mut bytes = masked.into_bytes();
    let mut idx = 0usize;
    while idx < chars.len() {
        if chars[idx].1 == '\''
            && let Some(close) = char_literal_close(&chars, idx)
        {
            let end = chars[close].0 + 1;
            if let Some(span) = bytes.get_mut(chars[idx].0..end) {
                span.fill(b' ');
            }
            idx = close + 1;
            continue;
        }
        idx += 1;
    }
    bytes
}

/// The index of the quote closing a char literal opened at `open`
/// (`'x'`, `'\n'`, `'\''`, `'\x41'`, `'\u{1F600}'`); `None` for a
/// lifetime (`'a`).
fn char_literal_close(chars: &[(usize, char)], open: usize) -> Option<usize> {
    match chars.get(open + 1)?.1 {
        '\'' => None,
        '\\' => {
            if chars.get(open + 2)?.1 == '\'' {
                return (chars.get(open + 3)?.1 == '\'').then_some(open + 3);
            }
            (open + 3..chars.len().min(open + 13)).find(|&idx| chars[idx].1 == '\'')
        }
        _ => (chars.get(open + 2)?.1 == '\'').then_some(open + 2),
    }
}

/// Byte ranges of the top-level comma-separated arguments of `structure`.
fn top_level_comma_ranges(structure: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (offset, byte) in structure.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                ranges.push((start, offset));
                start = offset + 1;
            }
            _ => {}
        }
    }
    ranges.push((start, structure.len()));
    ranges
}

struct ArgumentShape<'a> {
    /// The argument with each top-level group's contents removed
    /// (`Some(base) + 1` → `Some() + 1`).
    outside: String,
    /// The contents of each top-level `()`, `[]` and `{}` group.
    groups: Vec<&'a str>,
    /// A top-level `[value; length]`.
    array_repeat: bool,
    /// Every top-level group passes its contents through as values: see
    /// [`group_is_transparent`].
    all_groups_transparent: bool,
}

/// Whether the group opened by `opener` after `prefix` passes its contents
/// through as values: a bare tuple or array (`(10, true)`, `[1, 2]`), a
/// constructor or variant whose last path segment is UpperCamel
/// (`Some(10)`, `Ok(1)`, `Token::Num(3)`, `Case { n: 1 }`), or `vec![..]`.
/// A method call (`x.min(10)`, `10.min(x)`), a lowercase function or
/// macro (`parse(10)`, `format!(..)`), an index (`rows[0]`), a bare block
/// (`{ x }`) or a call on a call computes its value instead.
fn group_is_transparent(prefix: &str, opener: u8) -> bool {
    let prefix = prefix.trim_end();
    let Some(last) = prefix.chars().last() else {
        return opener != b'{';
    };
    if last == '!' {
        let name = prefix[..prefix.len() - 1]
            .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .next()
            .unwrap_or_default();
        return name == "vec" && opener != b'{';
    }
    if !(last.is_ascii_alphanumeric() || last == '_') {
        // `(`, `,`, `&`, `=` before the group: a bare tuple or array. A
        // call on a call (`f(x)(1)`, `rows[0](1)`), a generic call
        // (`f::<T>(1)`) or a bare block is not.
        return !matches!(last, ')' | ']' | '}' | '>' | '?') && opener != b'{';
    }
    let ident_start = prefix
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_')
        .last()
        .map_or(prefix.len(), |(idx, _)| idx);
    let ident = &prefix[ident_start..];
    let method = prefix[..ident_start].trim_end().ends_with('.');
    !method
        && opener != b'['
        && ident
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn argument_shape<'a>(argument: &'a str, structure: &[u8]) -> ArgumentShape<'a> {
    let mut outside = String::new();
    let mut groups = Vec::new();
    let mut array_repeat = false;
    let mut depth = 0usize;
    let mut cursor = 0usize;
    let mut group_start = 0usize;
    let mut opener = b'(';
    let mut all_groups_transparent = true;
    for (offset, byte) in structure.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => {
                if depth == 0 {
                    all_groups_transparent &=
                        group_is_transparent(argument.get(..offset).unwrap_or_default(), *byte);
                    outside.push_str(argument.get(cursor..=offset).unwrap_or_default());
                    group_start = offset + 1;
                    opener = *byte;
                }
                depth += 1;
            }
            b')' | b']' | b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    groups.push(argument.get(group_start..offset).unwrap_or_default());
                    cursor = offset;
                }
            }
            b';' if depth == 1 && opener == b'[' => array_repeat = true,
            _ => {}
        }
    }
    if depth == 0 {
        outside.push_str(argument.get(cursor..).unwrap_or_default());
    } else {
        groups.push(argument.get(group_start..).unwrap_or_default());
    }
    ArgumentShape {
        outside,
        groups,
        array_repeat,
        all_groups_transparent,
    }
}

fn scalar_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let chars = text.char_indices().collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < chars.len() {
        let (byte_idx, ch) = chars[idx];
        if ch == '"' {
            let mut end = byte_idx + ch.len_utf8();
            let mut cursor = idx + 1;
            let mut escaped = false;
            while cursor < chars.len() {
                let (next_byte, next_ch) = chars[cursor];
                end = next_byte + next_ch.len_utf8();
                if escaped {
                    escaped = false;
                } else if next_ch == '\\' {
                    escaped = true;
                } else if next_ch == '"' {
                    break;
                }
                cursor += 1;
            }
            if let Some(value) = text.get(byte_idx..end) {
                values.push(value.to_string());
            }
            idx = cursor.saturating_add(1);
            continue;
        }
        if ch == '\'' {
            // A char literal `'x'` / `'\n'` / `'\''`; a lifetime
            // (`'a`, never closed by a quote on its own) is not a
            // value. #3295: char arguments are exact inputs.
            let closing = if chars
                .get(idx + 1)
                .is_some_and(|(_, next_ch)| *next_ch == '\\')
            {
                chars.get(idx + 3)
            } else {
                chars.get(idx + 2)
            };
            if let Some((end_byte, end_ch)) = closing
                && *end_ch == '\''
                && let Some(value) = text.get(byte_idx..end_byte + end_ch.len_utf8())
            {
                values.push(value.to_string());
                idx = chars
                    .iter()
                    .position(|(scan_byte, _)| *scan_byte == *end_byte)
                    .map_or(idx + 1, |position| position.saturating_add(1));
                continue;
            }
        }
        // Boolean literals are exact inputs too (#3295). Both edges
        // must be identifier boundaries: `is_true`/`true_flag` are
        // identifiers, not booleans (#3295 review).
        for literal in ["true", "false"] {
            if text[byte_idx..].starts_with(literal) {
                let before = text[..byte_idx].chars().next_back();
                let after = text[byte_idx + literal.len()..].chars().next();
                let before_ok = !before
                    .is_some_and(|prev_ch: char| prev_ch.is_ascii_alphanumeric() || prev_ch == '_');
                let after_ok = !after
                    .is_some_and(|next_ch: char| next_ch.is_ascii_alphanumeric() || next_ch == '_');
                if before_ok && after_ok {
                    values.push(literal.to_string());
                    idx += 1;
                    break;
                }
            }
        }
        // Digits inside an identifier or a type suffix (`x1`, the `32` of
        // `99u32`, the `64` of `1.5f64`) are not a separate value: read as
        // one they sort ahead of the real literal and become its boundary.
        let in_identifier = idx
            .checked_sub(1)
            .and_then(|prev| chars.get(prev))
            .is_some_and(|(_, prev_ch)| prev_ch.is_ascii_alphanumeric() || *prev_ch == '_');
        if ch.is_ascii_digit() && in_identifier {
            idx += 1;
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '-'
                && chars
                    .get(idx + 1)
                    .is_some_and(|(_, next_ch)| next_ch.is_ascii_digit()))
        {
            let mut end = byte_idx + ch.len_utf8();
            let mut cursor = idx + 1;
            let mut seen_fraction = false;
            while cursor < chars.len() {
                let (next_byte, next_ch) = chars[cursor];
                // A fraction (`1.5`) belongs to the literal: stopping at
                // the `.` would read `amount > 1.5` as `amount > 1` (#4271).
                // A `.` not followed by a digit is a range or method call.
                let fraction = next_ch == '.'
                    && !seen_fraction
                    && chars
                        .get(cursor + 1)
                        .is_some_and(|(_, after)| after.is_ascii_digit());
                if next_ch.is_ascii_digit() || next_ch == '_' || fraction {
                    seen_fraction |= fraction;
                    end = next_byte + next_ch.len_utf8();
                    cursor += 1;
                } else {
                    break;
                }
            }
            if let Some(value) = text.get(byte_idx..end) {
                values.push(value.to_string());
            }
            idx = cursor;
            continue;
        }
        idx += 1;
    }
    values.sort();
    values.dedup();
    values
}

fn looks_like_table_row(line: &str) -> bool {
    (line.starts_with('(') || line.starts_with('[') || line.contains("[(")) && line.contains(',')
}

fn looks_like_builder_method(line: &str) -> bool {
    line.contains('.')
        && line.contains('(')
        && (line.contains("builder")
            || line.contains("with_")
            || line.contains(".amount(")
            || line.contains(".token(")
            || line.contains(".threshold("))
}

#[cfg(test)]
mod tests {
    #[test]
    fn constructed_field_name_reads_plain_and_shorthand_initializers() {
        assert_eq!(constructed_field_name("retries: 1,"), Some("retries"));
        assert_eq!(constructed_field_name("retries,"), Some("retries"));
        assert_eq!(
            constructed_field_name("ptr: NonNull::from(Box::leak(ptr))"),
            Some("ptr")
        );
        assert_eq!(constructed_field_name("Self::default()"), None);
        assert_eq!(constructed_field_name("a + b"), None);
    }

    #[test]
    fn field_reads_count_only_on_the_owner_result() {
        let body =
            "fn t() {\n    let cfg = default_config();\n    let fallback = Config::fallback();\n";
        let reads = |assertion: &str| {
            reads_owner_result_field(body, assertion, ".retries", "default_config")
        };
        assert!(reads("assert_eq!(cfg.retries, 3);"));
        assert!(reads("assert_eq!(default_config().retries, 3);"));
        // #4428 review: an unrelated value's same-named field is not the
        // constructed field.
        assert!(!reads("assert_eq!(fallback.retries, 3);"));
        assert!(!reads("assert_eq!(cfg.retries_left, 3);"));
        assert!(!reads("assert_eq!(Config::fallback().retries, 3);"));
        // #4428 review: the read must sit on the owner call itself, not on
        // another call in the same assertion.
        assert!(!reads(
            "assert_eq!(other().retries, default_config().timeout_secs);"
        ));
        assert!(!reads("assert_eq!(my_default_config().retries, 3);"));
        assert!(reads(
            "assert_eq!(Config::default_config(\"x\").retries, 3);"
        ));
        assert!(reads("assert_eq!(default_config(load(1)).retries, 3);"));
        assert!(!reads_owner_result_field(
            "let e = make();",
            "assert!(e.downcast_ref::<Box<dyn E>>().is_some());",
            ".ptr",
            "make"
        ));
        assert!(reads_owner_result_field(
            "let mut cfg: Config = default_config();",
            "assert_eq!(cfg.retries, 3);",
            ".retries",
            "default_config"
        ));
    }

    #[test]
    fn a_field_overwritten_before_the_assertion_is_not_the_owner_field() {
        let reads = |body: &str, assertion: &str| {
            reads_owner_result_field(body, assertion, ".total", "bundle")
        };
        let withheld = [
            // RIPR-SPEC-0005: credit holds only before shadow or overwrite.
            "let q = bundle(3);\n let q = Quote { total: 99, ..q };\n assert_eq!(q.total, 99);",
            "let mut q = bundle(3);\n q.total = 99;\n assert_eq!(q.total, 99);",
            "let q = Quote { total: 99, ..bundle(3) };\n assert_eq!(q.total, 99);",
            // An assignment after a pass-through update still overwrites.
            "let q = bundle(3);\n let mut q = Quote { items: 4, ..q };\n q.total = 1;\n assert_eq!(q.total, 1);",
        ];
        for body in withheld {
            let assertion = body.rsplit('\n').next().unwrap_or_default().trim();
            assert!(!reads(body, assertion), "{body}");
        }
        let credited = [
            // A struct update that leaves the field to its base passes it
            // through, as does a value read back from the receiver.
            "let q = bundle(3);\n let q = Quote { items: 4, ..q };\n assert_eq!(q.total, 45);",
            "let q = bundle(3);\n let q = Quote { total: q.total, ..q };\n assert_eq!(q.total, 45);",
            "let q = bundle(3);\n let total = q.total;\n let q = Quote { total, ..q };\n assert_eq!(q.total, 45);",
            "let mut q = bundle(3);\n q.total = q.total;\n assert_eq!(q.total, 45);",
            // A value copied out of the receiver first is still the owner's.
            "let q = bundle(3);\n let t = q.total;\n let q = Quote { total: t, ..q };\n assert_eq!(q.total, 45);",
            "let mut q = bundle(3);\n let t = q.total;\n let u = t + 0;\n q.total = u;\n assert_eq!(q.total, 45);",
            // Plain assignment and a second owner binding carry the value too.
            "let mut q = bundle(3);\n let mut t = 0;\n t = q.total;\n q.total = t;\n assert_eq!(q.total, 45);",
            "let base = bundle(3);\n let q = bundle(1);\n let q = Quote { total: base.total, ..q };\n assert_eq!(q.total, 45);",
            // A char literal quote does not hide a closing brace.
            "let q = bundle(3);\n { let c = '\"'; let q = Quote { total: 1, ..q }; drop((c, q)); }\n assert_eq!(q.total, 45);",
            // Only a statement still in scope at the assertion counts.
            "let q = bundle(3);\n { let q = Quote { total: 1, ..q }; drop(q); }\n assert_eq!(q.total, 45);",
            "let mut q = bundle(3);\n if false { q.total = 0; }\n assert_eq!(q.total, 45);",
            "let q = bundle(3);\n let _f = |q: Quote| { let q = Quote { total: 0, ..q }; q };\n assert_eq!(q.total, 45);",
            // Comments and strings are not code.
            "let q = bundle(3);\n // let q = Quote { total: 0, ..q };\n assert_eq!(q.total, 45);",
            "let q = bundle(3);\n let s = \"let q = Quote { total: 0 };\";\n assert_eq!(q.total, 45);",
            "let mut q = bundle(3);\n // q.total = 0;\n assert_eq!(q.total, 45);",
            // Reads before the overwrite, comparisons and other fields.
            "let mut q = bundle(3);\n let ok = q.total == 45;\n assert_eq!(q.total, 45);",
            "let mut q = bundle(3);\n q.total_cap = 1;\n assert_eq!(q.total, 45);",
            // Ambiguous rebinding and positions keep today's credit.
            "let q = bundle(3);\n let q = adjust(q);\n assert_eq!(q.total, 45);",
        ];
        for body in credited {
            let assertion = body.rsplit('\n').next().unwrap_or_default().trim();
            assert!(reads(body, assertion), "{body}");
        }
        let before = "let mut q = bundle(3);\n assert_eq!(q.total, 45);\n q.total = 1;";
        assert!(reads(before, "assert_eq!(q.total, 45);"));
        let twice = "let q = bundle(3);\n let q = Quote { total: 9, ..q };\n assert_eq!(q.total, 9);\n assert_eq!(q.total, 9);";
        assert!(reads(twice, "assert_eq!(q.total, 9);"));
    }

    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::rust_index::{CallFact, LiteralFact, OracleFact};
    use std::path::PathBuf;

    #[test]
    fn activation_evidence_records_observed_boundary_equality() {
        let owner = function(
            "pub fn score(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(100, 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        assert!(has_observed_boundary_equality(&activation));
        assert!(activation.missing_discriminators.is_empty());
        assert!(activation.observed_values.iter().any(|fact| {
            fact.context == ValueContext::FunctionArgument && fact.value == "amount == threshold"
        }));
    }

    #[test]
    fn activation_evidence_resolves_direct_local_boundary_operand_alias() {
        let owner = function(
            "pub fn score(raw_amount: i32, threshold: i32) -> bool {\n    let amount = raw_amount;\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(100, 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        assert!(has_observed_boundary_equality(&activation));
        assert!(activation.missing_discriminators.is_empty());
        assert!(activation.observed_values.iter().any(|fact| {
            fact.context == ValueContext::FunctionArgument && fact.value == "amount == threshold"
        }));
    }

    // #3295 review F2: an identifier argument like `is_true` never
    // yields a boolean exact input.
    #[test]
    fn boolean_extraction_requires_identifier_boundaries() {
        assert!(scalar_values("check(is_true)").is_empty());
        assert!(scalar_values("run(x_false)").is_empty());
        assert_eq!(
            scalar_values("f(true_flag)"),
            Vec::<String>::new(),
            "trailing identifier chars reject the token"
        );
        assert_eq!(scalar_values("check(true)"), vec!["true".to_string()]);
        assert_eq!(scalar_values("check(Some(true))"), vec!["true".to_string()]);
    }

    // #3295 review N4: a local that re-binds a parameter name wins over
    // the raw call argument.
    #[test]
    fn shadowing_local_wins_over_the_parameter_argument() -> Result<(), String> {
        let owner = function(
            "pub fn split_after(input: &str) -> bool {
    let input = input.strip_prefix(\"x\").map_or(\"none\", |s| s);
    input == \"y\"
}",
        );
        let test = test_with_call("boundary", "score(\"xy\");");
        let mut probe = probe(ProbeFamily::Predicate, "input == \"y\"");
        probe.location = SourceLocation::new("src/lib.rs", 3, 1);

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        // strip_prefix("x") over "xy" yields exactly "y": the boundary
        // is observed through the shadowing local, not the raw "xy".
        assert!(
            has_observed_boundary_equality(&activation),
            "observed: {:?}",
            activation.observed_values
        );
        let Some(fact) = activation
            .observed_values
            .iter()
            .find(|fact| fact.value == "input == \"y\"")
        else {
            return Err("boundary fact missing".to_string());
        };
        assert!(
            fact.text.contains("strip_prefix -> map_or"),
            "provenance names the evaluated chain: {}",
            fact.text
        );
        Ok(())
    }

    // #3295: computed local operands resolve through the bounded
    // evaluator over the exact test inputs, so the #3215 equality
    // boundary is observed instead of `unknown`.
    #[test]
    fn activation_evidence_resolves_computed_local_boundary_operands() {
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::split_after".to_string()),
            name: "split_after".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 9,
            body: "pub fn split_after(input: &str, delim: char) -> &str {\n    let end = input.rfind(delim).map_or(1, |idx| idx);\n    let start = delim.len_utf8();\n    if end == start {\n        &input[..end]\n    } else {\n        input\n    }\n}".into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        let test = TestSummary {
            name: "absent_delimiter_boundary".to_string(),
            file: PathBuf::from("tests/split.rs"),
            start_line: 4,
            end_line: 6,
            body: "split_after(\"ab\", 'x');".into(),
            calls: vec![CallFact {
                name: "split_after".to_string(),
                line: 5,
                text: "split_after(\"ab\", 'x');".to_string(),
            }],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };
        let probe = Probe {
            id: ProbeId("probe:src_lib.rs:predicate:eval".to_string()),
            location: SourceLocation::new("src/lib.rs", 4, 1),
            owner: Some(SymbolId("src/lib.rs::split_after".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: Some("if end == start {".to_string()),
            expression: "if end == start {".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        assert!(
            has_observed_boundary_equality(&activation),
            "the exact inputs end=1, start=1 must observe the boundary: {:?}",
            activation.observed_values
        );
        assert!(activation.missing_discriminators.is_empty());
        assert!(activation.observed_values.iter().any(|fact| {
            fact.context == ValueContext::FunctionArgument && fact.value == "end == start"
        }));
    }

    #[test]
    fn same_file_method_chain_owner_call_tracing_covers_activation_alias_helpers() {
        let owner = function(
            "pub fn score(raw_amount: Option<i32>, threshold: i32) -> bool {\n    if let Some(amount) = raw_amount { amount >= threshold } else { false }\n}",
        );
        let parameters = function_parameters(&owner);

        assert_eq!(
            boundary_local_operand_parameter(&owner, &parameters, "amount"),
            Some("raw_amount".to_string())
        );
        assert_eq!(
            boundary_local_operand_parameter(&owner, &parameters, ""),
            None
        );
        assert!(body_contains_wrapped_local_alias(
            &owner.body,
            "Some",
            "amount",
            "raw_amount"
        ));
        let match_body =
            "match raw_amount {\n    Some(amount) => amount >= threshold,\n    None => false,\n}";
        assert!(body_contains_wrapped_local_alias(
            match_body,
            "Some",
            "amount",
            "raw_amount"
        ));
        assert!(body_contains_match_parameter(match_body, "raw_amount"));
        assert!(body_contains_wrapper_pattern(match_body, "Some", "amount"));
        assert!(body_contains_direct_local_alias(
            "let amount = raw_amount;\namount >= threshold",
            "amount",
            "raw_amount"
        ));
        assert!(!body_contains_match_parameter(
            "// match raw_amount { Some(amount) => amount >= threshold }",
            "raw_amount"
        ));
        assert!(!body_contains_wrapper_pattern(
            "// Some(amount) => amount >= threshold",
            "Some",
            "amount"
        ));
        assert!(!body_contains_direct_local_alias(
            "let amount = raw_amount_extra;",
            "amount",
            "raw_amount"
        ));
        assert!(starts_with_identifier_token(
            " raw_amount.required_discriminator()",
            "raw_amount"
        ));
        assert!(!starts_with_identifier_token(
            " raw_amount_extra.required_discriminator()",
            "raw_amount"
        ));
    }

    #[test]
    fn activation_evidence_uses_exact_if_let_parameter_name_for_boundary_operand_alias() {
        let owner = function(
            "pub fn score(raw_amount: Option<i32>, raw_amount_extra: Option<i32>, threshold: i32) -> bool {\n    if let Some(amount) = raw_amount_extra { amount >= threshold } else { false }\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(Some(100), Some(101), 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        assert!(!has_observed_boundary_equality(&activation));
        assert_eq!(activation.missing_discriminators.len(), 1);
        assert!(
            activation.missing_discriminators[0]
                .reason
                .contains("observed amount values: 101"),
            "prefix parameter matches must not make raw_amount look like amount; got {:?}",
            activation.missing_discriminators
        );
    }

    #[test]
    fn activation_evidence_ignores_commented_match_boundary_operand_alias() {
        let owner = function(
            "pub fn score(raw_amount: Option<i32>, threshold: i32) -> bool {\n    // match raw_amount { Some(amount) => amount >= threshold, _ => false }\n    let amount = 1;\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(Some(100), 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let gathered = boundary_input(&owner, &probe, &[&test]);

        // The operand stays unmapped (no alias to `raw_amount`), so the
        // boundary is unresolved rather than a missing input (#6674).
        assert!(!has_observed_boundary_equality(&gathered.activation));
        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "an unmapped operand is not a missing input; got {:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `amount`"),
            "boundary operands must stay unresolved; got {reason:?}"
        );
    }

    #[test]
    fn activation_evidence_ignores_inline_commented_match_boundary_operand_alias() {
        let owner = function(
            "pub fn score(raw_amount: Option<i32>, threshold: i32) -> bool {\n    let _note = 0; // match raw_amount { Some(amount) => amount >= threshold, _ => false }\n    let amount = 1;\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(Some(100), 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let gathered = boundary_input(&owner, &probe, &[&test]);

        // The operand stays unmapped (no alias to `raw_amount`), so the
        // boundary is unresolved rather than a missing input (#6674).
        assert!(!has_observed_boundary_equality(&gathered.activation));
        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "an unmapped operand is not a missing input; got {:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `amount`"),
            "boundary operands must stay unresolved; got {reason:?}"
        );
    }

    #[test]
    fn an_opaque_macro_reaches_the_owner_however_its_bang_is_spaced() {
        let unread = |body: &str| {
            may_reach_owner_unread(
                &test_with_body_call(body, 11, "reason(Some(5))"),
                "reason",
                &std::collections::BTreeMap::new(),
            )
        };
        for spelling in [
            "exercise!()",
            "exercise! ()",
            "exercise !()",
            "exercise ! [x]",
        ] {
            assert!(
                unread(&format!(
                    "fn t() {{\n    assert_eq!(reason(Some(5)), 6);\n    {spelling};\n}}\n"
                )),
                "{spelling}"
            );
        }
        for read in [
            "assert_eq! (reason(Some(5)), 6);",
            "assert!(!(reason(Some(5)) == 0));",
            "if !(reason(Some(5)) == 0) { return; }",
            "assert!(reason(Some(5)) != (0));",
        ] {
            assert!(!unread(&format!("fn t() {{\n    {read}\n}}\n")), "{read}");
        }
    }

    fn test_with_body_call(body: &str, call_line: usize, call: &str) -> TestSummary {
        TestSummary {
            name: "score_boundary".to_string(),
            file: PathBuf::from("tests/score.rs"),
            start_line: 10,
            end_line: 10 + body.lines().count(),
            body: body.into(),
            calls: vec![CallFact {
                name: "score".to_string(),
                line: call_line,
                text: call.to_string(),
            }],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn boundary_activation(test: &TestSummary) -> ActivationEvidence {
        let owner = function("pub fn score(amount: i32) -> bool {\n    amount > 10\n}");
        activation_evidence(
            &probe(ProbeFamily::Predicate, "amount > 10"),
            Some(&owner),
            &[test],
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        )
    }

    #[test]
    fn let_bound_owner_argument_is_an_owner_input() {
        // `let amount = 10; score(amount)`: the let-bound literal flows
        // into the owner, so it is an input row and reaches the boundary.
        let test = test_with_body_call(
            "fn score_boundary() {\n    let amount = 10;\n    assert!(score(amount));\n}",
            12,
            "assert!(score(amount));",
        );
        let activation = boundary_activation(&test);

        assert!(has_observed_boundary_equality(&activation));
        assert!(activation.missing_discriminators.is_empty());
        assert_eq!(owner_input_values(&activation), vec!["10"]);
    }

    #[test]
    fn owner_call_outside_the_test_span_binds_no_test_value() {
        // A credited helper's `score(amount)` (#4574) sits on the helper's
        // line; the test's `let amount = 10` binds a different variable.
        let mut test = test_with_body_call(
            "fn score_boundary() {\n    let amount = 10;\n    check(amount + 5);\n}",
            3,
            "score(amount)",
        );
        test.calls[0].line = 3;
        let activation = boundary_activation(&test);

        assert!(owner_input_values(&activation).is_empty());
        assert!(!has_observed_boundary_equality(&activation));
    }

    #[test]
    fn rstest_case_parameter_owner_argument_is_an_owner_input() {
        // `#[case(10, false)] fn t(#[case] amount: i32, ..) { score(amount) }`:
        // each case row's value flows into the owner (#4601).
        let mut test = test_with_body_call(
            "fn score_boundary(#[case] amount: i32, #[case] expected: bool) {\n    assert_eq!(score(amount), expected);\n}",
            11,
            "assert_eq!(score(amount), expected);",
        );
        test.attrs = vec![
            "#[rstest]".to_string(),
            "#[case(10, false)]".to_string(),
            "#[case(11, true)]".to_string(),
        ];
        let activation = boundary_activation(&test);

        assert_eq!(owner_input_values(&activation), vec!["10", "11"]);
        assert!(has_observed_boundary_equality(&activation));
    }

    #[test]
    fn rstest_case_parameter_fails_closed_on_mut_or_let_rebinding() {
        for body in [
            "fn score_boundary(#[case] mut amount: i32) {\n    amount += 1;\n    assert!(score(amount));\n}",
            "fn score_boundary(#[case] amount: i32) {\n    let amount = amount + 1;\n    assert!(score(amount));\n}",
            "fn score_boundary(#[case] amount: i32) {\n    let (amount, _) = (amount + 5, 0);\n    assert!(score(amount));\n}",
            "fn score_boundary(#[case] amount: i32) {\n    for amount in [amount + 5] {\n        assert!(score(amount));\n    }\n}",
            "fn score_boundary(#[case] amount: i32) {\n    if let Some(amount) = Some(amount + 5) {\n        assert!(score(amount));\n    }\n}",
            "fn score_boundary(#[case] amount: i32) {\n    let run = |amount: i32| score(amount);\n    assert!(run(amount + 5));\n    assert!(score(amount));\n}",
            "fn score_boundary(#[case] amount: i32) {\n    fn far(amount: i32) -> bool { score(amount) }\n    assert!(far(amount + 100));\n}",
        ] {
            let mut test = test_with_body_call(body, 12, "assert!(score(amount));");
            test.attrs = vec!["#[rstest]".to_string(), "#[case(10)]".to_string()];
            if body.contains("fn far") {
                test.nested_fn_names = vec!["far".to_string()];
            }
            let activation = boundary_activation(&test);

            assert!(
                owner_input_values(&activation).is_empty(),
                "`{body}` must not bind a case value to the owner"
            );
        }
    }

    #[test]
    fn let_bound_owner_argument_fails_closed_on_mut_shadowed_or_computed_bindings() {
        for body in [
            "fn score_boundary() {\n    let mut amount = 10;\n    assert!(score(amount));\n}",
            "fn score_boundary() {\n    let amount = 10;\n    let amount = amount + 1;\n    assert!(score(amount));\n}",
            "fn score_boundary() {\n    let amount = base() + 10;\n    assert!(score(amount));\n}",
            "fn score_boundary() {\n    let amount = \"10\";\n    assert!(score(amount));\n}",
        ] {
            let call_line = 10
                + body
                    .lines()
                    .position(|line| line.contains("score(amount)"))
                    .unwrap_or_default();
            let test = test_with_body_call(body, call_line, "assert!(score(amount));");
            let activation = boundary_activation(&test);

            assert!(
                !has_observed_boundary_equality(&activation),
                "`{body}` must not bind an exact owner input"
            );
            assert!(
                owner_input_values(&activation).is_empty(),
                "`{body}` must not yield an owner input; got {:?}",
                activation.observed_values
            );
        }
    }

    #[test]
    fn owner_input_values_exclude_assertion_expected_values() {
        // The owner call's argument is an input; the expected value of an
        // assertion on another function is an oracle value.
        let mut test = test_with_call("score_boundary", "assert!(score(5));");
        test.body = "assert!(score(5));\nassert_eq!(tax_bps(\"EU\"), 10);".into();
        test.assertions = vec![oracle_fact(
            "assert_eq!(tax_bps(\"EU\"), 10);",
            OracleKind::ExactValue,
        )];
        let activation = boundary_activation(&test);

        assert_eq!(owner_input_values(&activation), vec!["5"]);
        assert!(
            activation.observed_values.iter().any(|fact| {
                fact.value == "10" && fact.context == ValueContext::AssertionArgument
            })
        );
    }

    #[test]
    fn activation_evidence_ignores_commented_match_wrapper_pattern() {
        let owner = function(
            "pub fn score(raw_amount: Option<i32>, threshold: i32) -> bool {\n    let _seen = match raw_amount { _ => false };\n    // Some(amount)\n    let amount = 1;\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(Some(100), 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let gathered = boundary_input(&owner, &probe, &[&test]);

        // The operand stays unmapped (no alias to `raw_amount`), so the
        // boundary is unresolved rather than a missing input (#6674).
        assert!(!has_observed_boundary_equality(&gathered.activation));
        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "an unmapped operand is not a missing input; got {:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `amount`"),
            "boundary operands must stay unresolved; got {reason:?}"
        );
    }

    #[test]
    fn activation_evidence_ignores_inline_commented_match_wrapper_pattern() {
        let owner = function(
            "pub fn score(raw_amount: Option<i32>, threshold: i32) -> bool {\n    let _seen = match raw_amount { _ => false }; // Some(amount)\n    let amount = 1;\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(Some(100), 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let gathered = boundary_input(&owner, &probe, &[&test]);

        // The operand stays unmapped (no alias to `raw_amount`), so the
        // boundary is unresolved rather than a missing input (#6674).
        assert!(!has_observed_boundary_equality(&gathered.activation));
        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "an unmapped operand is not a missing input; got {:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `amount`"),
            "boundary operands must stay unresolved; got {reason:?}"
        );
    }

    #[test]
    fn activation_evidence_keeps_computed_local_boundary_operand_unresolved() {
        let owner = function(
            "pub fn score(raw_amount: i32, threshold: i32) -> bool {\n    let amount = raw_amount + 1;\n    amount >= threshold\n}",
        );
        let test = test_with_call("score_uses_boundary", "score(100, 100);");
        let probe = probe(ProbeFamily::Predicate, "amount >= threshold");

        let gathered = boundary_input(&owner, &probe, &[&test]);

        // The operand stays unmapped (no alias to `raw_amount`), so the
        // boundary is unresolved rather than a missing input (#6674).
        assert!(!has_observed_boundary_equality(&gathered.activation));
        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "an unmapped operand is not a missing input; got {:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `amount`"),
            "boundary operands must stay unresolved; got {reason:?}"
        );
    }

    fn constant_boundary_activation(
        constant_source: &str,
        calls: &[&str],
    ) -> (ActivationEvidence, Vec<TestSummary>) {
        constant_boundary_activation_with_test_file(constant_source, "use app::score;", calls)
    }

    fn constant_boundary_activation_with_test_file(
        constant_source: &str,
        test_file_source: &str,
        calls: &[&str],
    ) -> (ActivationEvidence, Vec<TestSummary>) {
        let owner = function("pub fn score(amount: i32) -> bool {\n    amount > LIMIT\n}");
        let mut index = crate::analysis::rust_index::RustIndex::default();
        index.insert_file_only(
            PathBuf::from("src/lib.rs"),
            crate::analysis::facts::FileFacts {
                path: PathBuf::from("src/lib.rs"),
                source: format!("{constant_source}\n{}", owner.body).into(),
                ..Default::default()
            },
        );
        index.insert_file_only(
            PathBuf::from("tests/score.rs"),
            crate::analysis::facts::FileFacts {
                path: PathBuf::from("tests/score.rs"),
                source: test_file_source.into(),
                ..Default::default()
            },
        );
        let tests = calls
            .iter()
            .map(|call| test_with_call("score_boundary", call))
            .collect::<Vec<_>>();
        let related = tests.iter().collect::<Vec<_>>();
        let activation = activation_evidence(
            &probe(ProbeFamily::Predicate, "amount > LIMIT"),
            Some(&owner),
            &related,
            &[],
            None,
            &index,
            false,
        );
        (activation, tests)
    }

    #[test]
    fn same_file_constant_boundary_is_observed_at_its_literal_value() {
        let (activation, _) =
            constant_boundary_activation("const LIMIT: i32 = 10;", &["score(5);", "score(10);"]);
        assert!(has_observed_boundary_equality(&activation));
        assert!(activation.missing_discriminators.is_empty());

        let (off_boundary, _) =
            constant_boundary_activation("const LIMIT: i32 = 10;", &["score(5);", "score(11);"]);
        assert!(!has_observed_boundary_equality(&off_boundary));
        assert_eq!(off_boundary.missing_discriminators.len(), 1);
        assert_eq!(
            off_boundary.missing_discriminators[0].value,
            "amount == LIMIT"
        );
        assert!(
            off_boundary.missing_discriminators[0]
                .reason
                .ends_with("target LIMIT value: 10"),
            "{:?}",
            off_boundary.missing_discriminators
        );
    }

    #[test]
    fn given_constant_minus_offset_boundary_then_boundary_is_the_offset_value_not_the_offset() {
        // #6671: `LIMIT - 2` with `const LIMIT: i32 = 10;` compares against
        // 8, never the literal 2, and naming LIMIT itself is not the
        // boundary by identity once an offset applies.
        let owner = function("pub fn score(amount: i32) -> bool {\n    amount > LIMIT - 2\n}");
        let mut index = crate::analysis::rust_index::RustIndex::default();
        index.insert_file_only(
            PathBuf::from("src/lib.rs"),
            crate::analysis::facts::FileFacts {
                path: PathBuf::from("src/lib.rs"),
                source: format!("const LIMIT: i32 = 10;\n{}", owner.body).into(),
                ..Default::default()
            },
        );
        let run = |calls: &[&str]| {
            let tests = calls
                .iter()
                .map(|call| test_with_call("score_boundary", call))
                .collect::<Vec<_>>();
            let related = tests.iter().collect::<Vec<_>>();
            activation_and_boundary_input(
                &probe(ProbeFamily::Predicate, "amount > LIMIT - 2"),
                Some(&owner),
                &related,
                &[],
                None,
                &index,
                false,
                None,
            )
        };

        let on_boundary = run(&["score(8);", "score(9);"]);
        assert!(has_observed_boundary_equality(&on_boundary.activation));
        assert!(on_boundary.activation.missing_discriminators.is_empty());

        for calls in [
            &["score(2);", "score(20);"][..],
            &["score(LIMIT);", "score(20);"][..],
        ] {
            let off = run(calls);
            assert!(
                !has_observed_boundary_equality(&off.activation),
                "{calls:?}"
            );
            assert_eq!(off.unresolved_boundary, None);
            assert_eq!(off.activation.missing_discriminators.len(), 1, "{calls:?}");
            assert!(
                off.activation.missing_discriminators[0]
                    .reason
                    .ends_with("target LIMIT - 2 value: 8"),
                "{:?}",
                off.activation.missing_discriminators
            );
        }
    }

    #[test]
    fn argument_naming_the_constant_is_the_boundary_by_identity() {
        let (activation, _) = constant_boundary_activation(
            "const LIMIT: i32 = 5 * 2;",
            &["score(5);", "score(crate::LIMIT);"],
        );
        assert!(has_observed_boundary_equality(&activation));
        assert!(activation.missing_discriminators.is_empty());

        // Opaque value and no argument naming it: the missing boundary is
        // honest about what ripr cannot see.
        let (opaque, _) =
            constant_boundary_activation("const LIMIT: i32 = 5 * 2;", &["score(5);", "score(10);"]);
        assert!(!has_observed_boundary_equality(&opaque));
        assert_eq!(opaque.missing_discriminators.len(), 1);
        assert!(
            opaque.missing_discriminators[0]
                .reason
                .contains("ripr cannot see the value of constant LIMIT statically"),
            "{:?}",
            opaque.missing_discriminators
        );
    }

    #[test]
    fn argument_naming_a_test_file_constant_of_the_same_name_is_not_the_boundary() {
        // `tests/score.rs` declares its own `LIMIT = 3`; `score(LIMIT)` there
        // passes 3, not the owner's boundary, so identity must not credit it.
        let (shadowed, _) = constant_boundary_activation_with_test_file(
            "const LIMIT: i32 = 5 * 2;",
            "use app::score;\nconst LIMIT: i32 = 3;\n",
            &["score(5);", "score(LIMIT);"],
        );
        assert!(!has_observed_boundary_equality(&shadowed));
        assert_eq!(shadowed.missing_discriminators.len(), 1);

        // The same call from a test file that only imports the owner's
        // constant is still the boundary by identity.
        let (imported, _) = constant_boundary_activation_with_test_file(
            "const LIMIT: i32 = 5 * 2;",
            "use app::{score, LIMIT};\n",
            &["score(5);", "score(LIMIT);"],
        );
        assert!(has_observed_boundary_equality(&imported));
        assert!(imported.missing_discriminators.is_empty());
    }

    #[test]
    fn constant_not_pinned_to_the_owner_file_fails_closed() {
        for constant_source in [
            "use crate::config::LIMIT;",
            "mod eu { pub const LIMIT: i32 = 10; }\nmod us { pub const LIMIT: i32 = 20; }",
        ] {
            let (activation, _) =
                constant_boundary_activation(constant_source, &["score(5);", "score(LIMIT);"]);
            assert!(
                !has_observed_boundary_equality(&activation),
                "`{constant_source}` must not credit the boundary"
            );
            assert!(
                activation.missing_discriminators.is_empty(),
                "`{constant_source}` must not name an unconfirmable repair: {:?}",
                activation.missing_discriminators
            );
        }
    }

    #[test]
    fn activation_evidence_reports_missing_boundary_discriminator() {
        let owner = function("pub fn score(amount: i32) -> bool {\n    amount > 10\n}");
        let test = test_with_call("score_uses_adjacent_value", "score(9);");
        let probe = probe(ProbeFamily::Predicate, "amount > 10");
        let flow_sinks = vec![FlowSinkFact {
            kind: FlowSinkKind::ReturnValue,
            text: "amount > 10".to_string(),
            line: 2,
            owner: None,
        }];

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &flow_sinks,
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        assert_eq!(activation.missing_discriminators.len(), 1);
        assert_eq!(activation.missing_discriminators[0].value, "amount == 10");
        assert!(
            activation.missing_discriminators[0]
                .reason
                .contains("observed amount values: 9")
        );
        assert_eq!(
            activation.missing_discriminators[0]
                .flow_sink
                .as_ref()
                .map(|sink| &sink.kind),
            Some(&FlowSinkKind::ReturnValue)
        );
    }

    #[test]
    fn activation_evidence_omits_missing_error_variant_when_exact_assertion_exists() {
        let test = test_with_assertion(
            "rejects_revoked",
            "assert_eq!(err, AuthError::RevokedToken);",
            OracleKind::ExactErrorVariant,
        );
        let probe = probe(
            ProbeFamily::ErrorPath,
            "return Err(AuthError::RevokedToken);",
        );
        let flow_sinks = vec![FlowSinkFact {
            kind: FlowSinkKind::ErrorVariant,
            text: "Result::Err(AuthError::RevokedToken)".to_string(),
            line: 2,
            owner: None,
        }];

        let activation = activation_evidence(
            &probe,
            None,
            &[&test],
            &flow_sinks,
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );

        assert!(activation.missing_discriminators.is_empty());
    }

    #[test]
    fn activation_evidence_sorts_multiple_missing_discriminators() {
        let owner = function(
            "pub fn score(amount: i32) -> Result<bool, AuthError> {\n    if amount > 10 { return Err(AuthError::Bad); }\n    Ok(true)\n}",
        );
        let test = test_with_call("score_uses_adjacent_value", "score(9);");
        let probe = probe(ProbeFamily::Predicate, "amount > 10");
        let flow_sinks = vec![FlowSinkFact {
            kind: FlowSinkKind::ErrorVariant,
            text: "Result::Err(AuthError::Bad)".to_string(),
            line: 2,
            owner: None,
        }];

        let activation = activation_evidence(
            &probe,
            Some(&owner),
            &[&test],
            &flow_sinks,
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
        );
        let values = activation
            .missing_discriminators
            .iter()
            .map(|fact| fact.value.as_str())
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["AuthError::Bad", "amount == 10"]);
    }

    #[test]
    fn value_facts_for_test_preserves_table_builder_and_assertion_contexts() {
        let test = TestSummary {
            name: "table_and_builder".to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 10,
            end_line: 16,
            body: r#"let rows = [(99, 100), (100, 100)];
let input = Request::builder().amount(100).token("abc").build();
assert_eq!(input.amount, 100);"#
                .into(),
            calls: Vec::new(),
            assertions: vec![oracle_fact(
                "assert_eq!(input.amount, 100);",
                OracleKind::ExactValue,
            )],
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };

        let facts = value_facts_for_test(&test, None);

        assert!(
            facts
                .iter()
                .any(|fact| fact.context == ValueContext::TableRow && fact.value == "99")
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact.context == ValueContext::BuilderMethod && fact.value == "100")
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact.context == ValueContext::BuilderMethod && fact.value == "\"abc\"")
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact.context == ValueContext::AssertionArgument && fact.value == "100")
        );
    }

    /// #5357: `crate::KIB` and `u64::MAX` were reported as "enum variant"
    /// values. The path's own spelling decides; real variants keep the
    /// label.
    #[test]
    fn value_facts_for_test_labels_path_constants_apart_from_enum_variants() {
        let assertion = "assert_eq!(size.as_whole_units(crate::KIB), u64::MAX, Mode::Fast);";
        let test = TestSummary {
            name: "constants".to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 10,
            end_line: 12,
            body: assertion.to_string().into(),
            calls: Vec::new(),
            assertions: vec![oracle_fact(assertion, OracleKind::ExactValue)],
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };

        let facts = value_facts_for_test(&test, None);
        let context_of = |value: &str| {
            facts
                .iter()
                .find(|fact| fact.value == value)
                .map(|fact| fact.context.clone())
        };

        assert_eq!(context_of("crate::KIB"), Some(ValueContext::Constant));
        assert_eq!(context_of("u64::MAX"), Some(ValueContext::Constant));
        assert_eq!(context_of("Mode::Fast"), Some(ValueContext::EnumVariant));
    }

    #[test]
    fn value_facts_for_test_filters_non_owner_calls_and_reads_enum_call_arguments() {
        let owner = function(
            "pub fn score(error: AuthError) -> Result<(), AuthError> {\n    Err(error)\n}",
        );
        let test = TestSummary {
            name: "enum_call".to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 10,
            end_line: 12,
            body: "other(AuthError::Ignored);\nscore(AuthError::RevokedToken);".into(),
            calls: vec![
                CallFact {
                    line: 11,
                    name: "other".to_string(),
                    text: "other(AuthError::Ignored);".to_string(),
                },
                CallFact {
                    line: 12,
                    name: "score".to_string(),
                    text: "score(AuthError::RevokedToken);".to_string(),
                },
                CallFact {
                    line: 13,
                    name: "score".to_string(),
                    text: "score;".to_string(),
                },
            ],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };

        let facts = value_facts_for_test(&test, Some(&owner));

        assert!(
            facts
                .iter()
                .any(|fact| fact.value == "AuthError::RevokedToken")
        );
        assert!(!facts.iter().any(|fact| fact.value == "AuthError::Ignored"));
    }

    /// The run-scoped memo answers every (test, owner) query exactly as a
    /// fresh `value_facts_for_test` does, on the first (computing) query and
    /// every later (cached) one; owners that differ only in their parameter
    /// names get different facts, and a test or index the memo is not bound
    /// to is computed fresh without being cached.
    #[test]
    fn test_value_facts_memo_matches_fresh_facts_for_every_test_and_owner() -> Result<(), String> {
        let call = |line: usize, text: &str| CallFact {
            line,
            name: "score".to_string(),
            text: text.to_string(),
        };
        let test = |name: &str, body: &str, calls: Vec<CallFact>| TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 10,
            end_line: 14,
            body: body.into(),
            calls,
            assertions: vec![oracle_fact(
                "assert_eq!(total, 100);",
                OracleKind::ExactValue,
            )],
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };
        let index = crate::analysis::rust_index::RustIndex::from_owned(
            crate::analysis::facts::OwnedRustIndex {
                tests: vec![
                    test(
                        "enum_call",
                        "score(AuthError::RevokedToken);",
                        vec![call(11, "score(AuthError::RevokedToken);")],
                    ),
                    test(
                        "literal_call",
                        "let rows = [(99, 100)];\nscore(7);",
                        vec![call(12, "score(7);")],
                    ),
                ],
                functions: vec![
                    function("pub fn score(error: AuthError) -> u32 {\n    0\n}"),
                    function("pub fn score(code: AuthError) -> u32 {\n    0\n}"),
                ],
                ..Default::default()
            },
        );
        let memo = TestValueFacts::default();
        let owners = [
            None,
            Some(index.functions().at(0)),
            Some(index.functions().at(1)),
        ];
        for round in 0..2 {
            for test in &index.tests() {
                for owner in owners {
                    assert_eq!(
                        memo.facts_for(&index, test, owner),
                        value_facts_for_test(test, owner),
                        "round {round}: {} / {:?}",
                        test.name,
                        owner.map(|owner| owner.body.as_str())
                    );
                }
            }
        }
        assert_ne!(
            memo.facts_for(&index, index.tests().at(1), owners[1]),
            memo.facts_for(&index, index.tests().at(1), owners[2]),
            "the owner's parameter names are part of the facts"
        );
        let cached = memo.by_slot.borrow().len();
        assert_eq!(cached, index.tests().len() * owners.len());

        let detached = index.tests()[0].clone();
        assert_eq!(
            memo.facts_for(&index, &detached, owners[1]),
            value_facts_for_test(&detached, owners[1])
        );
        let other_index = index.clone();
        assert_eq!(
            memo.facts_for(&other_index, other_index.tests().at(0), None),
            value_facts_for_test(other_index.tests().at(0), None)
        );
        assert_eq!(memo.by_slot.borrow().len(), cached);

        // Membership reordering can leave the arena allocations at the same
        // addresses. A surviving memo must compute uncached after the revision
        // changes rather than reinterpret its old flat ordinal keys.
        let mut reordered = index;
        let before = reordered.storage_identity();
        reordered.reverse_flat_membership()?;
        let after = reordered.storage_identity();
        assert_eq!((before.0, before.1), (after.0, after.1));
        assert_ne!(before.2, after.2);
        for test in reordered.tests() {
            for owner in reordered.functions() {
                assert_eq!(
                    memo.facts_for(&reordered, test, Some(owner)),
                    value_facts_for_test(test, Some(owner))
                );
            }
        }
        assert_eq!(memo.by_slot.borrow().len(), cached);
        Ok(())
    }

    // #4228: a reversed literal (`100 < amount`) and a local boundary
    // (`let limit = 100;`) close at the boundary input and stay open one
    // step off it; a local no row can evaluate names no repair at all.
    #[test]
    fn reversed_and_local_boundaries_close_only_at_the_boundary_value() {
        enum Expect {
            Closed,
            Missing(&'static str),
            NoRepair,
        }
        let cases = [
            (
                "    100 < amount",
                "100 < amount",
                2,
                "score(100);",
                Expect::Closed,
            ),
            (
                "    100 < amount",
                "100 < amount",
                2,
                "score(101);",
                Expect::Missing("amount == 100"),
            ),
            (
                "    100 <= amount",
                "100 <= amount",
                2,
                "score(100);",
                Expect::Closed,
            ),
            (
                "    100 <= amount",
                "100 <= amount",
                2,
                "score(99);",
                Expect::Missing("amount == 100"),
            ),
            (
                "    -100 < amount",
                "-100 < amount",
                2,
                "score(-100);",
                Expect::Closed,
            ),
            (
                "    -100 < amount",
                "-100 < amount",
                2,
                "score(-99);",
                Expect::Missing("amount == -100"),
            ),
            (
                "    let limit = 100;\n    amount > limit",
                "amount > limit",
                3,
                "score(100);",
                Expect::Closed,
            ),
            (
                "    let limit = 100;\n    amount > limit",
                "amount > limit",
                3,
                "score(101);",
                Expect::Missing("amount == limit"),
            ),
            (
                "    let limit = 100;\n    amount >= limit",
                "amount >= limit",
                3,
                "score(100);",
                Expect::Closed,
            ),
            (
                "    let limit = 100;\n    limit < amount",
                "limit < amount",
                3,
                "score(100);",
                Expect::Closed,
            ),
            (
                "    let limit = 100;\n    limit < amount",
                "limit < amount",
                3,
                "score(101);",
                Expect::Missing("limit == amount"),
            ),
            // #4271: a decimal boundary and a decimal test argument are
            // read whole, so `1` does not hit the boundary `1.5`.
            (
                "    amount > 1.5",
                "amount > 1.5",
                2,
                "score(1.5);",
                Expect::Closed,
            ),
            (
                "    amount > 1.5",
                "amount > 1.5",
                2,
                "score(1.0);",
                Expect::Missing("amount == 1.5"),
            ),
            (
                "    1.5 < amount",
                "1.5 < amount",
                2,
                "score(1.2);",
                Expect::Missing("amount == 1.5"),
            ),
            (
                "    amount > 1.5f64",
                "amount > 1.5f64",
                2,
                "score(1.0);",
                Expect::Missing("amount == 1.5f64"),
            ),
            // A type suffix is not a second literal: `9.5f64` is 9.5, not
            // 64, and `99u32` is 99, not 32.
            (
                "    amount > 9.5f64",
                "amount > 9.5f64",
                2,
                "score(64.0);",
                Expect::Missing("amount == 9.5f64"),
            ),
            (
                "    amount > 99u32",
                "amount > 99u32",
                2,
                "score(32);",
                Expect::Missing("amount == 99u32"),
            ),
            (
                "    amount > 99u32",
                "amount > 99u32",
                2,
                "score(99);",
                Expect::Closed,
            ),
            // The evaluator cannot fold these initializers, so a test at
            // 100 could never close them: no repair is named.
            (
                "    let limit = 100; amount > limit",
                "amount > limit",
                2,
                "score(100);",
                Expect::NoRepair,
            ),
            (
                "    let limit = amount / 2 + 50;\n    amount > limit",
                "amount > limit",
                3,
                "score(100);",
                Expect::NoRepair,
            ),
        ];
        for (body, predicate, line, call, expect) in cases {
            // Decimal cases compare a real `f64` input.
            let ty = if predicate.contains('.') {
                "f64"
            } else {
                "i32"
            };
            let owner = function(&format!(
                "pub fn score(amount: {ty}) -> bool {{\n{body}\n}}"
            ));
            let test = test_with_call("score_boundary", call);
            let mut probe = probe(ProbeFamily::Predicate, predicate);
            probe.location = SourceLocation::new("src/lib.rs", line, 5);
            let activation = activation_evidence(
                &probe,
                Some(&owner),
                &[&test],
                &[],
                None,
                &crate::analysis::rust_index::RustIndex::default(),
                false,
            );
            let missing: Vec<&str> = activation
                .missing_discriminators
                .iter()
                .map(|fact| fact.value.as_str())
                .collect();
            match expect {
                Expect::Closed => assert!(
                    has_observed_boundary_equality(&activation) && missing.is_empty(),
                    "`{predicate}` with {call} must close; missing {missing:?}"
                ),
                Expect::Missing(value) => assert!(
                    !has_observed_boundary_equality(&activation) && missing == [value],
                    "`{predicate}` with {call} must name {value}; missing {missing:?}"
                ),
                Expect::NoRepair => assert!(
                    !has_observed_boundary_equality(&activation) && missing.is_empty(),
                    "`{predicate}` ({body}) with {call} must not credit or name a repair; missing {missing:?}"
                ),
            }
        }
    }

    // #4270: a commented `match` alias does not bind `amount`; the live
    // `let amount = 1;` does. `check` compares `threshold` against 1, the
    // same contract grip holds (`test_grip_evidence` commented-alias
    // tests): `raw_amount == threshold` never closes, `threshold == 1` does.
    #[test]
    fn commented_alias_leaves_the_local_boundary_in_charge() {
        let bodies = [
            "    // match raw_amount { Some(amount) => if amount >= threshold { amount - 10 } else { amount }, _ => 0 }\n    let amount = 1;\n    if amount >= threshold { amount - 10 } else { amount }",
            "    let _note = 0; // match raw_amount { Some(amount) => if amount >= threshold { amount - 10 } else { amount }, _ => 0 }\n    let amount = 1;\n    if amount >= threshold { amount - 10 } else { amount }",
            "    let _seen = match raw_amount { _ => false };\n    // Some(amount)\n    let amount = 1;\n    if amount >= threshold { amount - 10 } else { amount }",
            "    let _seen = match raw_amount { _ => false }; // Some(amount)\n    let amount = 1;\n    if amount >= threshold { amount - 10 } else { amount }",
        ];
        for body in bodies {
            let mut owner = function(&format!(
                "pub fn score(raw_amount: Option<i32>, threshold: i32) -> i32 {{\n{body}\n}}"
            ));
            owner.end_line = owner.body.lines().count();
            // The predicate is the body's last line, after the signature.
            let line = 1 + body.lines().count();
            let mut probe = probe(ProbeFamily::Predicate, "amount >= threshold");
            probe.location = SourceLocation::new("src/lib.rs", line, 5);
            for (call, closes) in [
                ("score(Some(50), 50);", false),
                ("score(Some(50), 1);", true),
            ] {
                let test = test_with_call("score_boundary", call);
                let activation = activation_evidence(
                    &probe,
                    Some(&owner),
                    &[&test],
                    &[],
                    None,
                    &crate::analysis::rust_index::RustIndex::default(),
                    false,
                );
                let missing: Vec<&str> = activation
                    .missing_discriminators
                    .iter()
                    .map(|fact| fact.value.as_str())
                    .collect();
                if closes {
                    assert!(
                        has_observed_boundary_equality(&activation) && missing.is_empty(),
                        "{call} hits the local boundary 1 ({body:?}); missing {missing:?}"
                    );
                } else {
                    assert!(
                        !has_observed_boundary_equality(&activation)
                            && missing == ["amount == threshold"],
                        "{call} must leave the local boundary open ({body:?}); missing {missing:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn missing_boundary_handles_missing_left_and_nonliteral_target() {
        let owner = function("pub fn score(amount: i32) -> bool {\n    amount > 10\n}");
        let test = test_with_call("score_uses_other_value", "score(9);");
        let probe = probe(ProbeFamily::Predicate, "threshold > limit");

        let gathered = boundary_input(&owner, &probe, &[&test]);

        // Neither operand maps to an input row, so no row says which side
        // of the boundary the test reaches: unresolved, not a missing
        // `threshold == limit` input (CodeRabbit review on #6796).
        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "{:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operands `threshold` and `limit`"),
            "got {reason:?}"
        );
    }

    #[test]
    fn owner_call_parameter_values_handles_empty_inputs_and_skips_other_calls() {
        let test = TestSummary {
            name: "mixed_calls".to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 10,
            end_line: 12,
            body: "other(1);\nscore(2);".into(),
            calls: vec![
                CallFact {
                    line: 11,
                    name: "other".to_string(),
                    text: "other(1);".to_string(),
                },
                CallFact {
                    line: 12,
                    name: "score".to_string(),
                    text: "score(2);".to_string(),
                },
                CallFact {
                    line: 13,
                    name: "score".to_string(),
                    text: "score;".to_string(),
                },
            ],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };

        assert!(owner_call_parameter_values(&[&test], "", &["amount".to_string()]).is_empty());
        assert!(owner_call_parameter_values(&[&test], "score", &[]).is_empty());

        let rows = owner_call_parameter_values(&[&test], "score", &["amount".to_string()]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].value, "2");
    }

    #[test]
    fn text_helpers_handle_braces_escapes_negative_numbers_and_dedup_contexts() {
        let owner = function("pub fn score(amount: i32) -> bool {\n    amount > 10\n}");
        let non_comparison_probe = probe(ProbeFamily::ReturnValue, "amount");
        assert!(
            observed_discriminator_values(
                &non_comparison_probe,
                Some(&owner),
                &[],
                None,
                &crate::analysis::rust_index::RustIndex::default(),
                false,
            )
            .is_empty()
        );
        assert!(
            observed_discriminator_values(
                &probe(ProbeFamily::Predicate, "amount > 10"),
                None,
                &[],
                None,
                &crate::analysis::rust_index::RustIndex::default(),
                false,
            )
            .is_empty()
        );
        assert_eq!(
            comparison_operands("if amount > 10 {"),
            Some(("amount".to_string(), "10".to_string()))
        );
        assert_eq!(comparison_operands("> 10"), None);
        assert_eq!(
            call_arguments(r#"score("a\",b", -12)"#, "score"),
            Some(vec![r#""a\",b""#.to_string(), "-12".to_string()])
        );
        assert_eq!(
            scalar_values(r#""a\"b" -12"#),
            vec!["\"a\\\"b\"".to_string(), "-12".to_string()]
        );
        // #4271: a fraction is part of the literal; a range or a method
        // call on an integer is not.
        assert_eq!(
            scalar_values("f(1.5, -0.25, 1_000.5)"),
            vec![
                "-0.25".to_string(),
                "1.5".to_string(),
                "1_000.5".to_string()
            ]
        );
        assert_eq!(
            scalar_values("f(99u32, 9.5f64, x1, 100_u8)"),
            vec!["100_".to_string(), "9.5".to_string(), "99".to_string()]
        );
        assert_eq!(
            scalar_values("f(0..5, 2.max(3), 1.2.3)"),
            vec![
                "0".to_string(),
                "1.2".to_string(),
                "2".to_string(),
                "3".to_string(),
                "5".to_string()
            ]
        );

        let mut facts = vec![
            value_fact(1, "score(1)", "1", ValueContext::FunctionArgument),
            value_fact(1, "score(1)", "1", ValueContext::AssertionArgument),
        ];

        sort_value_facts(&mut facts);

        assert_eq!(facts.len(), 2);
    }

    fn probe(family: ProbeFamily, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:src/lib.rs:2:score".to_string()),
            location: SourceLocation::new("src/lib.rs", 2, 5),
            owner: None,
            family,
            delta: DeltaKind::Control,
            before: None,
            after: Some(expression.to_string()),
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn function(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::score".to_string()),
            name: "score".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: body.into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn test_with_call(name: &str, call: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/score.rs"),
            start_line: 10,
            end_line: 12,
            body: call.into(),
            calls: vec![CallFact {
                name: "score".to_string(),
                line: 11,
                text: call.to_string(),
            }],
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn test_with_assertion(name: &str, assertion: &str, kind: OracleKind) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/score.rs"),
            start_line: 10,
            end_line: 12,
            body: assertion.into(),
            calls: Vec::new(),
            assertions: vec![oracle_fact(assertion, kind)],
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn oracle_fact(assertion: &str, kind: OracleKind) -> OracleFact {
        OracleFact {
            kind,
            strength: OracleStrength::Strong,
            line: 11,
            text: assertion.to_string(),
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        }
    }

    fn value_fact(line: usize, text: &str, value: &str, context: ValueContext) -> ValueFact {
        ValueFact {
            line,
            text: text.to_string(),
            value: value.to_string(),
            context,
        }
    }

    fn boundary_input(
        owner: &FunctionSummary,
        probe: &Probe,
        tests: &[&TestSummary],
    ) -> ActivationWithBoundaryInput {
        activation_and_boundary_input(
            probe,
            Some(owner),
            tests,
            &[],
            None,
            &crate::analysis::rust_index::RustIndex::default(),
            false,
            None,
        )
    }

    fn test_with_body_calls(body: &str, calls: &[(usize, &str)]) -> TestSummary {
        TestSummary {
            name: "boundary_inputs".to_string(),
            file: PathBuf::from("tests/score.rs"),
            start_line: 10,
            end_line: 10 + body.lines().count(),
            body: body.to_string().into(),
            calls: calls
                .iter()
                .map(|(line, text)| CallFact {
                    name: "score".to_string(),
                    line: *line,
                    text: (*text).to_string(),
                })
                .collect(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    #[test]
    fn computed_value_expressions_are_told_apart_from_spelled_values() {
        for computed in [
            "base + 1",
            "CURRENT - 2",
            "2 + 2",
            "n * 2",
            "x / 2",
            "x % 2",
            "flags | 1",
            "flags & 1",
            "flags ^ 1",
            "s.len() - 1",
            "Some(base + 1)",
            "&[b'f'; 16]",
            "[0u8; 4]",
            // A literal suffix or hex digit `e`/`E` is not a float exponent.
            "16usize - 1",
            "16usize-1",
            "2isize + 1",
            "0xFE - 1",
            "0x1E-1",
            "x? + 1",
            "parse(s)? - 1",
            "{ x } + 1",
            "1 << 4",
            "x >> 1",
        ] {
            assert!(
                is_computed_value_expression(computed),
                "`{computed}` computes its value"
            );
        }
        for spelled in [
            "10",
            "-5",
            "f(x, -1)",
            "Some(-5)",
            "&x",
            "*x",
            "\"a + b\"",
            "'+'",
            "b'-'",
            "1e-5",
            "1.5E+3",
            "a && b",
            "a || b",
            "[1, 2, 3]",
            "Vec::<u8>::new()",
            "base",
        ] {
            assert!(
                !is_computed_value_expression(spelled),
                "`{spelled}` spells its value"
            );
        }
    }

    #[test]
    fn computed_owner_argument_yields_no_exact_input_value() {
        // #6672: `score(base + 1)` passes neither `1` nor `base`; an
        // array-repeat length (`[b'f'; 16]`) is not an element value.
        let test = test_with_body_calls("let base = 9;", &[]);
        assert!(owner_argument_values(&test, "base + 1").is_empty());
        assert!(owner_argument_values(&test, "&[b'f'; 16]").is_empty());
        assert_eq!(owner_argument_values(&test, "base"), vec!["9"]);
        assert_eq!(owner_argument_values(&test, "Some(10)"), vec!["10"]);
        assert_eq!(owner_argument_values(&test, "-5"), vec!["-5"]);
        assert_eq!(literal_operand_value("2 + 2"), None);
        assert_eq!(literal_operand_value("CURRENT - 2"), None);
        assert_eq!(literal_operand_value("10"), Some("10".to_string()));
    }

    #[test]
    fn given_computed_argument_for_compared_parameter_then_boundary_is_unresolved_not_missing() {
        // #6672: `score(base + 1)` with `let base = 9` is 10, the boundary.
        // ripr cannot read it, so it must not report `amount == 10` as
        // missing from the remaining `score(base)` row.
        let owner = function("pub fn score(amount: u32) -> bool {\n    10 <= amount\n}");
        let test = test_with_body_calls(
            "let base = 9;\nassert!(score(base + 1));\nassert!(!score(base));",
            &[
                (11, "assert!(score(base + 1));"),
                (12, "assert!(!score(base));"),
            ],
        );
        let gathered = boundary_input(
            &owner,
            &probe(ProbeFamily::Predicate, "10 <= amount"),
            &[&test],
        );

        assert!(gathered.activation.missing_discriminators.is_empty());
        assert_eq!(owner_input_values(&gathered.activation), vec!["9"]);
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("computed argument for `amount`"),
            "got {reason:?}"
        );

        // A computation over a value ripr cannot bind (the loop variable
        // in `score(age + 1)`) is no more readable than the bare variable:
        // it is not a definite input, so the exact rows keep the missing
        // input (grid-boundary-property in the verdict corpus).
        let looped = test_with_body_calls(
            "for age in 0..120 {\n    if score(age) {\n        assert!(score(age + 1));\n    }\n}\nassert!(!score(0));\nassert!(score(119));",
            &[
                (11, "if score(age) {"),
                (12, "assert!(score(age + 1));"),
                (15, "assert!(!score(0));"),
                (16, "assert!(score(119));"),
            ],
        );
        let gathered = boundary_input(
            &owner,
            &probe(ProbeFamily::Predicate, "10 <= amount"),
            &[&looped],
        );
        assert_eq!(gathered.unresolved_boundary, None);
        assert_eq!(gathered.activation.missing_discriminators.len(), 1);
    }

    #[test]
    fn suffixed_or_shifted_arguments_are_computed_end_to_end() {
        // Review on #6796: `5usize - 1` once read as the literal 1 (the
        // `e` of `usize` passed for a float exponent), which credited the
        // boundary `1 <= amount` with an input the test never passes.
        let owner = function("pub fn score(amount: u32) -> bool {\n    1 <= amount\n}");
        let credit = test_with_body_calls(
            "assert!(score(5usize - 1));\nassert!(score(1 << 4));",
            &[
                (10, "assert!(score(5usize - 1));"),
                (11, "assert!(score(1 << 4));"),
            ],
        );
        let gathered = boundary_input(
            &owner,
            &probe(ProbeFamily::Predicate, "1 <= amount"),
            &[&credit],
        );
        assert!(!has_observed_boundary_equality(&gathered.activation));
        assert!(
            owner_input_values(&gathered.activation).is_empty(),
            "{:?}",
            gathered.activation.observed_values
        );
        assert!(
            gathered
                .unresolved_boundary
                .as_deref()
                .is_some_and(|reason| reason.contains("computed argument for `amount`")),
            "{:?}",
            gathered.unresolved_boundary
        );

        // The false-gap half: `16usize - 1` is 15, the boundary; it is a
        // definite unreadable input, so the boundary is unresolved rather
        // than missing beside the exact off-boundary row 20.
        let owner = function("pub fn score(amount: u32) -> bool {\n    15 <= amount\n}");
        let gap = test_with_body_calls(
            "assert!(score(16usize - 1));\nassert!(score(20));",
            &[
                (10, "assert!(score(16usize - 1));"),
                (11, "assert!(score(20));"),
            ],
        );
        let gathered = boundary_input(
            &owner,
            &probe(ProbeFamily::Predicate, "15 <= amount"),
            &[&gap],
        );
        assert!(gathered.activation.missing_discriminators.is_empty());
        assert!(gathered.unresolved_boundary.is_some());
    }

    #[test]
    fn table_and_builder_lines_skip_computed_arguments() {
        // Review on #6796: `.amount(base + 10)` passes neither 10 nor
        // `base`, and `[b'f'; 16]`'s 16 is a length.
        assert!(spelled_scalar_values(".amount(base + 10)").is_empty());
        assert!(spelled_scalar_values("(&[b'f'; 16], true),").contains(&"true".to_string()));
        assert!(!spelled_scalar_values("(&[b'f'; 16], true),").contains(&"16".to_string()));
        assert_eq!(
            spelled_scalar_values("(10, Some(-5), \"a, b\"),"),
            scalar_values("(10, Some(-5), \"a, b\"),")
        );
        assert_eq!(spelled_scalar_values(".amount(10)"), vec!["10".to_string()]);

        // Exact-head review on #6796: a char literal is one unit, never a
        // delimiter.
        let strings = |values: &[&str]| values.iter().map(|v| (*v).to_string()).collect::<Vec<_>>();
        assert_eq!(
            spelled_scalar_values("(',', true),"),
            strings(&["','", "true"])
        );
        assert_eq!(
            spelled_scalar_values("('(', Token::LParen),"),
            strings(&["'('"])
        );
        assert_eq!(spelled_scalar_values("('{', 1),"), strings(&["'{'", "1"]));
        assert_eq!(
            spelled_scalar_values("(b')', '\\''),"),
            // `scalar_values` reads the byte literal `b')'` as its char.
            strings(&["')'", "'\\''"])
        );

        // A binary operator after a group computes the argument: none of
        // its literals is a passed value.
        for computed in [
            ".amount(base.len() + 10)",
            ".amount(f(x) - 1)",
            ".amount((a + 1) * 2)",
            ".amount(parse(s)? - 1)",
            ".amount({ x } + 1)",
        ] {
            assert!(spelled_scalar_values(computed).is_empty(), "{computed}");
        }
        assert_eq!(spelled_scalar_values("(x.len() - 1, 9),"), strings(&["9"]));
        assert_eq!(
            spelled_scalar_values("(Some(base) + 1, 7),"),
            strings(&["7"])
        );
        assert_eq!(
            spelled_scalar_values("Case { input: 5, expected: Some(-3) },"),
            strings(&["-3", "5"])
        );
        assert_eq!(
            spelled_scalar_values("[(1, true), (2, false)],"),
            strings(&["1", "2", "false", "true"])
        );

        // Devin review on #6796: a group read through a call computes the
        // argument; only constructors and bare tuples/arrays pass through.
        for computed in [
            ".amount(x.min(10))",
            ".amount(base.saturating_add(10))",
            ".amount(parse(10))",
            ".amount(format!(\"{}\", 10))",
            ".amount(rows[10])",
            ".amount({ 10 })",
        ] {
            assert!(spelled_scalar_values(computed).is_empty(), "{computed}");
        }
        assert_eq!(spelled_scalar_values("(10.min(x), 3),"), strings(&["3"]));
        assert_eq!(spelled_scalar_values(".amount(Some(10))"), strings(&["10"]));
        assert_eq!(
            spelled_scalar_values("(10, true),"),
            strings(&["10", "true"])
        );
        assert_eq!(
            spelled_scalar_values("(Case { n: 1 }, 2),"),
            strings(&["1", "2"])
        );
        assert_eq!(
            spelled_scalar_values("(Ok(4), Token::Num(5)),"),
            strings(&["4", "5"])
        );
        assert_eq!(
            spelled_scalar_values(".with_items(vec![6, 7])"),
            strings(&["6", "7"])
        );
    }

    #[test]
    fn deeply_nested_table_line_is_unreadable_not_a_crash() {
        // Exact-head review on #6796: the recursive split once had no depth
        // bound, so one hostile line overflowed the stack.
        let line = format!("{}{}", "(1, ".repeat(5000), ")".repeat(5000));
        assert!(spelled_line_is_unreadable(&line));
        assert!(spelled_scalar_values(&line).is_empty());
        let shallow = format!(
            "{}{}",
            "(1, ".repeat(MAX_SPELLED_DEPTH),
            ")".repeat(MAX_SPELLED_DEPTH)
        );
        assert!(!spelled_line_is_unreadable(&shallow));
        assert_eq!(spelled_scalar_values(&shallow), vec!["1".to_string()]);

        // A test that feeds the owner from its rows (`score(n)`) cannot be
        // read past the deep line, so the boundary is unresolved.
        let owner = function("pub fn score(amount: u32) -> bool {\n    amount > 10\n}");
        let gap = probe(ProbeFamily::Predicate, "amount > 10");
        let body =
            format!("let rows = [\n{line},\n];\nfor (n, _) in rows {{ assert!(score(n)); }}");
        let test =
            test_with_body_calls(&body, &[(13, "for (n, _) in rows { assert!(score(n)); }")]);
        let gathered = boundary_input(&owner, &gap, &[&test]);
        assert!(gathered.activation.missing_discriminators.is_empty());
        assert!(
            gathered
                .unresolved_boundary
                .as_deref()
                .is_some_and(|reason| reason.contains("nests deeper than")),
            "{:?}",
            gathered.unresolved_boundary
        );

        // Devin review on #6796: a deep row feeding something else does not
        // hide exact owner calls. `score(10)` stays credited ...
        let body = format!("let rows = [\n{line},\n];\nother(rows);\nassert!(score(10));");
        let test = test_with_body_calls(&body, &[(14, "assert!(score(10));")]);
        let gathered = boundary_input(&owner, &gap, &[&test]);
        assert!(
            gathered.unresolved_boundary.is_none(),
            "{:?}",
            gathered.unresolved_boundary
        );
        assert!(has_observed_boundary_equality(&gathered.activation));
        assert!(gathered.activation.missing_discriminators.is_empty());

        // ... and `score(20)` keeps its missing equality discriminator.
        let body = format!("let rows = [\n{line},\n];\nother(rows);\nassert!(score(20));");
        let test = test_with_body_calls(&body, &[(14, "assert!(score(20));")]);
        let gathered = boundary_input(&owner, &gap, &[&test]);
        assert!(
            gathered.unresolved_boundary.is_none(),
            "{:?}",
            gathered.unresolved_boundary
        );
        assert!(
            gathered
                .activation
                .missing_discriminators
                .iter()
                .any(|fact| fact.value.contains("==")),
            "{:?}",
            gathered.activation.missing_discriminators
        );
    }

    #[test]
    fn char_literal_table_rows_are_owner_inputs_end_to_end() {
        // Exact-head review on #6796: `(',', true)` once split inside the
        // char literal, lost `','`, and read the boundary as present only
        // outside the owner's inputs.
        let owner = function("pub fn is_comma(ch: char) -> bool {\n    ch == ','\n}");
        let body = "let rows = [\n(',', true),\n('x', false),\n];\nfor (ch, expected) in rows { assert_eq!(is_comma(ch), expected); }";
        let mut test = test_with_body_calls(body, &[]);
        test.literals = ["','", "'x'", "true", "false"]
            .iter()
            .map(|value| LiteralFact {
                line: 11,
                value: (*value).to_string(),
            })
            .collect();
        let probe = probe(ProbeFamily::Predicate, "ch == ','");
        let gathered = boundary_input(&owner, &probe, &[&test]);
        let infection = super::super::infection::infection_evidence_with_boundary_input(
            &probe,
            &[&test],
            &gathered.activation,
            gathered.unresolved_boundary.as_deref(),
        );
        assert_eq!(infection.state, StageState::Yes, "{}", infection.summary);
    }

    #[test]
    fn computed_builder_argument_is_not_credited_end_to_end() {
        // Exact-head review on #6796: `.amount(name.len() + 10)` passes
        // no 10, so the boundary `amount > 10` is not credited.
        let owner = function("pub fn score(amount: u32) -> bool {\n    amount > 10\n}");
        let body = "let request = Request::builder()\n.amount(name.len() + 10)\n.build();\nassert!(score(request.amount));";
        let mut test = test_with_body_calls(body, &[]);
        test.literals = vec![LiteralFact {
            line: 11,
            value: "10".to_string(),
        }];
        let probe = probe(ProbeFamily::Predicate, "amount > 10");
        let gathered = boundary_input(&owner, &probe, &[&test]);
        assert!(owner_input_values(&gathered.activation).is_empty());
        let infection = super::super::infection::infection_evidence_with_boundary_input(
            &probe,
            &[&test],
            &gathered.activation,
            gathered.unresolved_boundary.as_deref(),
        );
        assert_ne!(infection.state, StageState::Yes, "{}", infection.summary);
    }

    #[test]
    fn free_identifiers_name_only_the_variables_an_expression_reads() {
        assert_eq!(free_identifiers("base + 1"), vec!["base"]);
        assert_eq!(free_identifiers("&[b'f'; 16]"), Vec::<String>::new());
        assert_eq!(free_identifiers("cfg.limit + 1"), vec!["cfg"]);
        assert_eq!(free_identifiers("len(x) * 2u32"), vec!["x"]);
        assert_eq!(free_identifiers("n as u64 + LIMIT"), vec!["n", "LIMIT"]);
        assert!(deterministic_computed_argument("LIMIT - 1", |_| false));
        assert!(deterministic_computed_argument("2 + 2", |_| false));
        assert!(!deterministic_computed_argument("age + 1", |_| false));
        assert!(deterministic_computed_argument("age + 1", |name| name == "age"));
        assert!(!deterministic_computed_argument("age", |_| true));
    }

    #[test]
    fn given_computed_helper_hop_argument_then_boundary_is_unresolved_not_missing() {
        // CodeRabbit review on #6796: the test calls `entry(9)` with a
        // literal, `entry` forwards to `inner(x)`, and `inner` calls the
        // owner as `score(y + 1)`. Row transfer stops at `y + 1`, so the
        // compared parameter is unreadable, not missing.
        let owner = function("pub fn score(amount: u32) -> bool {\n    10 <= amount\n}");
        let mut inner = function("pub fn inner(y: u32) -> bool {\n    score(y + 1)\n}");
        inner.name = "inner".to_string();
        let mut entry = function("pub fn entry(x: u32) -> bool {\n    inner(x)\n}");
        entry.name = "entry".to_string();
        let chain_with = |owner_argument: &str| super::super::helper_transfer::HelperChain {
            hops: vec![
                super::super::helper_transfer::HelperHop {
                    caller: inner.clone(),
                    call_text: format!("score({owner_argument})"),
                    arguments: vec![owner_argument.to_string()],
                },
                super::super::helper_transfer::HelperHop {
                    caller: entry.clone(),
                    call_text: "inner(x)".to_string(),
                    arguments: vec!["x".to_string()],
                },
            ],
            stop_above: None,
        };
        let mut test = test_with_call("entry_boundary", "assert!(entry(9));");
        test.calls[0].name = "entry".to_string();
        let run = |chain: &super::super::helper_transfer::HelperChain, test: &TestSummary| {
            activation_and_boundary_input(
                &probe(ProbeFamily::Predicate, "10 <= amount"),
                Some(&owner),
                &[test],
                &[],
                Some(chain),
                &crate::analysis::rust_index::RustIndex::default(),
                false,
                None,
            )
        };

        let computed = run(&chain_with("y + 1"), &test);
        assert!(computed.activation.missing_discriminators.is_empty());
        let reason = computed.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("computed argument for `amount`"),
            "got {reason:?}"
        );

        // Negative control: a straight pass-through binds the literal row,
        // so the off-boundary input keeps the actionable missing input.
        let straight = run(&chain_with("y"), &test);
        assert_eq!(straight.unresolved_boundary, None);
        assert_eq!(straight.activation.missing_discriminators.len(), 1);

        // A chain no related test enters says nothing about computed
        // arguments: the computed hop is out of scope.
        let mut unrelated = test.clone();
        unrelated.calls[0].name = "other".to_string();
        let unentered = run(&chain_with("y + 1"), &unrelated);
        assert_eq!(unentered.unresolved_boundary, None);
    }

    #[test]
    fn given_both_boundary_operands_unreadable_then_boundary_is_unresolved_not_missing() {
        // CodeRabbit review on #6796: exact rows exist, but neither
        // `out.count_ones()` nor `data.count_ones() / 2` folds from them,
        // so no row says which side of the boundary a test reaches.
        let owner = function(
            "pub fn score(out: u32, data: u32) -> bool {\n    out.count_ones() != data.count_ones() / 2\n}",
        );
        let test = test_with_call("score_boundary", "assert!(score(4, 8));");
        let gathered = boundary_input(
            &owner,
            &probe(
                ProbeFamily::Predicate,
                "out.count_ones() != data.count_ones() / 2",
            ),
            &[&test],
        );

        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "{:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operands `out.count_ones()` and `data.count_ones() / 2`"),
            "got {reason:?}"
        );
    }

    #[test]
    fn given_offset_of_opaque_constant_then_boundary_is_unresolved_not_missing() {
        // #6671 with gemini's review on #6796: `LIMIT - 2` over a
        // suffixed or hex `LIMIT` has no visible value. It is neither
        // read as 2 nor parsed from a guess, and since naming `LIMIT` is
        // not the offset boundary by identity, no missing input is named.
        for declaration in ["const LIMIT: i32 = 10i32;", "const LIMIT: i32 = 0x10;"] {
            let owner = function("pub fn score(amount: i32) -> bool {\n    amount > LIMIT - 2\n}");
            let mut index = crate::analysis::rust_index::RustIndex::default();
            index.insert_file_only(
                PathBuf::from("src/lib.rs"),
                crate::analysis::facts::FileFacts {
                    path: PathBuf::from("src/lib.rs"),
                    source: format!("{declaration}\n{}", owner.body).into(),
                    ..Default::default()
                },
            );
            let tests = [
                test_with_call("score_boundary", "score(2);"),
                test_with_call("score_boundary", "score(LIMIT);"),
            ];
            let related = tests.iter().collect::<Vec<_>>();
            let gathered = activation_and_boundary_input(
                &probe(ProbeFamily::Predicate, "amount > LIMIT - 2"),
                Some(&owner),
                &related,
                &[],
                None,
                &index,
                false,
                None,
            );
            assert!(
                !has_observed_boundary_equality(&gathered.activation),
                "{declaration}"
            );
            assert!(
                gathered.activation.missing_discriminators.is_empty(),
                "{declaration}: {:?}",
                gathered.activation.missing_discriminators
            );
            let reason = gathered.unresolved_boundary.unwrap_or_default();
            assert!(
                reason.contains("offsets constant `LIMIT`"),
                "{declaration}: got {reason:?}"
            );
        }
    }

    #[test]
    fn given_literal_inputs_off_the_boundary_then_missing_boundary_is_still_reported() {
        // Negative control: exact inputs on one side keep the actionable gap.
        let owner = function("pub fn score(amount: u32) -> bool {\n    10 <= amount\n}");
        let test = test_with_body_calls(
            "assert!(score(20));\nassert!(!score(5));",
            &[(10, "assert!(score(20));"), (11, "assert!(!score(5));")],
        );
        let gathered = boundary_input(
            &owner,
            &probe(ProbeFamily::Predicate, "10 <= amount"),
            &[&test],
        );

        assert_eq!(gathered.unresolved_boundary, None);
        assert_eq!(gathered.activation.missing_discriminators.len(), 1);
        assert_eq!(
            gathered.activation.missing_discriminators[0].value,
            "amount == 10"
        );
        assert!(
            gathered.activation.missing_discriminators[0]
                .reason
                .contains("observed amount values: 20, 5")
        );
    }

    #[test]
    fn given_local_accumulator_boundary_then_boundary_is_unresolved_not_missing() {
        // #6674: `count` grows once per input byte; no row maps it to a
        // test input, so "observed count values: unknown" is not a gap.
        let mut owner = function(
            "pub fn score(hex: &[u8]) -> bool {\n    let mut count = 0;\n    for _ in hex {\n        count += 1;\n        if 16 < count {\n            return true;\n        }\n    }\n    false\n}",
        );
        owner.end_line = 10;
        let mut predicate = probe(ProbeFamily::Predicate, "16 < count");
        predicate.location.line = 5;
        let test = test_with_body_calls(
            "assert!(!score(b\"ff\"));\nassert!(!score(&[b'f'; 16]));\nassert!(score(&[b'f'; 17]));",
            &[
                (10, "assert!(!score(b\"ff\"));"),
                (11, "assert!(!score(&[b'f'; 16]));"),
                (12, "assert!(score(&[b'f'; 17]));"),
            ],
        );
        let gathered = boundary_input(&owner, &predicate, &[&test]);

        assert!(gathered.activation.missing_discriminators.is_empty());
        assert!(
            !owner_input_values(&gathered.activation).contains(&"16"),
            "an array-repeat length is not an input value: {:?}",
            gathered.activation.observed_values
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `count`"),
            "got {reason:?}"
        );

        // With only computed arguments there is no row at all: no missing
        // input is named. `hex` is not a compared operand, so the
        // computed-argument reason stays out of scope, but the compared
        // local `count` is still unreadable (Devin review on #6796).
        let computed_only = test_with_body_calls(
            "assert!(!score(&[b'f'; 16]));",
            &[(10, "assert!(!score(&[b'f'; 16]));")],
        );
        let gathered = boundary_input(&owner, &predicate, &[&computed_only]);
        assert!(gathered.activation.missing_discriminators.is_empty());
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `count`"),
            "got {reason:?}"
        );
    }

    #[test]
    fn given_length_of_string_input_boundary_then_boundary_is_unresolved_not_missing() {
        // #6693: `s.len() < 4` with inputs "1234" and "123" sits on both
        // sides; ripr does not fold `.len()`, so it abstains.
        let owner = function("pub fn score(s: &str) -> bool {\n    s.len() < 4\n}");
        let test = test_with_body_calls(
            "assert!(!score(\"1234\"));\nassert!(score(\"123\"));",
            &[
                (10, "assert!(!score(\"1234\"));"),
                (11, "assert!(score(\"123\"));"),
            ],
        );
        let gathered = boundary_input(
            &owner,
            &probe(ProbeFamily::Predicate, "s.len() < 4"),
            &[&test],
        );

        assert!(gathered.activation.missing_discriminators.is_empty());
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `s.len()`"),
            "got {reason:?}"
        );
    }

    #[test]
    fn given_counted_local_boundary_then_boundary_is_unresolved_not_missing() {
        // #6693's counted-local shape: `letters` counts the input's words,
        // and the tests pass "" and "Ada Lovelace" (both sides). ripr does
        // not fold the count, so it abstains instead of naming a missing
        // `letters == 0` input. Only this abstain fallback is delivered;
        // folding the count into a witness is not.
        let mut owner = function(
            "pub fn initials(name: &str) -> String {\n    let letters = name.split_whitespace().count();\n    if letters == 0 {\n        return String::new();\n    }\n    name.to_string()\n}",
        );
        owner.end_line = 7;
        let mut predicate = probe(ProbeFamily::Predicate, "letters == 0");
        predicate.location.line = 3;
        let test = test_with_body_calls(
            "assert_eq!(initials(\"\"), \"\");\nassert_eq!(initials(\"Ada Lovelace\"), \"AL\");",
            &[
                (10, "assert_eq!(initials(\"\"), \"\");"),
                (11, "assert_eq!(initials(\"Ada Lovelace\"), \"AL\");"),
            ],
        );
        let mut test = test;
        for call in &mut test.calls {
            call.name = "initials".to_string();
        }
        owner.name = "initials".to_string();
        let gathered = boundary_input(&owner, &predicate, &[&test]);

        assert!(
            gathered.activation.missing_discriminators.is_empty(),
            "{:?}",
            gathered.activation.missing_discriminators
        );
        let reason = gathered.unresolved_boundary.unwrap_or_default();
        assert!(
            reason.contains("boundary operand `letters`"),
            "got {reason:?}"
        );
    }

    #[test]
    fn computed_only_call_with_local_counter_boundary_is_unknown_not_weak() {
        // Devin review on #6796: the only owner call passes a computed
        // array (`&[b'f'; 16]`), so no row exists, and the compared `count`
        // is a local counter. The stray length literal 16 must not read as
        // a weak boundary gap: the boundary is unresolved, infection unknown.
        let mut owner = function(
            "pub fn too_long(hex: &[u8]) -> bool {\n    let mut count = 0;\n    for _ in hex {\n        count += 1;\n    }\n    16 < count\n}",
        );
        owner.name = "too_long".to_string();
        owner.end_line = 7;
        let mut predicate = probe(ProbeFamily::Predicate, "16 < count");
        predicate.location.line = 6;
        let mut test = test_with_body_calls(
            "assert!(!too_long(&[b'f'; 16]));",
            &[(10, "assert!(!too_long(&[b'f'; 16]));")],
        );
        for call in &mut test.calls {
            call.name = "too_long".to_string();
        }
        test.literals = vec![LiteralFact {
            line: 10,
            value: "16".to_string(),
        }];
        let gathered = boundary_input(&owner, &predicate, &[&test]);
        assert!(gathered.activation.missing_discriminators.is_empty());
        assert!(
            gathered
                .unresolved_boundary
                .as_deref()
                .is_some_and(|reason| reason.contains("boundary operand `count`")),
            "{:?}",
            gathered.unresolved_boundary
        );
        let infection = super::super::infection::infection_evidence_with_boundary_input(
            &predicate,
            &[&test],
            &gathered.activation,
            gathered.unresolved_boundary.as_deref(),
        );
        assert_eq!(
            infection.state,
            StageState::Unknown,
            "{}",
            infection.summary
        );

        // Control: with no owner call at all the operand rule does not
        // fire (no call says the inputs exist but are unreadable).
        let mut uncalled = test.clone();
        uncalled.calls.clear();
        let gathered = boundary_input(&owner, &predicate, &[&uncalled]);
        assert!(
            gathered.unresolved_boundary.is_none(),
            "{:?}",
            gathered.unresolved_boundary
        );
    }

    #[test]
    fn method_call_builder_argument_is_not_credited_end_to_end() {
        // Devin review on #6796 (#6918 case a): `.amount(x.min(10))`
        // passes no 10, so `amount > 10` is not credited.
        let owner = function("pub fn score(amount: u32) -> bool {\n    amount > 10\n}");
        let body = "let request = Request::builder()\n.amount(x.min(10))\n.build();\nassert!(score(request.amount));";
        let mut test = test_with_body_calls(body, &[]);
        test.literals = vec![LiteralFact {
            line: 11,
            value: "10".to_string(),
        }];
        let probe = probe(ProbeFamily::Predicate, "amount > 10");
        let gathered = boundary_input(&owner, &probe, &[&test]);
        assert!(owner_input_values(&gathered.activation).is_empty());
        let infection = super::super::infection::infection_evidence_with_boundary_input(
            &probe,
            &[&test],
            &gathered.activation,
            gathered.unresolved_boundary.as_deref(),
        );
        assert_ne!(infection.state, StageState::Yes, "{}", infection.summary);
    }
}
