<!-- section: Fixed -->
- A panic inside one language adapter (or an AST consumer it drives) no
  longer takes the whole run with it (#11132, split from #10724). A preview
  adapter (TypeScript, JavaScript, Python, Perl) that panics is recorded as a
  `Failed` language run with a typed `adapter_panicked` reason in diff and repo
  scope; the other languages' findings are kept and the failed adapter
  contributes no finding, so nothing from it can be an actionable repair
  signal. A Rust adapter panic becomes the typed analysis failure Rust failures
  already were. A parse-worker failure while extracting an unchanged
  TypeScript test file is now disclosed as the typed
  `typescript_test_extraction_partial` limitation instead of silently dropping
  that file's tests (which could read as a false `no_static_path`). Stack
  exhaustion is not an unwind and is not caught: the per-language nesting
  budgets keep that class out of the parsers.
