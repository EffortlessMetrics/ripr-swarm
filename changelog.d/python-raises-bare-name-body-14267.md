<!-- section: Fixed -->
- Python: a `pytest.raises` test whose `with` body never calls the changed
  function no longer reads `exposed` for that function's changed `raise`
  when the function is named only before the `with`, in the comparison
  after it, in its `match=` pattern, or inside a string in the body. The
  bare-name, alias and changed-token credit for a free-function owner now
  reads only the exception body, like the module-qualified call already
  did (#14267). The body reader now also distinguishes code from text that
  merely looks like it: a `def ` or `class ` spelled inside a body string
  defers no call, and a replacement field of an f-string is read as code
  (#14267 review).
