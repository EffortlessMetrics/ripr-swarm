<!-- section: Fixed -->
- Gate decision tests: the e2e project root is allocated from a process id and a
  per-process sequence instead of the wall clock, so two concurrent nextest
  processes can no longer share one temp directory and judge another test's
  scope; the collision surfaced as `cap_omitted_seams = 0` in
  `e2e_cap_omitted_seams_keep_in_scope_gap_not_evaluated` and removed an
  innocent PR from the merge queue (#14937).
