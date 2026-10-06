//! Same-test pairing for boundary-class (predicate) probes.
//!
//! `exposed` requires one test that both feeds a boundary input to the owner
//! and holds a discriminating oracle on that call's result. Crediting a
//! boundary call from one test and an exact oracle from another is a false
//! `exposed` (#4828): `>=` → `>` still passes both tests.

use super::super::facts::CallFact;
use super::super::rust_index::{FunctionSummary, OracleFact, TestSummary, extract_literals};
use super::activation::{
    call_arguments, comparison_operands, function_parameters, owner_argument_values,
};
use super::helper_transfer::{HelperChain, chain_forwards_owner_result};
use super::text::delimited_contents_at;
use crate::domain::*;

/// Token carried in the discriminate summary when a predicate would otherwise
/// read `exposed` without an admitted oracle on the boundary call.
pub(in crate::analysis) const SAME_TEST_PAIRING_MISSING: &str = "same_test_pairing_missing";

pub(in crate::analysis) fn same_test_pairing_missing_summary() -> String {
    format!(
        "Discriminator unconfirmed: no admitted discriminating oracle is paired with the owner's boundary call ({SAME_TEST_PAIRING_MISSING}); a boundary input and a separate exact oracle do not establish that discriminator"
    )
}

/// True when some related test both feeds a boundary input to the owner and
/// holds an admitted discriminating oracle on that call's result.
///
/// Boundary credit is the activation authority's `==` facts (named constants,
/// helper hops, local bindings), not a second matcher. Call-argument matching
/// remains only for the literal/parameter shape those facts do not cover.
///
/// Non-predicate probes are not this gate; the caller must not use a `true`
/// result to promote a family this function does not judge.
///
/// `test_activation` recomputes activation from one test alone. The wrapper
/// entry path reads only those rows (#6780 review): `ValueFact` carries no
/// source test, so a row from the run-wide `activation` cannot be told apart
/// from a same-line row of another test in another file.
pub(in crate::analysis) fn has_same_test_boundary_oracle_pairing(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    activation: &ActivationEvidence,
    helper_chain: Option<&HelperChain>,
    assertion_admitted: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    test_activation: &dyn Fn(&TestSummary) -> ActivationEvidence,
) -> bool {
    if !matches!(probe.family, ProbeFamily::Predicate) {
        return false;
    }
    let Some(owner) = owner_fn else {
        return false;
    };
    // #6694 / #6672: a private helper reached only through a wrapper pairs
    // on the wrapper call when every hop hands the helper's result to its
    // caller's return; any other chain shape keeps the pairing missing.
    let forwarding_entry = helper_chain
        .filter(|chain| chain_forwards_owner_result(&owner.name, chain))
        .and_then(|chain| chain.hops.last())
        .map(|hop| hop.caller.name.as_str());
    related_tests.iter().any(|test| {
        test_pairs_boundary_input_with_oracle(
            probe,
            owner,
            test,
            activation,
            forwarding_entry,
            assertion_admitted,
            test_activation,
        )
    })
}

fn test_pairs_boundary_input_with_oracle(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    activation: &ActivationEvidence,
    forwarding_entry: Option<&str>,
    assertion_admitted: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    test_activation: &dyn Fn(&TestSummary) -> ActivationEvidence,
) -> bool {
    let bound_names = boundary_bound_locals(probe, owner, test, activation);
    // Computed at most once per test, and only when the entry path is live.
    let own_rows: std::cell::OnceCell<ActivationEvidence> = std::cell::OnceCell::new();
    test.assertions.iter().any(|assertion| {
        if !assertion_admitted(test, assertion) || !assertion_is_discriminating(assertion) {
            return false;
        }
        assertion_observes_boundary_owner_call(probe, owner, test, assertion, activation)
            || assertion_observes_bound_name(assertion, &bound_names)
            || forwarding_entry.is_some_and(|entry| {
                assertion_names_one_entry_call(owner, entry, assertion)
                    && assertion_observes_boundary_entry_call(
                        owner,
                        entry,
                        assertion,
                        own_rows.get_or_init(|| test_activation(test)),
                    )
            })
    })
}

fn assertion_names_one_entry_call(
    owner: &FunctionSummary,
    entry: &str,
    assertion: &OracleFact,
) -> bool {
    let subject = assertion_subject(&assertion.text);
    owner_call_count(&subject, entry) == 1
        && owner_call_count(&assertion.text, entry) == 1
        && owner_call_count(&assertion.text, &owner.name) == 0
}

/// The assertion's subject is one call of the chain's entry, the assertion
/// names neither the owner nor a second entry call, and activation already
/// recorded a boundary `==` row bound down the chain from this assertion's
/// line (the transferred row carries the entry call's text). `activation`
/// must hold only rows recomputed from the assertion's own test.
fn assertion_observes_boundary_entry_call(
    owner: &FunctionSummary,
    entry: &str,
    assertion: &OracleFact,
    activation: &ActivationEvidence,
) -> bool {
    assertion_names_one_entry_call(owner, entry, assertion)
        && activation.observed_values.iter().any(|fact| {
            // The transferred row's provenance starts with the entry call's
            // line text. That line must hold this assertion, exactly one
            // entry call, and no direct owner call (#6780 review N1): a
            // same-line `is_bulk(10)` row must not pair a far wrapper pin.
            let call_line = fact.text.split(" | ").next().unwrap_or_default();
            fact.line == assertion.line
                && fact.value.contains(" == ")
                && call_line.contains(&assertion.text)
                && owner_call_count(call_line, entry) == 1
                && owner_call_count(call_line, &owner.name) == 0
        })
}

fn assertion_is_discriminating(assertion: &OracleFact) -> bool {
    matches!(assertion.strength, OracleStrength::Strong)
        || matches!(
            assertion.kind,
            OracleKind::ExactValue
                | OracleKind::WholeObjectEquality
                | OracleKind::ExactErrorVariant
        )
}

fn assertion_observes_boundary_owner_call(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    assertion: &OracleFact,
    activation: &ActivationEvidence,
) -> bool {
    let subject = assertion_subject(&assertion.text);
    if owner_call_argument_lists(&subject, &owner.name)
        .iter()
        .any(|arguments| argument_list_activates_boundary(probe, owner, test, arguments))
    {
        return true;
    }
    // Line-level activation cannot tell two same-name calls apart. Use it
    // only when the assertion text names the owner once.
    owner_call_count(&assertion.text, &owner.name) == 1
        && activation_marks_boundary_call(
            activation,
            &CallFact {
                line: assertion.line,
                name: owner.name.clone(),
                text: assertion.text.clone(),
            },
        )
}

fn assertion_observes_bound_name(assertion: &OracleFact, bound_names: &[String]) -> bool {
    bound_names
        .iter()
        .any(|name| contains_ident(&assertion.text, name))
}

fn boundary_bound_locals(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    activation: &ActivationEvidence,
) -> Vec<String> {
    // Last binding of a name wins so `let got = gate(10); let got = gate(100)`
    // does not keep the shadowed boundary result.
    let mut last: Vec<(String, bool)> = Vec::new();
    for (offset, line) in test.body.lines().enumerate() {
        let Some(name) = let_binding_name(line) else {
            continue;
        };
        let line_number = test.start_line + offset;
        let call = CallFact {
            line: line_number,
            name: owner.name.clone(),
            text: line.trim().to_string(),
        };
        let is_boundary = owner_call_activates_boundary(probe, owner, test, &call, activation);
        if let Some(existing) = last.iter_mut().find(|(bound, _)| bound == &name) {
            existing.1 = is_boundary;
        } else {
            last.push((name, is_boundary));
        }
    }
    last.into_iter()
        .filter_map(|(name, is_boundary)| is_boundary.then_some(name))
        .collect()
}

fn let_binding_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("let")?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() || name == "_" || name.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    let after = rest.get(name.len()..)?.trim_start();
    let (before_eq, _) = after.split_once('=')?;
    let before_eq = before_eq.trim();
    (before_eq.is_empty() || before_eq.starts_with(':')).then_some(name)
}

fn owner_call_activates_boundary(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    call: &CallFact,
    activation: &ActivationEvidence,
) -> bool {
    if call.name != owner.name {
        return false;
    }
    if let Some(arguments) = call_arguments(&call.text, &call.name)
        && argument_list_activates_boundary(probe, owner, test, &arguments)
    {
        return true;
    }
    owner_call_count(&call.text, &owner.name) == 1
        && activation_marks_boundary_call(activation, call)
}

fn argument_list_activates_boundary(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    arguments: &[String],
) -> bool {
    let arg_values: Vec<Vec<String>> = arguments
        .iter()
        .map(|argument| owner_argument_values(test, argument))
        .collect();
    let Some((left, right)) = comparison_operands(&probe.expression) else {
        return false;
    };
    let parameters = function_parameters(owner);
    let left_index = parameter_index(&parameters, &left);
    let right_index = parameter_index(&parameters, &right);
    if let (Some(left_index), Some(right_index)) = (left_index, right_index) {
        return values_overlap(
            arg_values.get(left_index).map(Vec::as_slice).unwrap_or(&[]),
            arg_values
                .get(right_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
    }
    if let Some(left_index) = left_index {
        let right_literals = extract_literals(&right);
        return values_overlap(
            arg_values.get(left_index).map(Vec::as_slice).unwrap_or(&[]),
            &right_literals,
        );
    }
    if let Some(right_index) = right_index {
        let left_literals = extract_literals(&left);
        return values_overlap(
            arg_values
                .get(right_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &left_literals,
        );
    }
    false
}

fn assertion_subject(text: &str) -> String {
    for marker in ["assert_eq!(", "assert_ne!(", "assert_matches!(", "assert!("] {
        if let Some(index) = find_marker(text, marker)
            && let Some(open) = index.checked_add(marker.len().saturating_sub(1))
            && let Some(inner) = delimited_contents_at(text, open)
        {
            return inner;
        }
    }
    text.to_string()
}

fn owner_call_argument_lists(text: &str, name: &str) -> Vec<Vec<String>> {
    let needle = format!("{name}(");
    let mut lists = Vec::new();
    let mut from = 0usize;
    while from < text.len() {
        let Some(rel) = text.get(from..).and_then(|rest| rest.find(&needle)) else {
            break;
        };
        let abs = from + rel;
        if abs > 0 {
            let before = text.as_bytes()[abs - 1];
            if before.is_ascii_alphanumeric() || before == b'_' {
                from = abs + 1;
                continue;
            }
        }
        if let Some(arguments) = call_arguments(text.get(abs..).unwrap_or(""), name) {
            lists.push(arguments);
        }
        from = abs + needle.len();
    }
    lists
}

fn owner_call_count(text: &str, name: &str) -> usize {
    owner_call_argument_lists(text, name).len()
}

fn contains_ident(text: &str, name: &str) -> bool {
    find_ident_at(text, name).is_some()
}

fn find_ident_at(text: &str, name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    let mut from = 0usize;
    while from < text.len() {
        let rel = text.get(from..).and_then(|rest| rest.find(name))?;
        let abs = from + rel;
        let before_ok = abs == 0 || {
            let before = text.as_bytes()[abs - 1];
            !(before.is_ascii_alphanumeric() || before == b'_')
        };
        let end = abs + name.len();
        let after_ok = end >= text.len() || {
            let after = text.as_bytes()[end];
            !(after.is_ascii_alphanumeric() || after == b'_')
        };
        if before_ok && after_ok {
            return Some(abs);
        }
        from = abs + 1;
    }
    None
}

fn find_marker(text: &str, marker: &str) -> Option<usize> {
    let mut from = 0usize;
    while from < text.len() {
        let rel = text.get(from..).and_then(|rest| rest.find(marker))?;
        let abs = from + rel;
        let before_ok = abs == 0 || {
            let before = text.as_bytes()[abs - 1];
            !(before.is_ascii_alphanumeric() || before == b'_')
        };
        if before_ok {
            return Some(abs);
        }
        from = abs + 1;
    }
    None
}

fn parameter_index(parameters: &[String], operand: &str) -> Option<usize> {
    parameters.iter().position(|parameter| parameter == operand)
}

fn values_overlap(left: &[String], right: &[String]) -> bool {
    !left.is_empty() && !right.is_empty() && left.iter().any(|value| right.contains(value))
}

/// Infection already recorded that this owner-call line sits on the boundary
/// (`left == right` from named constants, helper hops, or local bindings).
fn activation_marks_boundary_call(activation: &ActivationEvidence, call: &CallFact) -> bool {
    activation.observed_values.iter().any(|fact| {
        fact.line == call.line
            && fact.value.contains(" == ")
            && (fact.text.is_empty() || fact.text.contains(&call.text))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::rust_index::{
        CallFact, LiteralFact, OracleFact, extract_identifier_tokens,
    };
    use std::path::PathBuf;

    // These units isolate semantic pairing of already-admitted oracle facts.
    // Public API/runtime controls exercise the real parser-backed admission.
    fn pairing_with_admitted_oracles(
        probe: &Probe,
        owner: Option<&FunctionSummary>,
        tests: &[&TestSummary],
        activation: &ActivationEvidence,
    ) -> bool {
        has_same_test_boundary_oracle_pairing(
            probe,
            owner,
            tests,
            activation,
            None,
            &|_, _| true,
            &|_| activation.clone(),
        )
    }

    #[test]
    fn split_tests_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let boundary = test_summary(
            "boundary",
            "let _ = gate(10); let _ = gate(9);",
            vec![
                call("gate", "let _ = gate(10);"),
                call("gate", "let _ = gate(9);"),
            ],
            vec![],
            &["10", "9"],
        );
        let far = test_summary(
            "far",
            "assert_eq!(gate(100), true);",
            vec![call("gate", "assert_eq!(gate(100), true);")],
            vec![exact("assert_eq!(gate(100), true);")],
            &["100"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&boundary, &far],
                &ActivationEvidence::default(),
            ),
            "a no-oracle boundary call plus a far exact oracle must not pair"
        );
    }

    #[test]
    fn same_call_assert_eq_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let paired = test_summary(
            "paired_boundary",
            "assert_eq!(gate(10), true);",
            vec![call("gate", "assert_eq!(gate(10), true);")],
            vec![exact("assert_eq!(gate(10), true);")],
            &["10"],
        );
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "assert_eq!(gate(10), true) must pair"
        );
    }

    #[test]
    fn same_test_split_calls_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mixed = test_summary(
            "mixed",
            "let _ = gate(10);\nassert_eq!(gate(100), true);",
            vec![
                call("gate", "let _ = gate(10);"),
                call("gate", "assert_eq!(gate(100), true);"),
            ],
            vec![exact("assert_eq!(gate(100), true);")],
            &["10", "100"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&mixed],
                &ActivationEvidence::default(),
            ),
            "boundary call and far oracle in the same test still need the oracle on the boundary call"
        );
    }

    #[test]
    fn same_line_split_calls_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mixed = test_summary(
            "mixed",
            "let _ = gate(10); assert_eq!(gate(100), true);",
            vec![
                call("gate", "let _ = gate(10); assert_eq!(gate(100), true);"),
                call("gate", "let _ = gate(10); assert_eq!(gate(100), true);"),
            ],
            vec![exact("let _ = gate(10); assert_eq!(gate(100), true);")],
            &["10", "100"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&mixed],
                &ActivationEvidence::default(),
            ),
            "a same-line unasserted boundary call plus a far exact oracle must not pair"
        );
    }

    #[test]
    fn irrelevant_argument_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: "pub fn gate(input: u32, marker: u32) -> bool { input >= 10 }".to_string(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let far = test_summary(
            "far",
            "assert_eq!(gate(100, 10), true);",
            vec![call("gate", "assert_eq!(gate(100, 10), true);")],
            vec![exact("assert_eq!(gate(100, 10), true);")],
            &["100", "10"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&far],
                &ActivationEvidence::default(),
            ),
            "a boundary literal in an unused argument must not pair"
        );
    }

    #[test]
    fn shadowed_boundary_binding_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut shadowed = test_summary(
            "shadowed",
            "let got = gate(10);\nlet got = gate(100);\nassert_eq!(got, true);",
            vec![
                call("gate", "let got = gate(10);"),
                call("gate", "let got = gate(100);"),
            ],
            vec![exact("assert_eq!(got, true);")],
            &["10", "100"],
        );
        shadowed.calls[0].line = 1;
        shadowed.calls[1].line = 2;
        shadowed.assertions[0].line = 3;
        shadowed.end_line = 4;
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&shadowed],
                &ActivationEvidence::default(),
            ),
            "asserting a binding that later shadows the boundary result must not pair"
        );
    }

    #[test]
    fn short_let_bound_name_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut bound = test_summary(
            "bound",
            "let x = gate(10);\nassert_eq!(x, true);",
            vec![call("gate", "let x = gate(10);")],
            vec![exact("assert_eq!(x, true);")],
            &["10"],
        );
        bound.calls[0].line = 1;
        bound.assertions[0].line = 2;
        bound.end_line = 3;
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&bound],
                &ActivationEvidence::default(),
            ),
            "let x = gate(10); assert_eq!(x, true) must pair"
        );
    }

    #[test]
    fn let_bound_boundary_call_asserted_later_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut bound = test_summary(
            "bound",
            "let got = gate(10);\nassert_eq!(got, true);",
            vec![call("gate", "let got = gate(10);")],
            vec![exact("assert_eq!(got, true);")],
            &["10"],
        );
        bound.calls[0].line = 1;
        bound.assertions[0].line = 2;
        bound.end_line = 3;
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&bound],
                &ActivationEvidence::default(),
            ),
            "let got = gate(10); assert_eq!(got, true) must pair"
        );
    }

    #[test]
    fn typed_let_bound_boundary_call_asserted_later_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut bound = test_summary(
            "bound",
            "let got: bool = gate(10);\nassert_eq!(got, true);",
            vec![call("gate", "let got: bool = gate(10);")],
            vec![exact("assert_eq!(got, true);")],
            &["10"],
        );
        bound.calls[0].line = 1;
        bound.assertions[0].line = 2;
        bound.end_line = 3;
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&bound],
                &ActivationEvidence::default(),
            ),
            "let got: bool = gate(10); assert_eq!(got, true) must pair"
        );
    }

    #[test]
    fn let_binding_name_requires_keyword_boundary_and_allows_type_ascription() {
        assert_eq!(
            let_binding_name("let got = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(
            let_binding_name("let mut got = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(
            let_binding_name("let got: bool = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(
            let_binding_name("let mut got: bool = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(let_binding_name("letter = gate(10);"), None);
        assert_eq!(let_binding_name("let_got = gate(10);"), None);
        assert_eq!(let_binding_name("let _ = gate(10);"), None);
    }

    #[test]
    fn two_parameter_equal_args_in_assert_eq_pair() {
        let probe = predicate_probe("amount >= discount_threshold");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::discounted_total".to_string()),
            name: "discounted_total".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 6,
            body: "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 { amount }"
                .to_string(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "equality_threshold_discounts",
            "assert_eq!(discounted_total(100, 100), 90);",
            vec![call(
                "discounted_total",
                "assert_eq!(discounted_total(100, 100), 90);",
            )],
            vec![exact("assert_eq!(discounted_total(100, 100), 90);")],
            &["100", "90"],
        );
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "equal parameter arguments in the asserted owner call must pair"
        );
    }

    #[test]
    fn weak_assertion_on_boundary_call_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let weak = test_summary(
            "smoke",
            "assert!(gate(10));",
            vec![call("gate", "assert!(gate(10));")],
            vec![OracleFact {
                line: 1,
                text: "assert!(gate(10));".to_string(),
                kind: OracleKind::RelationalCheck,
                strength: OracleStrength::Weak,
                observed_tokens: extract_identifier_tokens("assert!(gate(10));"),
                ok_value_observed: None,
            }],
            &["10"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&weak],
                &ActivationEvidence::default(),
            ),
            "a weak oracle on the boundary call is not a discriminating pairing"
        );
    }

    #[test]
    fn activation_equality_fact_pairs_when_call_args_are_not_probe_literals() {
        let probe = predicate_probe("final_label == \"alpha\"");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::classify".to_string()),
            name: "classify".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 9,
            end_line: 16,
            body: "pub fn classify(input: &str) -> &'static str { input }".to_string(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "word_label_is_word",
            "assert_eq!(classify(\"word\"), \"word\");",
            vec![call(
                "classify",
                "assert_eq!(classify(\"word\"), \"word\");",
            )],
            vec![exact("assert_eq!(classify(\"word\"), \"word\");")],
            &["word"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(classify(\"word\"), \"word\"); | helper hop".to_string(),
                value: "final_label == \"alpha\"".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&paired], &activation),
            "an activation == fact on the asserted owner call must pair even when the call args are not the probe literals"
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "without the activation == fact, classify(\"word\") must not pair against final_label == \"alpha\""
        );
    }

    fn wrapper_chain(wrapper_body: &str) -> HelperChain {
        let mut wrapper = gate_owner();
        wrapper.name = "order_discount".to_string();
        wrapper.id = SymbolId("src/lib.rs::order_discount".to_string());
        wrapper.body = wrapper_body.to_string();
        HelperChain {
            hops: vec![crate::analysis::classify::helper_transfer::HelperHop {
                caller: wrapper,
                call_text: "if is_bulk(qty) {".to_string(),
                arguments: vec!["qty".to_string()],
            }],
            stop_above: None,
        }
    }

    fn bulk_owner() -> FunctionSummary {
        let mut owner = gate_owner();
        owner.name = "is_bulk".to_string();
        owner.id = SymbolId("src/lib.rs::is_bulk".to_string());
        owner.body = "fn is_bulk(qty: u32) -> bool { 10 <= qty }".to_string();
        owner
    }

    fn transferred_boundary_row(line: usize, assertion: &str) -> ActivationEvidence {
        ActivationEvidence {
            observed_values: vec![ValueFact {
                line,
                text: format!("{assertion} | exact input qty = 10; literal operand 10 = 10"),
                value: "qty == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        }
    }

    const FORWARDING_WRAPPER: &str =
        "pub fn order_discount(qty: u32) -> u32 {\n    if is_bulk(qty) { 5 } else { 0 }\n}";

    // #6694 / #6672: the wrapper's exact pin on the boundary input pairs with
    // the private helper's boundary when the wrapper forwards the helper's
    // result to its return and activation bound the row down the chain.
    #[test]
    fn forwarding_wrapper_oracle_pairs_with_the_helper_boundary() {
        let probe = predicate_probe("10 <= qty");
        let owner = bulk_owner();
        let assertion = "assert_eq!(order_discount(10), 5);";
        let test = test_summary(
            "ten_items_earn_the_bulk_discount",
            assertion,
            vec![call("order_discount", assertion)],
            vec![exact(assertion)],
            &["10", "5"],
        );
        let activation = transferred_boundary_row(1, assertion);
        let chain = wrapper_chain(FORWARDING_WRAPPER);
        let pairs = |chain: Option<&HelperChain>, activation: &ActivationEvidence| {
            has_same_test_boundary_oracle_pairing(
                &probe,
                Some(&owner),
                &[&test],
                activation,
                chain,
                &|_, _| true,
                &|_| activation.clone(),
            )
        };
        assert!(pairs(Some(&chain), &activation));
        // Discriminating controls: no chain, no transferred boundary row,
        // or a wrapper that drops the helper's result never pair.
        assert!(!pairs(None, &activation));
        assert!(!pairs(Some(&chain), &ActivationEvidence::default()));
        let dropping = wrapper_chain(
            "pub fn order_discount(qty: u32) -> u32 {\n    let _ = is_bulk(qty);\n    5\n}",
        );
        assert!(!pairs(Some(&dropping), &activation));
    }

    #[test]
    fn wrapper_oracle_off_the_boundary_line_does_not_pair() {
        // The boundary row sits on a call with no oracle; the asserted
        // wrapper call is a far input on another line.
        let probe = predicate_probe("10 <= qty");
        let owner = bulk_owner();
        let boundary_call = "let _ = order_discount(10);";
        let far = "assert_eq!(order_discount(12), 5);";
        let mut far_oracle = exact(far);
        far_oracle.line = 2;
        let test = test_summary(
            "split",
            &format!("{boundary_call}\n{far}"),
            vec![call("order_discount", boundary_call)],
            vec![far_oracle],
            &["10", "12", "5"],
        );
        let activation = transferred_boundary_row(1, boundary_call);
        assert!(!has_same_test_boundary_oracle_pairing(
            &probe,
            Some(&owner),
            &[&test],
            &activation,
            Some(&wrapper_chain(FORWARDING_WRAPPER)),
            &|_, _| true,
            &|_| activation.clone(),
        ));
    }

    fn pairs_through_forwarding_wrapper(
        assertion: &str,
        activation: &ActivationEvidence,
        wrapper_body: &str,
    ) -> bool {
        let test = test_summary(
            "wrapper_pin",
            assertion,
            vec![call("order_discount", assertion)],
            vec![exact(assertion)],
            &["10"],
        );
        has_same_test_boundary_oracle_pairing(
            &predicate_probe("10 <= qty"),
            Some(&bulk_owner()),
            &[&test],
            activation,
            Some(&wrapper_chain(wrapper_body)),
            &|_, _| true,
            &|_| activation.clone(),
        )
    }

    // #6780 review N1: a same-line direct owner call at the boundary must
    // not pair a far wrapper pin through the line-level row.
    #[test]
    fn same_line_owner_row_does_not_pair_a_far_wrapper_pin() {
        let line = "let ok = is_bulk(10); assert_eq!(order_discount(3), 0);";
        let mut oracle = exact("assert_eq!(order_discount(3), 0);");
        oracle.line = 1;
        let test = test_summary(
            "same_line",
            line,
            vec![call("is_bulk", line), call("order_discount", line)],
            vec![oracle],
            &["10", "3", "0"],
        );
        assert!(!has_same_test_boundary_oracle_pairing(
            &predicate_probe("10 <= qty"),
            Some(&bulk_owner()),
            &[&test],
            &transferred_boundary_row(1, line),
            Some(&wrapper_chain(FORWARDING_WRAPPER)),
            &|_, _| true,
            &|_| transferred_boundary_row(1, line),
        ));
    }

    // #6780 review N2: the entry guard refuses a second entry call and an
    // assertion that also names the owner.
    #[test]
    fn entry_guard_refuses_repeated_entry_calls_and_owner_mentions() {
        let repeated = "assert_eq!(order_discount(10), order_discount(10));";
        assert!(!pairs_through_forwarding_wrapper(
            repeated,
            &transferred_boundary_row(1, repeated),
            FORWARDING_WRAPPER,
        ));
        // Two owner calls keep the owner-call fallback out of the way, so
        // only the entry path could pair here.
        let names_owner =
            "assert_eq!(order_discount(10), u32::from(is_bulk(1)) * 5 + u32::from(is_bulk(2)));";
        assert!(!pairs_through_forwarding_wrapper(
            names_owner,
            &transferred_boundary_row(1, names_owner),
            FORWARDING_WRAPPER,
        ));
        // Control: the plain pin pairs.
        let plain = "assert_eq!(order_discount(10), 5);";
        assert!(pairs_through_forwarding_wrapper(
            plain,
            &transferred_boundary_row(1, plain),
            FORWARDING_WRAPPER,
        ));
    }

    // #6780 review (CodeRabbit): a transferred boundary row from test A must
    // not pair test B's wrapper pin on the same line in another file. Both
    // write the same entry call text; only A binds `qty` to the boundary.
    #[test]
    fn another_tests_same_line_row_does_not_pair_the_wrapper_pin() {
        let probe = predicate_probe("10 <= qty");
        let owner = bulk_owner();
        let line = "assert_eq!(order_discount(qty), 5);";
        let mut boundary_test = test_summary(
            "boundary_input_without_admitted_oracle",
            &format!("let qty = 10;\n{line}"),
            vec![call("order_discount", line)],
            Vec::new(),
            &["10", "5"],
        );
        boundary_test.file = PathBuf::from("tests/a.rs");
        let mut far_oracle = exact(line);
        far_oracle.line = 2;
        let mut far_test = test_summary(
            "far_input_with_oracle",
            &format!("let qty = 3;\n{line}"),
            vec![call("order_discount", line)],
            vec![far_oracle],
            &["3", "5"],
        );
        far_test.file = PathBuf::from("tests/b.rs");
        // The run-wide row carries only line and text: it cannot name A.
        let run_wide = transferred_boundary_row(2, line);
        let own_rows = |test: &TestSummary| {
            if test.file == boundary_test.file && test.name == boundary_test.name {
                transferred_boundary_row(2, line)
            } else {
                ActivationEvidence::default()
            }
        };
        let chain = wrapper_chain(FORWARDING_WRAPPER);
        assert!(!has_same_test_boundary_oracle_pairing(
            &probe,
            Some(&owner),
            &[&boundary_test, &far_test],
            &run_wide,
            Some(&chain),
            &|_, _| true,
            &own_rows,
        ));
        // Control: the same far test pairs when its own rows hold the boundary.
        assert!(has_same_test_boundary_oracle_pairing(
            &probe,
            Some(&owner),
            &[&boundary_test, &far_test],
            &run_wide,
            Some(&chain),
            &|_, _| true,
            &|_| transferred_boundary_row(2, line),
        ));
    }

    // #6780 review B1 / B3: a wrapper that rebinds the forwarded parameter,
    // or branches into a computed value, does not pair.
    #[test]
    fn rebinding_or_computed_branch_wrappers_do_not_pair() {
        let plain = "assert_eq!(order_discount(10), 5);";
        let row = transferred_boundary_row(1, plain);
        for body in [
            "pub fn order_discount(qty: u32) -> u32 {\n    let qty = qty * 2;\n    if is_bulk(qty) { 5 } else { 0 }\n}",
            "pub fn order_discount(mut qty: u32) -> u32 {\n    qty += 1;\n    if is_bulk(qty) { 5 } else { 0 }\n}",
            "pub fn order_discount(qty: u32) -> u32 {\n    if is_bulk(qty) { qty / 2 } else { 5 }\n}",
        ] {
            assert!(
                !pairs_through_forwarding_wrapper(plain, &row, body),
                "{body}"
            );
        }
    }

    fn predicate_probe(expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:test".to_string()),
            location: SourceLocation::new("src/lib.rs", 2, 1),
            owner: Some(SymbolId("src/lib.rs::gate".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn gate_owner() -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: "pub fn gate(input: u32) -> bool { input >= 10 }".to_string(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        }
    }

    fn test_summary(
        name: &str,
        body: &str,
        calls: Vec<CallFact>,
        assertions: Vec<OracleFact>,
        literals: &[&str],
    ) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/gate.rs"),
            start_line: 1,
            end_line: 4,
            body: body.to_string(),
            calls,
            assertions,
            literals: literals
                .iter()
                .map(|value| LiteralFact {
                    line: 1,
                    value: (*value).to_string(),
                })
                .collect(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn call(name: &str, text: &str) -> CallFact {
        CallFact {
            line: 1,
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    fn exact(text: &str) -> OracleFact {
        OracleFact {
            line: 1,
            text: text.to_string(),
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
            observed_tokens: extract_identifier_tokens(text),
            ok_value_observed: None,
        }
    }
}
