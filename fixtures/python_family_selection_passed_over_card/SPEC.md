# python_family_selection_passed_over_card

Python preview fixture for #6910: a changed return whose related test holds a
strong exception assertion and a weaker value assertion. Family selection shows
the value assertion on the row and surfaces `alignment_reason:
other_behavior_assertion_passed_over` with the strength-only reason retained in
`superseded_alignment_reason`. The repair card stays advisory (never delegated).
