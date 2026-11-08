<!-- section: Fixed -->
- Diagnostic catalog self-checks are now real: closed-vocabulary labels must be
  distinct, non-empty snake_case (compile-time assert), and an entry's recovery
  declaration must be duplicate-free with a typed unavailable reason exactly
  when it lists no recovery. Both have negative tests. Emitted codes are
  unchanged (#12832).
