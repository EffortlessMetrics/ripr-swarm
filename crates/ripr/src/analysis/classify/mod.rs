mod activation;
mod arm_selection;
mod boundary_pairing;
mod context;
mod decision;
mod effect_carrier;
mod flow;
mod gap_admission;
mod helper_transfer;
mod infection;
mod match_transfer;
mod operand_pin;
mod owner_pin;
mod owner_shape;
mod propagation_witness;
mod reach;
mod related_tests;
mod reveal;
mod scanner_transfer;
mod text;
mod transitive_reach;
mod value_transfer;

pub(in crate::analysis) use activation::{
    ARM_UNSELECTED_REASON_PREFIX, LocalBoundary, TestValueFacts, activation_and_boundary_input,
    comparison_operands, computed_input_parameters, literal_operand_value, local_boundary,
    signature_parameters,
};
pub(in crate::analysis) use arm_selection::ArmSelector;
pub(in crate::analysis) use boundary_pairing::{
    WrapperEntryPairing, has_same_test_boundary_oracle_pairing, same_test_pairing_missing_summary,
};
pub(in crate::analysis) use context::ProbeContext;
pub(in crate::analysis) use decision::{
    classify, confidence_score, ensure_unknown_stop_reason, missing_evidence,
    recommended_next_step, stop_reasons,
};
pub(in crate::analysis) use effect_carrier::EffectStateCarrier;
pub(in crate::analysis) use flow::{local_flow_sinks, propagation_evidence_with_witness};
pub(in crate::analysis) use gap_admission::{
    REFUSALS_ARE_ANALYZER_LIMITS, withhold_unsupported_gap,
};
pub(in crate::analysis) use helper_transfer::{
    HELPER_RESULT_NOT_FORWARDED, callee_is_unique, chain_forwards_to_observed_hops,
    chain_passes_effect_target_to_observed_hops, helper_only_reach, resolve_chain,
};
pub(in crate::analysis) use infection::infection_evidence_with_boundary_input;
pub(in crate::analysis) use operand_pin::operand_only_pin;
pub(in crate::analysis) use owner_pin::{
    OwnerPinSyntax, OwnerReturnPin, WithheldMacroBindings, helper_pins_owner_call, pin_scope_needs,
    trait_impl_self_type_names,
};
pub(in crate::analysis) use owner_shape::is_assertion_shaped_owner;
pub(in crate::analysis) use propagation_witness::{
    PropagationWitnessV1, assertion_observes_direct_collection, current_path_witness,
    direct_collection_mutation_receiver,
};
pub(in crate::analysis) use reach::{
    is_proximity_only, is_trait_impl_method, owner_may_be_reached_unseen, reach_evidence,
};
pub(in crate::analysis) use related_tests::{
    DependencyEdgeContext, RelatedTestCandidateIndex, body_contains_owner_call,
    call_text_may_call_free_function, find_related_tests_with_candidate_index, impl_self_type_name,
    method_call_resolves_to_impl, owner_call_text, owner_dispatch_trait, package_prefix,
    test_calls_free_function,
};
pub(in crate::analysis) use reveal::reveal_outcome;
pub(in crate::analysis) use reveal::wrapper_error_seam_expression;
pub(in crate::analysis) use reveal::{
    ASSERTION_CONTEXT_UNESTABLISHED, FileUseStatements, oracle_crediting_relations,
};
pub(in crate::analysis) use reveal::{ReturnOracleAdmission, contains_as_whole_word};
// RIPR-SPEC-0106: re-export the variant parsers so test_grip_evidence.rs can
// apply variant-binding without reaching into the private `text` submodule.
pub(in crate::analysis) use text::{
    changed_error_variant, enum_variant_values, error_constructor_call_paths,
    error_constructor_payloads, error_result_payload_literal_sets, exact_error_variant,
    rust_string_literals,
};
// RIPR-SPEC-0114: bounded transitive-reach walk for Rust no_static_path findings.
// RIPR-SPEC-0115: the walk now returns a witness so the limitation can name the
// witnessing test (file:line) and the entry public-API symbol.
pub(in crate::analysis) use transitive_reach::{
    MAX_TRANSITIVE_DEPTH, MacroReachWitness, RUST_MACRO_REACH_MESSAGE,
    RUST_TRANSITIVE_REACH_MESSAGE, TransitiveReachIndex, TransitiveWitness, macro_reach_limit_kind,
    macro_reach_limitation_detail_lines, macro_reach_witness_pointer, transitive_reach_limit_kind,
    transitive_reach_limitation_detail_lines, transitive_reach_witness_pointer,
};
