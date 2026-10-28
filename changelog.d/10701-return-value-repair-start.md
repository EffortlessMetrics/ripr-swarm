<!-- section: Fixed -->
- `ripr repair` now starts from a weak test on a changed return value or error
  when a direct caller test with an editable target exists and reach, activation
  and propagation are all established; tests that reach the function only as a
  function value or inside an executor helper such as `block_on` are left alone.
  When no safe start exists, the refusal names the manual `ripr agent stub`
  route (#10701).
  The start is withheld, with the same plain reason, for a related `#[ignore]`
  or `#[should_panic]` test, a changed struct literal, a returned type with a
  non-plain `Drop` (including `Self`, an unresolved type alias, or a workspace
  holding a macro, `extern crate` or outside-crate item), an equality or
  closed-range comparison of the owner call or a value derived from it through
  `let` bindings, and an exact error-variant assertion for another variant.
  `ripr repair <seam>` does not apply the finding-level static-limited verdict
  that `ripr check` uses; it relies on the seam-level evidence and these gates.
