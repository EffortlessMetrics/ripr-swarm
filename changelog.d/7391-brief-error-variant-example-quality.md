<!-- section: Fixed -->
- Agent briefs and seam packets: `error_variant` assertion examples are
  generated from typed parts instead of splicing the raw payload identity, so
  an owned string payload now binds by value and compares against the literal
  (`matches!(err, message if message == "...")`) with the `expect_err` label
  correctly escaped. Payloads that cannot appear in pattern position
  (`format!(...)`, other call payloads) no longer serialize a non-compiling
  example as `state: "concrete"`; the assertion shape downgrades to
  `unresolved` with `static_limitation_blocks_derivation` (#7391).
- Agent briefs: `verification.suggested_test_command` is `null` when the
  repair target is `not_applicable` instead of emitting
  `cargo test not_applicable`, a filter that matches zero tests and exits 0
  (#7391).
