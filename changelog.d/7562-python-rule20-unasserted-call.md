<!-- section: Fixed -->
- Python rule 20 no longer treats a discarded owner call as activating a
  removed additive term. A strong related test that only asserts inputs
  where the term is 0 now reads `weakly_exposed` even when the same test
  also contains an unasserted call whose term is nonzero. Assigned, nested,
  or otherwise unproven-unobserved calls still block the downgrade
  (RIPR-SPEC-0238, #7562).
