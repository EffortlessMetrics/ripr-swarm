# Returned-closure acceptance matrix

Executed analyzer build `d2d981acb38ffa5285824a438a0f623a292842a1` over canonical `b4f04013c237dd034a5009dbfd46d45bef7e4733`.
Frozen CLI SHA256 `b7d15596ad6728e70590a53399b85c1998b3cc6cadedbff954b17d6cbb7cba2a`.

Each row compiles original and independently specified mutant source, then executes native tests. `101` is a failed Rust assertion, not compilation. Every original passes. Conservative unknown controls may reject a mutant but remain outside the bounded admission.

| Case | Analyzer | Expected exposure | Original | Mutant |
|---|---|---:|---:|---:|
| direct | exposed | True | 0 | 101 |
| qualified_despite_rebinding | exposed | True | 0 | 101 |
| unused | weakly_exposed | False | 0 | 0 |
| bound_and_applied | weakly_exposed | False | 0 | 101 |
| rebound_callee | static_unknown | False | 0 | 0 |
| nested_fn_twin | static_unknown | False | 0 | 0 |
| foreign_qualified_twin | weakly_exposed | False | 0 | 0 |
| foreign_imported_twin | static_unknown | False | 0 | 0 |
| zero_capture | weakly_exposed | False | 0 | 0 |
| nonliteral_capture | static_unknown | False | 0 | 101 |
| captured_offset_zero | static_unknown | False | 0 | 0 |
| captured_offset_identity | static_unknown | False | 0 | 0 |
| body_literal_zero | static_unknown | False | 0 | 0 |
| constant_value_positive | exposed | True | 0 | 101 |
| uppercase_callee | static_unknown | False | 0 | 0 |
| existing_unit_positive | static_unknown | False | 0 | 0 |
| existing_unit_negative | static_unknown | False | 0 | 0 |
| existing_coincident_pair | static_unknown | False | 0 | 0 |
| existing_underscore_identifier | static_unknown | False | 0 | 0 |
| long_token_alias_limit | static_unknown | False | 0 | 0 |
| long_token_positive | exposed | True | 0 | 101 |
| long_token_unit_collision | static_unknown | False | 0 | 0 |
| long_token_unapplied | weakly_exposed | False | 0 | 0 |
| confirming_token_unapplied | weakly_exposed | False | 0 | 0 |
| confirming_token_unit_collision | static_unknown | False | 0 | 0 |
| parameter_shadow | static_unknown | False | 0 | 0 |
| signed_negative_positive | exposed | True | 0 | 101 |
| addition_unit_positive | exposed | True | 0 | 101 |
| unsigned_positive | exposed | True | 0 | 101 |
| signed_division_positive | exposed | True | 0 | 101 |
| float_positive | exposed | True | 0 | 101 |
| float_collision | static_unknown | False | 0 | 0 |
| float32_round_positive | exposed | True | 0 | 101 |
| float32_round_collision | static_unknown | False | 0 | 0 |
| sibling_constant_positive | exposed | True | 0 | 101 |
| raw_primitive_alias_limit | static_unknown | False | 0 | 101 |
| unrelated_qualified_primitive_positive | exposed | True | 0 | 101 |
| primitive_alias_limit | static_unknown | False | 0 | 101 |
| parent_primitive_alias_positive | weakly_exposed | False | 0 | 101 |
| raw_parameter_shadow | static_unknown | False | 0 | 0 |
| raw_constant_positive | exposed | True | 0 | 101 |
| renamed_primitive_positive | exposed | True | 0 | 101 |
| unused_trait_positive | exposed | True | 0 | 101 |
| simple_import_alias_limit | static_unknown | False | 0 | 101 |
| commented_import_alias_limit | static_unknown | False | 0 | 101 |
| raw_import_alias_limit | static_unknown | False | 0 | 101 |
| parent_alias_bare_positive | exposed | True | 0 | 101 |
| grouped_crate_import_positive | exposed | True | 0 | 101 |
| grouped_self_import_positive | exposed | True | 0 | 101 |
| computed_constant_limit | static_unknown | False | 0 | 101 |
| narrow_overflow_limit | static_unknown | False | 0 | 101 |

Four body/capture/helper cases independently establish non-equivalence at another input while their pin assertions agree. All four witness tests pass.

Limits: fixed-width homogeneous primitives, bounded literal/parameter arithmetic, literal local-scope captures, <=8 binary nodes; unsupported aliases/computed constants/casts/overflow/nonfinite cases withhold. Same-node operator replacements preserve grouping and operand order. This is bounded static discrimination, not mutation adequacy. The inherited outer `factory::` pin remains weakly_exposed; bare calls from a child test module remain supported.

The original canonical e184 review corpus and 68be intermediate receipts remain retained under the parent proof directories. The 8a6e private-module wiring failed compilation and did not execute tests; d2d9 repairs that wiring.
