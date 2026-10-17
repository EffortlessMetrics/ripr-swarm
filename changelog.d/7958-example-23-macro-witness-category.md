<!-- section: Fixed -->
- A rust-macro-reach seam limitation (`rust_macro_reach_unresolved` or
  `rust_macro_wrapped_test_call_unresolved`) now keeps evidence-record
  category `opaque_static_evidence` instead of inheriting
  `macro_generated_value` from the word "macro" in the summary (RIPR-SPEC-0236
  example 23; #7958).
