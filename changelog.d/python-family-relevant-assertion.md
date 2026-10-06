<!-- section: Fixed -->
- Python related-test rows now show the assertion relevant to the changed
  behavior instead of the strongest assertion in the test (RIPR-SPEC-0224).
  A changed `return` is judged by a value observer rather than a stronger
  `pytest.raises(..., match=...)`, a changed `raise` by the exception
  observer rather than a stronger normal-value assertion or mock call
  expectation, and a field change by an observer of the changed field ahead
  of a stronger exact assertion on a sibling field. The row, the
  `test_oracle` evidence, sink alignment and the boundary and changed-default gates all read that one
  assertion. When every assertion in a test observes another behavior
  family, the row shows no oracle and the evidence says
  `no_<family>_relevant_assertion`, instead of crediting the wrong-family
  assertion. Equal candidates are chosen by family, whole-value
  over `len(...)` observation and shape before source position, so
  reordering assertions does not change the result (#5572).
