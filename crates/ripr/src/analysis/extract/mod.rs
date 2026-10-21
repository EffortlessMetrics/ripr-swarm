mod calls;
mod groups;
mod literals;
mod mask;
mod oracles;
pub(crate) mod property_macros;
mod returns;
mod shadow;
mod text;

pub(crate) use calls::{
    MAX_CALL_FACTS_PER_LINE, attribute_ranges, call_name_bounds_before_paren, extract_call_facts,
    has_saturated_line, line_at_call_budget, runs_its_arguments,
};
pub(crate) use groups::{delimiters_balance, group_end};
pub(crate) use literals::{extract_literal_facts, extract_literals};
// `string_literal_spans` offsets index the unmasked text: a literal's
// contents are `text[open + 1..close]`, escapes unprocessed (#7133).
pub(crate) use mask::{
    mask_comments_and_strings, mask_with_char_literals, mask_with_string_delimiter_quotes,
    rust_identity_spans, string_literal_spans,
};
#[cfg(test)]
pub(crate) use oracles::contains_macro_invocation;
pub(crate) use oracles::{
    OracleTextShape, assertion_oracle_text, classify_assertion, classify_assertion_in,
    equality_assertion_arguments, err_return_guard_oracles, err_return_guard_twin,
    extract_assertions, extract_assertions_in, extract_line_scanned_oracles,
    guarded_result_match_scan_with_shadow_authority, has_oracle_text_shape,
    is_inequality_assertion, is_unwrap_err_bound_error_assertion, outer_assertion_condition,
    retain_reached_oracles, terminal_err_return_guard_oracle, uncompared_unwrap_err,
    unwrap_err_bound_variables,
};
pub(crate) use returns::extract_return_facts;
pub(crate) use shadow::{
    ShadowAuthority, extract_pattern_words, fact_body_defines_callee_fn, fact_body_let_shadow_line,
    test_body_defines_callee_fn, test_body_let_shadow_line,
};
pub(crate) use text::extract_identifier_tokens;
