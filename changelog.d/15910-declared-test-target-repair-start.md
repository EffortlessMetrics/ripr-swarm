<!-- section: Fixed -->
- `ripr check` and `ripr repair` now offer a repair start when the related
  test lives in a `[[test]]` target the manifest declares outside `tests/`
  (`path = "spec/pricing.rs"`, or a `Tests/` directory). The manifest is the
  authority: a declared path under `src/`, or one that also roots another
  target, still gets no start, and the attempt keeps the test-only edit rules.
  `ripr agent stub`, the MCP repair tools and the printed `cargo test --test`
  command name the same declared file and its manifest target name, including
  for a file name with a literal `%` (#15910).
