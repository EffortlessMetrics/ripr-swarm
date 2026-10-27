<!-- section: Fixed -->
- `ripr repair` now starts from a weak test on a changed return value or error
  when a direct caller test with an editable target exists and reach, activation
  and propagation are all established; tests that reach the function only as a
  function value or inside an executor helper such as `block_on` are left alone.
  When no safe start exists, the refusal names the manual `ripr agent stub`
  route (#10701).
