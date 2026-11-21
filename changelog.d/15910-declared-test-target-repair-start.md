<!-- section: Fixed -->
- `ripr check` and `ripr repair` now offer a repair start when the related
  test lives in a `[[test]]` target the manifest declares outside `tests/`
  (`path = "spec/pricing.rs"`, or a `Tests/` directory). The manifest is the
  authority: a declared path under `src/`, or one that also roots another
  target, still gets no start, and the attempt keeps the test-only edit rules
  (#15910).
