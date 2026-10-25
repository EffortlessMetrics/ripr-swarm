mod adapter;
pub(crate) mod fn_signature;
pub(crate) mod lexical;
mod local_assertion_macros;
mod local_test_macros;
pub(crate) mod loop_accumulator;
mod macro_scope;
mod module_tree;
mod mut_parameter;
mod name_scope;
mod nesting;
mod owner_pin;
mod passthrough_item_macros;
mod pattern_pin;
pub(crate) mod run_memo;
pub(crate) use mut_parameter::mut_parameter_unwritten_at;
#[cfg(test)]
pub(crate) use owner_pin::COMPARISON_NESTED_MACROS;
pub(crate) use owner_pin::debug_assert_eq_calls;
pub(crate) use owner_pin::macro_binding_candidates;
pub(crate) use owner_pin::{
    AssertionContextRefusal, HelperLoan, MacroBindingCandidates, MacroBindingKind,
    MacroBindingSite, OwnerPinAssertions, TRUSTED_MACRO_NAMES, attribute_settles_test_outcome,
    constant_table_call_column, constant_table_column, constant_table_column_around,
    direct_literal_call_column, empty_local_macro_invocation_ranges,
    empty_macro_binding_ambiguities, err_return_guard_key, local_empty_macro_names,
    macro_binding_scan, macro_call_is_statement, node_has_macro_escape, owner_pin_assertions,
    owner_reference_count, returns_leave_the_function, skipping_table_column,
    source_binds_value_name, source_declares_value_name, supported_targeted_item_context,
    trusted_macro_binding_site_sets, trusted_macro_binding_sites, use_rebinds_crate_name,
};
pub(crate) use owner_pin::{decimal_u8, literal_check_syntax, literal_owner_syntax};
pub(crate) use pattern_pin::{PatternPins, pattern_pins};
pub(crate) mod ra;

pub use adapter::{
    LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter, SyntaxNodeFact, TextRange,
};
pub(crate) use local_assertion_macros::{LocalMacroCall, MacroArm, local_macro_calls};
#[cfg(test)]
pub(crate) use local_test_macros::expand_local_test_macros;
pub(crate) use local_test_macros::{
    GeneratedTestPins, expand_local_test_macros_with, generated_test_pins,
};
pub(crate) use macro_scope::{
    ImportedMacros, MacroScope, first_macro_call, generator_calls, macro_scope,
};
pub(crate) use module_tree::{RustModuleTreeEdge, RustModuleTreeScan, rust_module_tree_scan};
pub(crate) use name_scope::{
    NameScope, NameScopes, UseBinding, fn_call_has_local_binding, name_scopes_for_fn,
};
pub(crate) use nesting::{
    block_comment_only_lines, non_code_token_end, parse_clean_source_file, rust_nesting_refusal,
};
pub(crate) use ra::drop_in_assertion_leaf;
pub(crate) use ra::parser_oracles_for_function;
#[cfg(test)]
pub(crate) use ra::production_owner_module_path;
pub(crate) use ra::rust_include_directives;
pub(crate) use ra::{
    GovernedCfgTestModule, InvokedCallbackParam, ModuleItemScopes, closure_argument_positions,
    fn_name_binding_count, governed_cfg_test_modules, has_test_attribute,
    inline_unit_module_layout, invoked_callback_params, macro_call_columns, module_item_scopes,
    passes_call_making_closure,
};
