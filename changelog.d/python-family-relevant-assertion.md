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
- When the selected assertion changes a finding, its class or stage may move
  only where the old strength-only choice was another family's assertion or
  depended on assertion order; a changed raise with a matching
  `pytest.raises(..., match=...)` is now `exposed` whichever assertion comes
  last. Test-side static limits (opaque assertion helpers, property-based
  inputs) are no longer suppressed by a strong assertion of another family.
  The missing entry and observe summary name the kind of assertion the test
  does have instead of reporting the oracle as `unknown`. Repair cards whose
  rows have no family-relevant assertion (`alignment_reason:
  no_family_relevant_assertion`) or now show a different assertion than the
  strongest one (`other_behavior_assertion_passed_over`) are never delegated
  to an agent packet (#5572).
