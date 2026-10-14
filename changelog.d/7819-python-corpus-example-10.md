<!-- section: Changed -->
- Python verdict corpus: RIPR-SPEC-0238 example 10
  (`np.testing.assert_array_equal`) now has a case in a new numpy subject,
  so the corpus covers all 40 in-scope spec examples (floor 40). The case
  reads `limited` (`static_unknown`), as the spec expects for an unknown
  assertion helper (#7819).
