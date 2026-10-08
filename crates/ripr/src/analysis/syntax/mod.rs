mod adapter;
pub(crate) mod fn_signature;
pub(crate) mod lexical;
mod module_tree;
mod nesting;
mod owner_pin;
#[cfg(test)]
pub(crate) use owner_pin::macro_binding_candidates;
pub(crate) use owner_pin::{
    AssertionContextRefusal, MacroBindingCandidates, MacroBindingKind, MacroBindingSite,
    OwnerPinAssertions, TRUSTED_MACRO_NAMES, attribute_settles_test_outcome, constant_table_column,
    empty_macro_binding_ambiguities, local_empty_macro_names, macro_binding_scan,
    owner_pin_assertions, returns_leave_the_function, trusted_macro_binding_sites,
};
pub(crate) mod ra;

pub use adapter::{
    LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter, SyntaxNodeFact, TextRange,
};
pub(crate) use module_tree::{RustModuleTreeEdge, RustModuleTreeScan, rust_module_tree_scan};
pub(crate) use nesting::{non_code_token_end, parse_clean_source_file, rust_nesting_refusal};
pub(crate) use ra::parser_oracles_for_function;
#[cfg(test)]
pub(crate) use ra::production_owner_module_path;
pub(crate) use ra::rust_include_directives;
pub(crate) use ra::{
    GovernedCfgTestModule, ModuleItemScopes, fn_name_binding_count, governed_cfg_test_modules,
    inline_unit_module_layout, module_item_scopes,
};
