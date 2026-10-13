<!-- section: Added -->
- `ripr pilot` and `ripr agent repair` now repair seams whose tests live in an
  inline `#[cfg(test)] mod tests` inside the source file, the `cargo new --lib`
  layout. Pilot prints the `ripr agent repair ... --phase before` command for
  such a seam when the file has exactly one governed inline test module, and
  the attempt confines the edit to that module: the after phase is compliant
  only when at least one new `#[test]` (or other recognised test-attribute)
  function is inserted into its body, with production code, the module
  declaration, existing tests and comments, and any staged or committed copy
  of the file unchanged or equal to the validated bytes. A helper function
  alone is not a repair, and a file matched by
  `languages.rust.generated_file_patterns` is refused at the before phase.
  The repair card's stop conditions name the module confinement. Any other
  change to the file fails the attempt as `outside_inline_test_region`. A file
  with no inline test module, two candidate modules, or an out-of-line
  `mod tests;` still gets no repair command, and the before phase refuses it
  with the `tests/` alternative (#5210).
